use std::{
    env,
    ffi::OsString,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::timeout};

const DEFAULT_TIMEOUT_SECONDS: u64 = 1_800;
const MAX_TIMEOUT_SECONDS: u64 = 7_200;
const MAX_OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
struct GrokBuildArgs {
    prompt: String,
    cwd: String,
    session_id: Option<String>,
    timeout_seconds: Option<u64>,
    auto_approve: Option<bool>,
    model: Option<String>,
}

#[derive(Debug, Serialize)]
struct GrokBuildResult {
    success: bool,
    exit_code: Option<i32>,
    timed_out: bool,
    session_id: Option<String>,
    command: Vec<String>,
    cwd: String,
    stdout: String,
    stderr: String,
    output_truncated: bool,
    error: Option<String>,
}

async fn run_grok(args: GrokBuildArgs) -> Result<GrokBuildResult> {
    validate_prompt(&args.prompt)?;
    let cwd = canonical_directory(&args.cwd)?;
    ensure_allowed_root(&cwd)?;

    if let Some(session_id) = args.session_id.as_deref() {
        validate_session_id(session_id)?;
    }
    if let Some(model) = args.model.as_deref() {
        validate_model(model)?;
    }

    let grok_binary = env::var_os("GROK_BIN").unwrap_or_else(|| OsString::from("grok"));
    let timeout_seconds = args
        .timeout_seconds
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .clamp(10, MAX_TIMEOUT_SECONDS);

    let cli_args = build_grok_args(&cwd, &args);

    let mut command = Command::new(&grok_binary);
    command
        .args(&cli_args)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let command_for_result = std::iter::once(grok_binary.to_string_lossy().into_owned())
        .chain(cli_args.iter().cloned())
        .collect::<Vec<_>>();

    let output = match timeout(Duration::from_secs(timeout_seconds), command.output()).await {
        Ok(output) => output.context(
            "failed to start Grok Build; set GROK_BIN to the full executable path if `grok` is not on PATH",
        )?,
        Err(_) => {
            return Ok(GrokBuildResult {
                success: false,
                exit_code: None,
                timed_out: true,
                session_id: args.session_id,
                command: redact_prompt(command_for_result),
                cwd: cwd.to_string_lossy().into_owned(),
                stdout: String::new(),
                stderr: String::new(),
                output_truncated: false,
                error: Some(format!("Grok Build exceeded {timeout_seconds} seconds")),
            });
        }
    };

    let session_id = extract_session_id(&output.stdout).or(args.session_id);
    let (stdout, stdout_truncated) = decode_and_clip(&output.stdout);
    let (stderr, stderr_truncated) = decode_and_clip(&output.stderr);

    Ok(GrokBuildResult {
        success: output.status.success(),
        exit_code: output.status.code(),
        timed_out: false,
        session_id,
        command: redact_prompt(command_for_result),
        cwd: cwd.to_string_lossy().into_owned(),
        stdout,
        stderr,
        output_truncated: stdout_truncated || stderr_truncated,
        error: if output.status.success() {
            None
        } else {
            Some("Grok Build exited with a non-zero status".to_owned())
        },
    })
}

fn build_grok_args(cwd: &Path, args: &GrokBuildArgs) -> Vec<String> {
    let mut cli_args = vec![
        "--no-auto-update".to_owned(),
        "--no-alt-screen".to_owned(),
        "--cwd".to_owned(),
        cwd.to_string_lossy().into_owned(),
        "--output-format".to_owned(),
        "json".to_owned(),
    ];

    if args.auto_approve.unwrap_or(false) {
        cli_args.push("--always-approve".to_owned());
    }
    if let Some(model) = args.model.as_deref() {
        cli_args.push("--model".to_owned());
        cli_args.push(model.to_owned());
    }
    if let Some(session_id) = args.session_id.as_deref() {
        cli_args.push("--resume".to_owned());
        cli_args.push(session_id.to_owned());
    }
    cli_args.push("-p".to_owned());
    cli_args.push(args.prompt.clone());
    cli_args
}

fn validate_prompt(prompt: &str) -> Result<()> {
    if prompt.trim().is_empty() {
        bail!("prompt must not be empty");
    }
    if prompt.len() > 256 * 1024 {
        bail!("prompt is too large; maximum size is 256 KiB");
    }
    Ok(())
}

fn canonical_directory(raw: &str) -> Result<PathBuf> {
    let path = std::fs::canonicalize(raw)
        .with_context(|| format!("cwd does not exist or cannot be resolved: {raw}"))?;
    if !path.is_dir() {
        bail!("cwd is not a directory: {}", path.display());
    }
    Ok(path)
}

fn ensure_allowed_root(cwd: &Path) -> Result<()> {
    let Some(raw_roots) = env::var_os("GROK_BRIDGE_ALLOWED_ROOTS") else {
        return Ok(());
    };

    let roots = env::split_paths(&raw_roots)
        .filter_map(|path| std::fs::canonicalize(path).ok())
        .collect::<Vec<_>>();

    if roots.is_empty() {
        bail!("GROK_BRIDGE_ALLOWED_ROOTS was set, but none of its paths could be resolved");
    }

    if roots.iter().any(|root| cwd.starts_with(root)) {
        return Ok(());
    }

    bail!(
        "cwd is outside GROK_BRIDGE_ALLOWED_ROOTS: {}",
        cwd.display()
    )
}

fn validate_session_id(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let valid = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        });

    if !valid {
        bail!("session_id must be a canonical UUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)");
    }
    Ok(())
}

fn validate_model(value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 120
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        });
    if !valid {
        bail!("model contains unsupported characters");
    }
    Ok(())
}

fn decode_and_clip(bytes: &[u8]) -> (String, bool) {
    if bytes.len() <= MAX_OUTPUT_BYTES {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }

    let clipped = &bytes[..MAX_OUTPUT_BYTES];
    (
        format!(
            "{}\n...[output truncated by grok-bridge]",
            String::from_utf8_lossy(clipped)
        ),
        true,
    )
}

fn redact_prompt(mut command: Vec<String>) -> Vec<String> {
    if let Some(index) = command.iter().position(|item| item == "-p")
        && let Some(prompt) = command.get_mut(index + 1)
    {
        *prompt = format!("<prompt: {} bytes>", prompt.len());
    }
    command
}

fn extract_session_id(stdout: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let session_id = value.as_object()?.get("sessionId")?.as_str()?;
    validate_session_id(session_id).ok()?;
    Some(session_id.to_owned())
}

fn parse_request(input: &[u8]) -> Result<GrokBuildArgs> {
    serde_json::from_slice(input).context("STDIN must contain one UTF-8 JSON GrokBuildArgs object")
}

fn read_request() -> Result<GrokBuildArgs> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .context("failed to read request from STDIN")?;
    parse_request(&input)
}

fn error_result(error: String) -> GrokBuildResult {
    GrokBuildResult {
        success: false,
        exit_code: None,
        timed_out: false,
        session_id: None,
        command: Vec::new(),
        cwd: String::new(),
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        error: Some(error),
    }
}

async fn stdio_request() -> Result<()> {
    let result = match read_request() {
        Ok(args) => match run_grok(args).await {
            Ok(result) => result,
            Err(error) => {
                eprintln!("grok-bridge: {error:#}");
                error_result(format!("{error:#}"))
            }
        },
        Err(error) => {
            eprintln!("grok-bridge: {error:#}");
            error_result(format!("{error:#}"))
        }
    };

    let mut output = serde_json::to_vec(&result).context("failed to serialize Grok result")?;
    output.push(b'\n');
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&output)
        .context("failed to write Grok result to STDOUT")?;
    stdout.flush().context("failed to flush STDOUT")
}

async fn doctor() -> Result<()> {
    let grok_binary = env::var_os("GROK_BIN").unwrap_or_else(|| OsString::from("grok"));
    let output = Command::new(&grok_binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .await
        .with_context(|| "could not run Grok Build; install it or set GROK_BIN")?;

    println!("grok-bridge: {}", env!("CARGO_PKG_VERSION"));
    println!("grok binary: {}", grok_binary.to_string_lossy());
    println!("grok exit code: {:?}", output.status.code());
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        bail!("Grok Build version check failed");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();

    if let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "doctor" | "--doctor" => return doctor().await,
            "--version" | "-V" => {
                println!("grok-bridge {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "grok-bridge {}\n\nUSAGE:\n  grok-bridge            Read one JSON request from STDIN and write one JSON result to STDOUT\n  grok-bridge doctor     Verify that Grok Build is available\n  grok-bridge --version  Print version",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(());
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    stdio_request().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_session_ids() {
        for value in [
            "550e8400-e29b-41d4-a716-446655440000",
            "550E8400-E29B-41D4-A716-446655440000",
        ] {
            assert!(validate_session_id(value).is_ok());
        }
    }

    #[test]
    fn rejects_invalid_session_ids() {
        for value in [
            "",
            "feature-login",
            "550e8400e29b41d4a716446655440000",
            "550e8400-e29b-41d4-a716-44665544000z",
        ] {
            assert!(validate_session_id(value).is_err());
        }
    }

    #[test]
    fn omits_resume_for_new_session() {
        let args = test_args(None);
        let cli_args = build_grok_args(Path::new("project"), &args);
        assert!(!cli_args.iter().any(|value| value == "--resume"));
        assert!(!cli_args.iter().any(|value| value == "--session-id"));
    }

    #[test]
    fn uses_resume_for_existing_session() {
        let session_id = "550e8400-e29b-41d4-a716-446655440000";
        let args = test_args(Some(session_id));
        let cli_args = build_grok_args(Path::new("project"), &args);
        let resume_index = cli_args
            .iter()
            .position(|value| value == "--resume")
            .unwrap();
        assert_eq!(
            cli_args.get(resume_index + 1).map(String::as_str),
            Some(session_id)
        );
        assert!(!cli_args.iter().any(|value| value == "--session-id"));
    }

    #[test]
    fn extracts_top_level_session_id() {
        let stdout = br#"{"sessionId":"550e8400-e29b-41d4-a716-446655440000","result":"ok"}"#;
        assert_eq!(
            extract_session_id(stdout).as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(
            extract_session_id(
                br#"{"result":{"sessionId":"550e8400-e29b-41d4-a716-446655440000"}}"#
            ),
            None
        );
    }

    #[test]
    fn clips_large_output() {
        let input = vec![b'x'; MAX_OUTPUT_BYTES + 1];
        let (output, truncated) = decode_and_clip(&input);
        assert!(truncated);
        assert!(output.contains("output truncated"));
    }

    #[test]
    fn redacts_prompt_from_command_echo() {
        let command = vec!["grok".into(), "-p".into(), "secret task".into()];
        let redacted = redact_prompt(command);
        assert_eq!(redacted[2], "<prompt: 11 bytes>");
    }

    #[test]
    fn parses_json_request_and_rejects_missing_fields() {
        let request = br#"{"prompt":"fix it","cwd":".","session_id":null,"timeout_seconds":10,"auto_approve":false,"model":null}"#;
        let parsed = parse_request(request).unwrap();
        assert_eq!(parsed.prompt, "fix it");
        assert!(parse_request(br#"{"cwd":"."}"#).is_err());
    }

    fn test_args(session_id: Option<&str>) -> GrokBuildArgs {
        GrokBuildArgs {
            prompt: "fix it".to_owned(),
            cwd: ".".to_owned(),
            session_id: session_id.map(str::to_owned),
            timeout_seconds: None,
            auto_approve: None,
            model: None,
        }
    }
}
