mod gui_fonts;
mod hooks;
mod protocol;
mod server;
mod session;
mod terminal_gui;
mod transport;
mod version_check;

mod app {
    use std::{
        collections::BTreeMap,
        env,
        ffi::{OsStr, OsString},
        io::{self, Write},
        process::Command,
    };

    use anyhow::{Context, Result, bail};
    use serde::Serialize;

    use crate::{
        hooks,
        protocol::{Request, ResponseEnvelope, ResponseResult, WaitCondition},
        server, session, terminal_gui, transport,
    };

    struct TerminalOptions {
        session: Option<String>,
        cwd: Option<String>,
        prompt: Option<String>,
        model: Option<String>,
        owner: Option<String>,
        always_approve: bool,
    }

    enum Action {
        Rpc { request: Request, auto_start: bool },
        Terminal(TerminalOptions),
        Hooks(HooksAction),
        ServerUi,
        Doctor,
        Help,
        Version,
        InternalHook,
        InternalServer,
    }

    enum HooksAction {
        Install,
        Status,
        Uninstall,
    }

    pub(super) fn main() {
        let exit_code = match run() {
            Ok(exit_code) => exit_code,
            Err(error) => {
                eprintln!("grok-bridge: {error:#}");
                1
            }
        };
        std::process::exit(exit_code);
    }

    fn run() -> Result<i32> {
        match parse_args(env::args_os().skip(1).collect())? {
            Action::Rpc {
                request,
                auto_start,
            } => {
                let response = transport::call(request, auto_start)?;
                write_json(&response)?;
                Ok(if response.ok { 0 } else { 1 })
            }
            Action::Terminal(options) => {
                let session = terminal_session(options)?;
                terminal_gui::run(session)?;
                Ok(0)
            }
            Action::Hooks(action) => {
                let status = match action {
                    HooksAction::Install => hooks::install()?,
                    HooksAction::Status => hooks::status()?,
                    HooksAction::Uninstall => hooks::uninstall()?,
                };
                write_json(&status)?;
                Ok(0)
            }
            Action::ServerUi => server_ui(),
            Action::Doctor => doctor(),
            Action::Help => {
                print_help();
                Ok(0)
            }
            Action::Version => {
                println!("grok-bridge {}", env!("CARGO_PKG_VERSION"));
                Ok(0)
            }
            Action::InternalHook => {
                hooks::run_ingress_fail_open();
                Ok(0)
            }
            Action::InternalServer => {
                server::run()?;
                Ok(0)
            }
        }
    }

    fn parse_args(arguments: Vec<OsString>) -> Result<Action> {
        let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
            return Ok(Action::Help);
        };
        match command {
            "__hook" => {
                ensure_no_arguments(&arguments[1..])?;
                Ok(Action::InternalHook)
            }
            "__server" => {
                ensure_no_arguments(&arguments[1..])?;
                Ok(Action::InternalServer)
            }
            "hooks" => parse_hooks(&arguments[1..]),
            "server" => parse_server(&arguments[1..]),
            "create" => parse_create(&arguments[1..]),
            "list" => {
                ensure_no_arguments(&arguments[1..])?;
                rpc(Request::List, true)
            }
            "heartbeat" => {
                ensure_no_arguments(&arguments[1..])?;
                rpc(Request::Heartbeat, true)
            }
            "close-codex" => {
                ensure_no_arguments(&arguments[1..])?;
                rpc(Request::CloseCodex, true)
            }
            "show" => {
                let options = parse_options(&arguments[1..], &["--session"])?;
                rpc(
                    Request::Show {
                        session: required(&options, "--session")?.to_owned(),
                    },
                    true,
                )
            }
            "read" => parse_read(&arguments[1..]),
            "send" => parse_send(&arguments[1..]),
            "write" => parse_write(&arguments[1..]),
            "resize" => parse_resize(&arguments[1..]),
            "wait" => parse_wait(&arguments[1..]),
            "close" => {
                let options = parse_options(&arguments[1..], &["--session"])?;
                rpc(
                    Request::Close {
                        session: required(&options, "--session")?.to_owned(),
                    },
                    true,
                )
            }
            "terminal" => parse_terminal(&arguments[1..]),
            "doctor" | "--doctor" => {
                ensure_no_arguments(&arguments[1..])?;
                Ok(Action::Doctor)
            }
            "help" | "--help" | "-h" => {
                ensure_no_arguments(&arguments[1..])?;
                Ok(Action::Help)
            }
            "--version" | "-V" => {
                ensure_no_arguments(&arguments[1..])?;
                Ok(Action::Version)
            }
            other => bail!("unknown command: {other}"),
        }
    }

    fn parse_hooks(arguments: &[OsString]) -> Result<Action> {
        let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
            bail!("hooks requires install, status, or uninstall");
        };
        ensure_no_arguments(&arguments[1..])?;
        let action = match command {
            "install" => HooksAction::Install,
            "status" => HooksAction::Status,
            "uninstall" => HooksAction::Uninstall,
            other => bail!("unknown hooks command: {other}"),
        };
        Ok(Action::Hooks(action))
    }

    fn parse_server(arguments: &[OsString]) -> Result<Action> {
        let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
            bail!("server requires start, status, stop, or ui");
        };
        ensure_no_arguments(&arguments[1..])?;
        match command {
            "start" => rpc(Request::ServerStatus, true),
            "status" => rpc(Request::ServerStatus, false),
            "stop" => rpc(Request::ServerStop, false),
            "ui" => Ok(Action::ServerUi),
            other => bail!("unknown server command: {other}"),
        }
    }

    fn parse_create(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(
            arguments,
            &[
                "--cwd",
                "--prompt",
                "--model",
                "--owner",
                "--always-approve",
            ],
        )?;
        let cwd = match options.get("--cwd") {
            Some(cwd) => cwd.clone(),
            None => env::current_dir()
                .context("failed to determine the current working directory")?
                .to_string_lossy()
                .into_owned(),
        };
        rpc(
            Request::Create {
                cwd,
                prompt: options.get("--prompt").cloned(),
                model: options.get("--model").cloned(),
                owner: options.get("--owner").cloned().or_else(default_owner),
                always_approve: options.contains_key("--always-approve"),
            },
            true,
        )
    }

    fn parse_terminal(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(
            arguments,
            &[
                "--session",
                "--cwd",
                "--prompt",
                "--model",
                "--owner",
                "--always-approve",
            ],
        )?;
        let session = options.get("--session").cloned();
        if session.is_some()
            && [
                "--cwd",
                "--prompt",
                "--model",
                "--owner",
                "--always-approve",
            ]
            .iter()
            .any(|name| options.contains_key(*name))
        {
            bail!("--session cannot be combined with session creation options");
        }
        Ok(Action::Terminal(TerminalOptions {
            session,
            cwd: options.get("--cwd").cloned(),
            prompt: options.get("--prompt").cloned(),
            model: options.get("--model").cloned(),
            owner: options.get("--owner").cloned().or_else(default_owner),
            always_approve: options.contains_key("--always-approve"),
        }))
    }

    fn terminal_session(options: TerminalOptions) -> Result<String> {
        if let Some(session) = options.session {
            let response = transport::call(
                Request::Show {
                    session: session.clone(),
                },
                true,
            )?;
            ensure_success(&response)?;
            return Ok(session);
        }

        let cwd = match options.cwd {
            Some(cwd) => cwd,
            None => env::current_dir()
                .context("failed to determine the current working directory")?
                .to_string_lossy()
                .into_owned(),
        };
        let response = transport::call(
            Request::Create {
                cwd,
                prompt: options.prompt,
                model: options.model,
                owner: options.owner,
                always_approve: options.always_approve,
            },
            true,
        )?;
        ensure_success(&response)?;
        match response.result {
            Some(ResponseResult::Session(state)) => Ok(state.session),
            _ => bail!("runtime returned an unexpected create response"),
        }
    }

    fn default_owner() -> Option<String> {
        ["CODEX_THREAD_ID", "CODEX_SESSION_ID"]
            .into_iter()
            .find_map(|name| env::var(name).ok().filter(|value| !value.trim().is_empty()))
    }

    fn server_ui() -> Result<i32> {
        let response = transport::call(Request::ServerStatus, true)?;
        ensure_success(&response)?;
        match response.result {
            Some(ResponseResult::ServerInfo(info)) => match info.web_url {
                Some(url) => {
                    open_browser(&url)?;
                    eprintln!("grok-bridge WebUI: {url}");
                    Ok(0)
                }
                None => bail!("runtime WebUI is unavailable; check server stderr"),
            },
            _ => bail!("runtime returned an unexpected server status response"),
        }
    }

    fn open_browser(url: &str) -> Result<()> {
        #[cfg(windows)]
        let mut command = Command::new("explorer.exe");
        #[cfg(target_os = "macos")]
        let mut command = Command::new("open");
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut command = Command::new("xdg-open");
        #[cfg(not(any(windows, unix)))]
        bail!("opening a browser is unsupported on this platform; visit {url}");
        command
            .arg(url)
            .spawn()
            .context("failed to open the Runtime WebUI in the default browser")?;
        Ok(())
    }

    fn ensure_success(response: &ResponseEnvelope) -> Result<()> {
        if response.ok {
            return Ok(());
        }
        match &response.error {
            Some(error) => bail!("{}: {}", error.code, error.message),
            None => bail!("runtime returned an unsuccessful response without an error"),
        }
    }

    fn parse_read(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(
            arguments,
            &["--session", "--cursor", "--limit", "--wait-ms"],
        )?;
        rpc(
            Request::Read {
                session: required(&options, "--session")?.to_owned(),
                cursor: parse_number(&options, "--cursor")?,
                limit: parse_number(&options, "--limit")?,
                wait_ms: parse_number(&options, "--wait-ms")?,
            },
            true,
        )
    }

    fn parse_send(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(arguments, &["--session", "--text", "--interrupt"])?;
        let input = if options.contains_key("--interrupt") {
            if options.contains_key("--text") {
                bail!("--interrupt cannot be combined with --text");
            }
            "\u{3}".to_owned()
        } else {
            required(&options, "--text")?.to_owned()
        };
        rpc(
            Request::Send {
                session: required(&options, "--session")?.to_owned(),
                input,
            },
            true,
        )
    }

    fn parse_write(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(arguments, &["--session", "--data-base64"])?;
        rpc(
            Request::Write {
                session: required(&options, "--session")?.to_owned(),
                data_base64: required(&options, "--data-base64")?.to_owned(),
            },
            true,
        )
    }

    fn parse_resize(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(arguments, &["--session", "--cols", "--rows"])?;
        rpc(
            Request::Resize {
                session: required(&options, "--session")?.to_owned(),
                cols: parse_required_number(&options, "--cols")?,
                rows: parse_required_number(&options, "--rows")?,
            },
            true,
        )
    }

    fn parse_wait(arguments: &[OsString]) -> Result<Action> {
        let options = parse_options(arguments, &["--session", "--for", "--timeout-ms"])?;
        let condition = match required(&options, "--for")? {
            "tui-idle" => WaitCondition::TuiIdle,
            "exit" => WaitCondition::Exit,
            value => bail!("unsupported wait condition: {value}"),
        };
        rpc(
            Request::Wait {
                session: required(&options, "--session")?.to_owned(),
                for_condition: condition,
                timeout_ms: parse_number(&options, "--timeout-ms")?,
            },
            true,
        )
    }

    fn rpc(request: Request, auto_start: bool) -> Result<Action> {
        Ok(Action::Rpc {
            request,
            auto_start,
        })
    }

    fn parse_options(arguments: &[OsString], allowed: &[&str]) -> Result<BTreeMap<String, String>> {
        let mut options = BTreeMap::new();
        let mut index = 0;
        while index < arguments.len() {
            let name = arguments[index]
                .to_str()
                .context("command options must be valid Unicode")?;
            if !allowed.contains(&name) {
                bail!("unknown option: {name}");
            }
            if options.contains_key(name) {
                bail!("option was provided more than once: {name}");
            }
            if matches!(name, "--always-approve" | "--interrupt") {
                options.insert(name.to_owned(), String::new());
                index += 1;
                continue;
            }
            let value = arguments
                .get(index + 1)
                .with_context(|| format!("{name} requires a value"))?
                .to_str()
                .with_context(|| format!("{name} must be valid Unicode"))?;
            if value.is_empty() {
                bail!("{name} requires a non-empty value");
            }
            options.insert(name.to_owned(), value.to_owned());
            index += 2;
        }
        Ok(options)
    }

    fn required<'a>(options: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
        options
            .get(name)
            .map(String::as_str)
            .with_context(|| format!("missing required option: {name}"))
    }

    fn parse_number<T>(options: &BTreeMap<String, String>, name: &str) -> Result<Option<T>>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        options
            .get(name)
            .map(|value| {
                value
                    .parse::<T>()
                    .map_err(|error| anyhow::anyhow!("invalid value for {name}: {value}: {error}"))
            })
            .transpose()
    }

    fn parse_required_number<T>(options: &BTreeMap<String, String>, name: &str) -> Result<T>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let value = required(options, name)?;
        value
            .parse::<T>()
            .map_err(|error| anyhow::anyhow!("invalid value for {name}: {value}: {error}"))
    }

    fn ensure_no_arguments(arguments: &[OsString]) -> Result<()> {
        if arguments.is_empty() {
            Ok(())
        } else {
            bail!("unexpected arguments")
        }
    }

    fn write_json(value: &impl Serialize) -> Result<()> {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        serde_json::to_writer(&mut output, value).context("failed to write JSON response")?;
        output
            .write_all(b"\n")
            .context("failed to finish JSON response")
    }

    fn doctor() -> Result<i32> {
        let grok = env::var_os("GROK_BIN").unwrap_or_else(session::default_grok_bin);
        let output = Command::new(&grok)
            .arg("--version")
            .output()
            .with_context(|| {
                format!(
                    "failed to start {}; set GROK_BIN to the Grok executable",
                    OsStr::new(&grok).to_string_lossy()
                )
            })?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            bail!("Grok version check failed with {}", output.status);
        }
        match transport::call(Request::ServerStatus, false) {
            Ok(response) => write_json(&response)?,
            Err(_) => println!("runtime_server=stopped"),
        }
        Ok(0)
    }

    fn print_help() {
        println!(
            "grok-bridge {}\n\nUSAGE:\n  grok-bridge hooks install|status|uninstall\n  grok-bridge server start|status|stop|ui\n  grok-bridge create [--cwd <path>] [--prompt <text>] [--model <model>] [--owner <title>] [--always-approve]\n  grok-bridge list\n  grok-bridge heartbeat\n  grok-bridge close-codex\n  grok-bridge show --session <handle>\n  grok-bridge read --session <handle> [--cursor <n>] [--limit <bytes>] [--wait-ms <n>]\n  grok-bridge send --session <handle> (--text <text> | --interrupt)\n  grok-bridge write --session <handle> --data-base64 <base64>\n  grok-bridge resize --session <handle> --cols <n> --rows <n>\n  grok-bridge wait --session <handle> --for tui-idle|exit [--timeout-ms <n>]\n  grok-bridge close --session <handle>\n  grok-bridge terminal --session <handle>\n  grok-bridge terminal [--cwd <path>] [--prompt <text>] [--model <model>] [--owner <title>] [--always-approve]\n  grok-bridge doctor\n\n`hooks install` adds the managed Grok lifecycle hooks used for accurate working, waiting, and idle status. `server ui` starts the singleton Runtime and opens its localhost WebUI. Session RPCs refresh the current CODEX_THREAD_ID/CODEX_SESSION_ID lease; `heartbeat` refreshes it explicitly and `close-codex` closes only that Codex session's Grok processes. `terminal --session` attaches to an existing persistent PTY session. Without `--session`, it creates a session and opens the terminal GUI. Closing either UI only detaches; only an explicit close action or expired safe-session lease terminates Grok. RPC commands print one JSON response; interactive UI commands are exceptions. Every session command auto-starts one per-user Runtime Server when needed.",
            env!("CARGO_PKG_VERSION")
        );
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_wait_and_interrupt_commands() {
            assert!(matches!(
                parse_args(vec!["heartbeat".into()]).unwrap(),
                Action::Rpc {
                    request: Request::Heartbeat,
                    ..
                }
            ));
            assert!(matches!(
                parse_args(vec!["close-codex".into()]).unwrap(),
                Action::Rpc {
                    request: Request::CloseCodex,
                    ..
                }
            ));
            let action = parse_args(vec![
                "wait".into(),
                "--session".into(),
                "gbt-1".into(),
                "--for".into(),
                "tui-idle".into(),
            ])
            .unwrap();
            assert!(matches!(
                action,
                Action::Rpc {
                    request: Request::Wait {
                        for_condition: WaitCondition::TuiIdle,
                        ..
                    },
                    ..
                }
            ));

            let action = parse_args(vec![
                "send".into(),
                "--session".into(),
                "gbt-1".into(),
                "--interrupt".into(),
            ])
            .unwrap();
            assert!(matches!(
                action,
                Action::Rpc {
                    request: Request::Send { input, .. },
                    ..
                } if input == "\u{3}"
            ));
        }

        #[test]
        fn parses_hook_management_and_hidden_ingress_commands() {
            assert!(matches!(
                parse_args(vec!["hooks".into(), "install".into()]).unwrap(),
                Action::Hooks(HooksAction::Install)
            ));
            assert!(matches!(
                parse_args(vec!["hooks".into(), "status".into()]).unwrap(),
                Action::Hooks(HooksAction::Status)
            ));
            assert!(matches!(
                parse_args(vec!["hooks".into(), "uninstall".into()]).unwrap(),
                Action::Hooks(HooksAction::Uninstall)
            ));
            assert!(matches!(
                parse_args(vec!["__hook".into()]).unwrap(),
                Action::InternalHook
            ));
            assert!(parse_args(vec!["__hook".into(), "extra".into()]).is_err());
        }

        #[test]
        fn rejects_duplicate_and_conflicting_options() {
            assert!(
                parse_args(vec![
                    "show".into(),
                    "--session".into(),
                    "one".into(),
                    "--session".into(),
                    "two".into(),
                ])
                .is_err()
            );
            assert!(
                parse_args(vec![
                    "send".into(),
                    "--session".into(),
                    "one".into(),
                    "--text".into(),
                    "hello".into(),
                    "--interrupt".into(),
                ])
                .is_err()
            );
            assert!(
                parse_args(vec![
                    "terminal".into(),
                    "--session".into(),
                    "one".into(),
                    "--prompt".into(),
                    "new task".into(),
                ])
                .is_err()
            );
        }

        #[test]
        fn parses_terminal_attach_and_create_modes() {
            let attach =
                parse_args(vec!["terminal".into(), "--session".into(), "gbt-1".into()]).unwrap();
            assert!(matches!(
                attach,
                Action::Terminal(TerminalOptions {
                    session: Some(session),
                    ..
                }) if session == "gbt-1"
            ));

            let create = parse_args(vec![
                "terminal".into(),
                "--prompt".into(),
                "修复中文".into(),
                "--always-approve".into(),
            ])
            .unwrap();
            assert!(matches!(
                create,
                Action::Terminal(TerminalOptions {
                    session: None,
                    prompt: Some(prompt),
                    always_approve: true,
                    ..
                }) if prompt == "修复中文"
            ));
        }
    }
}

fn main() {
    app::main();
}
