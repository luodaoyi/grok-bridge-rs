use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, BufReader, Lines},
    process::{ChildStdout, Command},
    time::{Instant, MissedTickBehavior, interval, sleep},
};

const DEFAULT_TIMEOUT_SECONDS: u64 = 1_800;
const MAX_TIMEOUT_SECONDS: u64 = 7_200;
const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_EVENT_TEXT_BYTES: usize = 16 * 1024;
const DEFAULT_READ_LIMIT: usize = 200;
const MAX_READ_LIMIT: usize = 1_000;
const DEFAULT_WAIT_MILLISECONDS: u64 = 300_000;
const MAX_WAIT_MILLISECONDS: u64 = 7_200_000;
const POLL_MILLISECONDS: u64 = 100;
const HEARTBEAT_MILLISECONDS: u64 = 5_000;

#[derive(Debug, Deserialize)]
struct StartRequest {
    prompt: String,
    cwd: String,
    timeout_seconds: Option<u64>,
    auto_approve: Option<bool>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendRequest {
    prompt: String,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TurnRequest {
    prompt: String,
    timeout_seconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SessionPhase {
    Starting,
    Running,
    Idle,
    Failed,
    TimedOut,
    Stopped,
}

impl SessionPhase {
    fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }

    fn is_turn_terminal(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Failed | Self::TimedOut | Self::Stopped
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SessionState {
    handle: String,
    phase: SessionPhase,
    activity: String,
    cwd: String,
    provider_session_id: Option<String>,
    turn: u32,
    auto_approve: bool,
    model: Option<String>,
    worker_pid: Option<u32>,
    created_at_ms: u64,
    updated_at_ms: u64,
    turn_started_at_ms: Option<u64>,
    turn_finished_at_ms: Option<u64>,
    exit_code: Option<i32>,
    timed_out: bool,
    last_seq: u64,
    last_text: String,
    last_stderr: String,
    last_stop_reason: Option<String>,
    last_usage: Option<Value>,
    output_truncated: bool,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SessionEvent {
    seq: u64,
    timestamp_ms: u64,
    turn: u32,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<Value>,
}

#[derive(Debug, Serialize)]
struct StartResult {
    handle: String,
    state: SessionState,
}

#[derive(Debug, Serialize)]
struct ReadResult {
    state: SessionState,
    events: Vec<SessionEvent>,
    oldest_cursor: u64,
    next_cursor: u64,
    latest_cursor: u64,
    limited: bool,
}

#[derive(Debug, Serialize)]
struct WaitResult {
    reached: bool,
    wait_timed_out: bool,
    waited_for: String,
    state: SessionState,
}

#[derive(Debug, Serialize)]
struct StopResult {
    accepted: bool,
    state: SessionState,
}

#[derive(Debug, Serialize)]
struct RemoveResult {
    handle: String,
    removed: bool,
}

#[derive(Debug, Serialize)]
struct ListResult {
    sessions: Vec<SessionState>,
}

#[derive(Debug, Serialize)]
struct DoctorResult {
    bridge_version: String,
    grok_binary: String,
    available: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    state_dir: String,
}

#[derive(Debug, Serialize)]
struct SuccessEnvelope<'a, T> {
    ok: bool,
    result: &'a T,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope<'a> {
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

struct SessionPaths {
    directory: PathBuf,
    state: PathBuf,
    events: PathBuf,
    request: PathBuf,
    stop: PathBuf,
    lock: PathBuf,
}

impl SessionPaths {
    fn new(handle: &str) -> Result<Self> {
        validate_handle(handle)?;
        let directory = state_root()?.join(handle);
        Ok(Self {
            state: directory.join("state.json"),
            events: directory.join("events.jsonl"),
            request: directory.join("request.json"),
            stop: directory.join("stop.requested"),
            lock: directory.join("worker.lock"),
            directory,
        })
    }
}

struct WorkerLock {
    path: PathBuf,
}

impl Drop for WorkerLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForcedExit {
    Stopped,
    TimedOut,
}

#[tokio::main]
async fn main() {
    let internal_worker = env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == OsStr::new("__worker"));

    if let Err(error) = dispatch().await {
        eprintln!("grok-bridge: {error:#}");
        if !internal_worker {
            let _ = write_error("command_failed", &format!("{error:#}"));
        }
        std::process::exit(1);
    }
}

async fn dispatch() -> Result<()> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let Some(command) = arguments.first() else {
        print_help();
        return Ok(());
    };
    let command = command.to_string_lossy();

    match command.as_ref() {
        "start" | "create" => {
            ensure_no_arguments(&arguments[1..])?;
            let request = read_stdin_json::<StartRequest>()?;
            let result = start_session(request)?;
            write_success(&result)
        }
        "status" => {
            let options = parse_options(&arguments[1..], &["--session"])?;
            let handle = required_option(&options, "--session")?;
            let state = load_state(&SessionPaths::new(handle)?)?;
            write_success(&state)
        }
        "read" => {
            let options = parse_options(
                &arguments[1..],
                &["--session", "--cursor", "--limit", "--wait-ms"],
            )?;
            let handle = required_option(&options, "--session")?;
            let cursor = parse_u64_option(&options, "--cursor")?.unwrap_or(0);
            let limit = parse_usize_option(&options, "--limit")?
                .unwrap_or(DEFAULT_READ_LIMIT)
                .clamp(1, MAX_READ_LIMIT);
            let wait_ms = parse_u64_option(&options, "--wait-ms")?
                .unwrap_or(0)
                .min(MAX_WAIT_MILLISECONDS);
            let result = read_session(handle, cursor, limit, wait_ms).await?;
            write_success(&result)
        }
        "wait" => {
            let options = parse_options(&arguments[1..], &["--session", "--for", "--timeout-ms"])?;
            let handle = required_option(&options, "--session")?;
            let wait_for = options
                .get("--for")
                .map(String::as_str)
                .unwrap_or("tui-idle");
            validate_wait_target(wait_for)?;
            let timeout_ms = parse_u64_option(&options, "--timeout-ms")?
                .unwrap_or(DEFAULT_WAIT_MILLISECONDS)
                .clamp(POLL_MILLISECONDS, MAX_WAIT_MILLISECONDS);
            let result = wait_session(handle, wait_for, timeout_ms).await?;
            write_success(&result)
        }
        "send" => {
            let options = parse_options(&arguments[1..], &["--session"])?;
            let handle = required_option(&options, "--session")?;
            let request = read_stdin_json::<SendRequest>()?;
            let result = send_session(handle, request)?;
            write_success(&result)
        }
        "stop" => {
            let options = parse_options(&arguments[1..], &["--session"])?;
            let handle = required_option(&options, "--session")?;
            let result = stop_session(handle)?;
            write_success(&result)
        }
        "remove" => {
            let options = parse_options(&arguments[1..], &["--session"])?;
            let handle = required_option(&options, "--session")?;
            let result = remove_session(handle)?;
            write_success(&result)
        }
        "list" => {
            ensure_no_arguments(&arguments[1..])?;
            let result = list_sessions()?;
            write_success(&result)
        }
        "doctor" | "--doctor" => {
            ensure_no_arguments(&arguments[1..])?;
            let result = doctor().await?;
            write_success(&result)
        }
        "--version" | "-V" => {
            ensure_no_arguments(&arguments[1..])?;
            println!("grok-bridge {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        "__worker" => {
            let handle = arguments
                .get(1)
                .context("internal worker requires a session handle")?
                .to_string_lossy()
                .into_owned();
            ensure_no_arguments(&arguments[2..])?;
            worker_entry(&handle).await;
            Ok(())
        }
        other => bail!("unknown command: {other}"),
    }
}

fn print_help() {
    println!(
        "grok-bridge {}\n\nUSAGE:\n  grok-bridge start\n  grok-bridge status --session <handle>\n  grok-bridge read --session <handle> [--cursor <n>] [--limit <n>] [--wait-ms <n>]\n  grok-bridge wait --session <handle> [--for tui-idle|exit] [--timeout-ms <n>]\n  grok-bridge send --session <handle>\n  grok-bridge stop --session <handle>\n  grok-bridge remove --session <handle>\n  grok-bridge list\n  grok-bridge doctor\n\nSTART reads one UTF-8 JSON object from STDIN. SEND reads one UTF-8 JSON object containing prompt and optional timeout_seconds. REMOVE deletes a non-active session and its local events. All protocol commands write one JSON object to STDOUT.",
        env!("CARGO_PKG_VERSION")
    );
}

fn start_session(request: StartRequest) -> Result<StartResult> {
    validate_prompt(&request.prompt)?;
    if let Some(model) = request.model.as_deref() {
        validate_model(model)?;
    }
    let cwd = canonical_directory(&request.cwd)?;
    ensure_allowed_root(&cwd)?;
    let timeout_seconds = normalized_timeout(request.timeout_seconds);
    let root = state_root()?;
    fs::create_dir_all(&root)
        .with_context(|| format!("failed to create state directory: {}", root.display()))?;
    set_private_directory_permissions(&root)?;

    let (handle, paths) = create_session_directory(&root)?;
    let now = now_millis();
    let mut state = SessionState {
        handle: handle.clone(),
        phase: SessionPhase::Starting,
        activity: "queued".to_owned(),
        cwd: cwd.to_string_lossy().into_owned(),
        provider_session_id: None,
        turn: 1,
        auto_approve: request.auto_approve.unwrap_or(false),
        model: request.model,
        worker_pid: None,
        created_at_ms: now,
        updated_at_ms: now,
        turn_started_at_ms: None,
        turn_finished_at_ms: None,
        exit_code: None,
        timed_out: false,
        last_seq: 0,
        last_text: String::new(),
        last_stderr: String::new(),
        last_stop_reason: None,
        last_usage: None,
        output_truncated: false,
        error: None,
    };

    save_state(&paths, &state)?;
    write_turn_request(
        &paths,
        &TurnRequest {
            prompt: request.prompt,
            timeout_seconds,
        },
    )?;
    append_event(
        &paths,
        &mut state,
        "session_created",
        None,
        Some(json!({"phase": "starting"})),
    )?;

    if let Err(error) = launch_worker(&handle) {
        let _ = fs::remove_file(&paths.request);
        state.phase = SessionPhase::Failed;
        state.activity = "failed".to_owned();
        state.error = Some(format!("failed to launch background worker: {error:#}"));
        state.turn_finished_at_ms = Some(now_millis());
        let error_detail = state.error.clone();
        append_event(
            &paths,
            &mut state,
            "turn_failed",
            None,
            Some(json!({"error": error_detail})),
        )?;
        return Ok(StartResult { handle, state });
    }

    Ok(StartResult { handle, state })
}

fn send_session(handle: &str, request: SendRequest) -> Result<StartResult> {
    validate_prompt(&request.prompt)?;
    let paths = SessionPaths::new(handle)?;
    let mut state = load_state(&paths)?;
    if state.phase != SessionPhase::Idle {
        bail!(
            "session is not idle; current phase is {}",
            phase_name(state.phase)
        );
    }
    if state.provider_session_id.is_none() {
        bail!("session has no Grok resume UUID; start a new session instead");
    }

    let _ = fs::remove_file(&paths.stop);
    state.turn = state.turn.saturating_add(1);
    state.phase = SessionPhase::Starting;
    state.activity = "queued".to_owned();
    state.worker_pid = None;
    state.turn_started_at_ms = None;
    state.turn_finished_at_ms = None;
    state.exit_code = None;
    state.timed_out = false;
    state.last_text.clear();
    state.last_stderr.clear();
    state.last_stop_reason = None;
    state.last_usage = None;
    state.output_truncated = false;
    state.error = None;
    save_state(&paths, &state)?;
    write_turn_request(
        &paths,
        &TurnRequest {
            prompt: request.prompt,
            timeout_seconds: normalized_timeout(request.timeout_seconds),
        },
    )?;
    append_event(
        &paths,
        &mut state,
        "turn_queued",
        None,
        Some(json!({"phase": "starting"})),
    )?;

    if let Err(error) = launch_worker(handle) {
        let _ = fs::remove_file(&paths.request);
        state.phase = SessionPhase::Failed;
        state.activity = "failed".to_owned();
        state.error = Some(format!("failed to launch background worker: {error:#}"));
        state.turn_finished_at_ms = Some(now_millis());
        let error_detail = state.error.clone();
        append_event(
            &paths,
            &mut state,
            "turn_failed",
            None,
            Some(json!({"error": error_detail})),
        )?;
    }

    Ok(StartResult {
        handle: handle.to_owned(),
        state,
    })
}

async fn worker_entry(handle: &str) {
    if let Err(error) = run_worker(handle).await {
        let _ = mark_worker_failure(handle, &format!("{error:#}"));
    }
}

async fn run_worker(handle: &str) -> Result<()> {
    let paths = SessionPaths::new(handle)?;
    let _lock = acquire_worker_lock(&paths)?;
    let mut state = load_state(&paths)?;
    if state.phase != SessionPhase::Starting {
        bail!(
            "worker expected starting phase, found {}",
            phase_name(state.phase)
        );
    }

    let request = read_turn_request(&paths)?;
    let _ = fs::remove_file(&paths.request);
    validate_prompt(&request.prompt)?;
    validate_session_configuration(&state)?;
    let cwd = canonical_directory(&state.cwd)?;
    ensure_allowed_root(&cwd)?;

    let grok_binary = env::var_os("GROK_BIN").unwrap_or_else(|| OsString::from("grok"));
    let cli_args = build_grok_args(&cwd, &state, &request);
    let mut command = Command::new(&grok_binary);
    command
        .args(&cli_args)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().with_context(|| {
        "failed to start Grok Build; set GROK_BIN to the full executable path if `grok` is not on PATH"
    })?;
    let stdout = child
        .stdout
        .take()
        .context("Grok stdout was not captured")?;
    let stderr = child
        .stderr
        .take()
        .context("Grok stderr was not captured")?;
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();

    state.phase = SessionPhase::Running;
    state.activity = "starting_grok".to_owned();
    state.worker_pid = Some(std::process::id());
    state.turn_started_at_ms = Some(now_millis());
    let resumed = state.provider_session_id.is_some();
    append_event(
        &paths,
        &mut state,
        "turn_started",
        None,
        Some(json!({
            "timeout_seconds": request.timeout_seconds,
            "resumed": resumed
        })),
    )?;

    let deadline = Instant::now() + Duration::from_secs(request.timeout_seconds);
    let mut ticker = interval(Duration::from_millis(POLL_MILLISECONDS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_heartbeat = Instant::now();
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut process_status = None;
    let mut forced_exit = None;
    let mut final_text = String::new();
    let mut stderr_text = String::new();
    let mut pending_text = String::new();
    let mut pending_text_truncated = false;

    while process_status.is_none() || !stdout_done || !stderr_done {
        tokio::select! {
            line = next_stdout_line(&mut stdout_lines), if !stdout_done => {
                match line {
                    Ok(Some(line)) => {
                        handle_provider_line(
                            &paths,
                            &mut state,
                            &line,
                            &mut final_text,
                            &mut pending_text,
                            &mut pending_text_truncated,
                        )?;
                        if pending_text.len() >= MAX_EVENT_TEXT_BYTES {
                            flush_text_event(&paths, &mut state, &mut pending_text)?;
                        }
                    }
                    Ok(None) => stdout_done = true,
                    Err(error) => {
                        stdout_done = true;
                        state.error = Some(format!("failed to read Grok streaming output: {error}"));
                    }
                }
            }
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(line)) => {
                        let accepted = append_limited(&mut stderr_text, &format!("{line}\n"), MAX_OUTPUT_BYTES);
                        if !accepted.is_empty() {
                            append_event(
                                &paths,
                                &mut state,
                                "diagnostic",
                                Some(clip_string(&accepted, MAX_EVENT_TEXT_BYTES)),
                                None,
                            )?;
                        }
                        if accepted.len() < line.len().saturating_add(1) {
                            state.output_truncated = true;
                        }
                    }
                    Ok(None) => stderr_done = true,
                    Err(error) => {
                        stderr_done = true;
                        state.error = Some(format!("failed to read Grok stderr: {error}"));
                    }
                }
            }
            _ = ticker.tick() => {
                if !pending_text.is_empty() {
                    flush_text_event(&paths, &mut state, &mut pending_text)?;
                }

                if forced_exit.is_none() && paths.stop.exists() {
                    state.activity = "stopping".to_owned();
                    append_event(&paths, &mut state, "stop_requested", None, None)?;
                    let _ = child.kill().await;
                    forced_exit = Some(ForcedExit::Stopped);
                } else if forced_exit.is_none() && Instant::now() >= deadline {
                    state.activity = "timing_out".to_owned();
                    append_event(
                        &paths,
                        &mut state,
                        "timeout_requested",
                        None,
                        Some(json!({"timeout_seconds": request.timeout_seconds})),
                    )?;
                    let _ = child.kill().await;
                    forced_exit = Some(ForcedExit::TimedOut);
                }

                if process_status.is_none() {
                    process_status = child.try_wait().context("failed to query Grok process status")?;
                }
                if process_status.is_none()
                    && last_heartbeat.elapsed() >= Duration::from_millis(HEARTBEAT_MILLISECONDS)
                {
                    let activity = state.activity.clone();
                    append_event(
                        &paths,
                        &mut state,
                        "heartbeat",
                        None,
                        Some(json!({"activity": activity})),
                    )?;
                    last_heartbeat = Instant::now();
                }
            }
        }
    }

    if !pending_text.is_empty() {
        flush_text_event(&paths, &mut state, &mut pending_text)?;
    }
    if pending_text_truncated {
        state.output_truncated = true;
    }

    let status = process_status.context("Grok process exited without a status")?;
    state.exit_code = status.code();
    state.last_text = final_text;
    state.last_stderr = stderr_text;
    state.worker_pid = None;
    state.turn_finished_at_ms = Some(now_millis());

    match forced_exit {
        Some(ForcedExit::Stopped) => {
            state.phase = SessionPhase::Stopped;
            state.activity = "stopped".to_owned();
            state.error = Some("Grok Build was stopped by request".to_owned());
            append_event(&paths, &mut state, "turn_stopped", None, None)?;
        }
        Some(ForcedExit::TimedOut) => {
            state.phase = SessionPhase::TimedOut;
            state.activity = "timed_out".to_owned();
            state.timed_out = true;
            state.error = Some(format!(
                "Grok Build exceeded {} seconds",
                request.timeout_seconds
            ));
            append_event(
                &paths,
                &mut state,
                "turn_timed_out",
                None,
                Some(json!({"timeout_seconds": request.timeout_seconds})),
            )?;
        }
        None if status.success() && state.provider_session_id.is_some() => {
            state.phase = SessionPhase::Idle;
            state.activity = "idle".to_owned();
            state.error = None;
            let exit_code = state.exit_code;
            append_event(
                &paths,
                &mut state,
                "turn_completed",
                None,
                Some(json!({"exit_code": exit_code})),
            )?;
        }
        None => {
            state.phase = SessionPhase::Failed;
            state.activity = "failed".to_owned();
            if state.error.is_none() {
                state.error = Some(if status.success() {
                    "Grok Build did not return a resumable session UUID".to_owned()
                } else {
                    "Grok Build exited with a non-zero status".to_owned()
                });
            }
            let exit_code = state.exit_code;
            let error = state.error.clone();
            append_event(
                &paths,
                &mut state,
                "turn_failed",
                None,
                Some(json!({"exit_code": exit_code, "error": error})),
            )?;
        }
    }

    let _ = fs::remove_file(&paths.stop);
    Ok(())
}

async fn next_stdout_line(lines: &mut Lines<BufReader<ChildStdout>>) -> io::Result<Option<String>> {
    lines.next_line().await
}

fn handle_provider_line(
    paths: &SessionPaths,
    state: &mut SessionState,
    line: &str,
    final_text: &mut String,
    pending_text: &mut String,
    output_truncated: &mut bool,
) -> Result<()> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => {
            append_event(
                paths,
                state,
                "provider_output",
                Some(clip_string(line, MAX_EVENT_TEXT_BYTES)),
                Some(json!({"format": "plain"})),
            )?;
            return Ok(());
        }
    };
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    match event_type {
        "thought" => {
            if state.activity != "thinking" {
                state.activity = "thinking".to_owned();
                append_event(
                    paths,
                    state,
                    "activity",
                    None,
                    Some(json!({"name": "thinking"})),
                )?;
            }
        }
        "text" => {
            if state.activity != "responding" {
                state.activity = "responding".to_owned();
                append_event(
                    paths,
                    state,
                    "activity",
                    None,
                    Some(json!({"name": "responding"})),
                )?;
            }
            if let Some(text) = value.get("data").and_then(Value::as_str) {
                let accepted = append_limited(final_text, text, MAX_OUTPUT_BYTES);
                pending_text.push_str(&accepted);
                if accepted.len() < text.len() {
                    *output_truncated = true;
                }
            }
        }
        "end" => {
            flush_text_event(paths, state, pending_text)?;
            if let Some(session_id) = value.get("sessionId").and_then(Value::as_str) {
                validate_session_id(session_id)?;
                state.provider_session_id = Some(session_id.to_owned());
            }
            state.last_stop_reason = value
                .get("stopReason")
                .and_then(Value::as_str)
                .map(str::to_owned);
            state.last_usage = value.get("usage").cloned();
            append_event(
                paths,
                state,
                "provider_end",
                None,
                Some(json!({
                    "stop_reason": state.last_stop_reason,
                    "session_id": state.provider_session_id,
                    "usage": state.last_usage
                })),
            )?;
        }
        other => {
            append_event(
                paths,
                state,
                "provider_event",
                None,
                Some(json!({"type": other})),
            )?;
        }
    }
    Ok(())
}

fn flush_text_event(
    paths: &SessionPaths,
    state: &mut SessionState,
    pending_text: &mut String,
) -> Result<()> {
    if pending_text.is_empty() {
        return Ok(());
    }
    let text = std::mem::take(pending_text);
    append_event(paths, state, "text", Some(text), None)
}

fn mark_worker_failure(handle: &str, error: &str) -> Result<()> {
    let paths = SessionPaths::new(handle)?;
    let _ = fs::remove_file(&paths.request);
    let mut state = load_state(&paths)?;
    state.phase = SessionPhase::Failed;
    state.activity = "failed".to_owned();
    state.worker_pid = None;
    state.turn_finished_at_ms = Some(now_millis());
    state.error = Some(error.to_owned());
    append_event(
        &paths,
        &mut state,
        "turn_failed",
        None,
        Some(json!({"error": error})),
    )
}

fn acquire_worker_lock(paths: &SessionPaths) -> Result<WorkerLock> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    open_private_file(&mut options, &paths.lock).with_context(|| {
        format!(
            "session already has an active worker: {}",
            paths.directory.display()
        )
    })?;
    Ok(WorkerLock {
        path: paths.lock.clone(),
    })
}

fn launch_worker(handle: &str) -> Result<()> {
    let executable = env::current_exe().context("failed to resolve grok-bridge executable")?;
    let mut command = background_command(&executable, handle, true);
    match command.spawn() {
        Ok(_) => Ok(()),
        #[cfg(windows)]
        Err(first_error) => {
            let mut fallback = background_command(&executable, handle, false);
            fallback.spawn().with_context(|| {
                format!(
                    "detached worker launch failed ({first_error}); fallback launch also failed"
                )
            })?;
            Ok(())
        }
        #[cfg(not(windows))]
        Err(error) => Err(error).context("failed to launch background worker"),
    }
}

fn background_command(executable: &Path, handle: &str, break_away: bool) -> StdCommand {
    let mut command = StdCommand::new(executable);
    command
        .arg("__worker")
        .arg(handle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_background_process(&mut command, break_away);
    command
}

#[cfg(windows)]
fn configure_background_process(command: &mut StdCommand, break_away: bool) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    let mut flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    if break_away {
        flags |= CREATE_BREAKAWAY_FROM_JOB;
    }
    command.creation_flags(flags);
}

#[cfg(unix)]
fn configure_background_process(command: &mut StdCommand, _break_away: bool) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn configure_background_process(_command: &mut StdCommand, _break_away: bool) {}

async fn read_session(handle: &str, cursor: u64, limit: usize, wait_ms: u64) -> Result<ReadResult> {
    let paths = SessionPaths::new(handle)?;
    let deadline = Instant::now() + Duration::from_millis(wait_ms);

    loop {
        let state = load_state(&paths)?;
        if cursor > state.last_seq {
            bail!(
                "cursor {cursor} is newer than latest cursor {}",
                state.last_seq
            );
        }
        let result = read_events(&paths, state, cursor, limit)?;
        if !result.events.is_empty()
            || wait_ms == 0
            || result.state.phase.is_turn_terminal()
            || Instant::now() >= deadline
        {
            return Ok(result);
        }
        sleep(Duration::from_millis(POLL_MILLISECONDS)).await;
    }
}

fn read_events(
    paths: &SessionPaths,
    state: SessionState,
    cursor: u64,
    limit: usize,
) -> Result<ReadResult> {
    let mut events = Vec::new();
    let mut oldest_cursor = 0;
    if paths.events.exists() {
        let content = fs::read_to_string(&paths.events)
            .with_context(|| format!("failed to read events: {}", paths.events.display()))?;
        for line in content.lines() {
            let event: SessionEvent = match serde_json::from_str(line) {
                Ok(event) => event,
                Err(_) => continue,
            };
            if oldest_cursor == 0 {
                oldest_cursor = event.seq;
            }
            if event.seq > cursor && events.len() < limit {
                events.push(event);
            }
        }
    }

    let next_cursor = events.last().map(|event| event.seq).unwrap_or(cursor);
    Ok(ReadResult {
        limited: next_cursor < state.last_seq,
        latest_cursor: state.last_seq,
        state,
        events,
        oldest_cursor,
        next_cursor,
    })
}

async fn wait_session(handle: &str, wait_for: &str, timeout_ms: u64) -> Result<WaitResult> {
    let paths = SessionPaths::new(handle)?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let state = load_state(&paths)?;
        if wait_target_reached(wait_for, state.phase) {
            return Ok(WaitResult {
                reached: true,
                wait_timed_out: false,
                waited_for: wait_for.to_owned(),
                state,
            });
        }
        if state.phase.is_turn_terminal() {
            return Ok(WaitResult {
                reached: false,
                wait_timed_out: false,
                waited_for: wait_for.to_owned(),
                state,
            });
        }
        if Instant::now() >= deadline {
            return Ok(WaitResult {
                reached: false,
                wait_timed_out: true,
                waited_for: wait_for.to_owned(),
                state,
            });
        }
        sleep(Duration::from_millis(POLL_MILLISECONDS)).await;
    }
}

fn stop_session(handle: &str) -> Result<StopResult> {
    let paths = SessionPaths::new(handle)?;
    let mut state = load_state(&paths)?;
    if state.phase.is_active() {
        let _file = create_private_file(&paths.stop)
            .with_context(|| format!("failed to request stop: {}", paths.stop.display()))?;
        return Ok(StopResult {
            accepted: true,
            state,
        });
    }
    if state.phase == SessionPhase::Idle {
        state.phase = SessionPhase::Stopped;
        state.activity = "stopped".to_owned();
        state.updated_at_ms = now_millis();
        append_event(&paths, &mut state, "session_stopped", None, None)?;
        return Ok(StopResult {
            accepted: true,
            state,
        });
    }
    Ok(StopResult {
        accepted: false,
        state,
    })
}

fn remove_session(handle: &str) -> Result<RemoveResult> {
    let paths = SessionPaths::new(handle)?;
    let state = load_state(&paths)?;
    if state.phase.is_active() {
        bail!("cannot remove an active session; stop it and wait for exit first");
    }

    let root = fs::canonicalize(state_root()?).context("failed to resolve state directory")?;
    let directory = fs::canonicalize(&paths.directory).with_context(|| {
        format!(
            "failed to resolve session directory: {}",
            paths.directory.display()
        )
    })?;
    ensure_direct_child(&root, &directory)?;
    fs::remove_dir_all(&directory).with_context(|| {
        format!(
            "failed to remove session directory: {}",
            directory.display()
        )
    })?;

    Ok(RemoveResult {
        handle: handle.to_owned(),
        removed: true,
    })
}

fn list_sessions() -> Result<ListResult> {
    let root = state_root()?;
    if !root.exists() {
        return Ok(ListResult {
            sessions: Vec::new(),
        });
    }
    let mut sessions = fs::read_dir(&root)
        .with_context(|| format!("failed to list state directory: {}", root.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let handle = entry.file_name().to_string_lossy().into_owned();
            let paths = SessionPaths::new(&handle).ok()?;
            load_state(&paths).ok()
        })
        .collect::<Vec<_>>();
    sessions.sort_by_key(|state| std::cmp::Reverse(state.updated_at_ms));
    Ok(ListResult { sessions })
}

async fn doctor() -> Result<DoctorResult> {
    let grok_binary = env::var_os("GROK_BIN").unwrap_or_else(|| OsString::from("grok"));
    let output = Command::new(&grok_binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .await
        .with_context(|| "could not run Grok Build; install it or set GROK_BIN")?;
    let (stdout, _) = decode_and_clip(&output.stdout);
    let (stderr, _) = decode_and_clip(&output.stderr);
    Ok(DoctorResult {
        bridge_version: env!("CARGO_PKG_VERSION").to_owned(),
        grok_binary: grok_binary.to_string_lossy().into_owned(),
        available: output.status.success(),
        exit_code: output.status.code(),
        stdout,
        stderr,
        state_dir: state_root()?.to_string_lossy().into_owned(),
    })
}

fn build_grok_args(cwd: &Path, state: &SessionState, request: &TurnRequest) -> Vec<String> {
    let mut arguments = vec![
        "--no-auto-update".to_owned(),
        "--no-alt-screen".to_owned(),
        "--cwd".to_owned(),
        cwd.to_string_lossy().into_owned(),
        "--output-format".to_owned(),
        "streaming-json".to_owned(),
    ];
    if state.auto_approve {
        arguments.push("--always-approve".to_owned());
    }
    if let Some(model) = state.model.as_deref() {
        arguments.push("--model".to_owned());
        arguments.push(model.to_owned());
    }
    if let Some(session_id) = state.provider_session_id.as_deref() {
        arguments.push("--resume".to_owned());
        arguments.push(session_id.to_owned());
    }
    arguments.push("-p".to_owned());
    arguments.push(request.prompt.clone());
    arguments
}

fn validate_session_configuration(state: &SessionState) -> Result<()> {
    validate_handle(&state.handle)?;
    if let Some(session_id) = state.provider_session_id.as_deref() {
        validate_session_id(session_id)?;
    }
    if let Some(model) = state.model.as_deref() {
        validate_model(model)?;
    }
    Ok(())
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
    let path = fs::canonicalize(raw)
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
        .filter_map(|path| fs::canonicalize(path).ok())
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
        bail!("session ID must be a canonical UUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)");
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

fn validate_handle(value: &str) -> Result<()> {
    let valid = value.starts_with("gb-")
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if !valid {
        bail!("invalid session handle");
    }
    Ok(())
}

fn normalized_timeout(value: Option<u64>) -> u64 {
    value
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .clamp(10, MAX_TIMEOUT_SECONDS)
}

fn validate_wait_target(value: &str) -> Result<()> {
    if matches!(value, "idle" | "tui-idle" | "exit") {
        return Ok(());
    }
    bail!("--for must be one of: tui-idle, idle, exit")
}

fn wait_target_reached(target: &str, phase: SessionPhase) -> bool {
    match target {
        "idle" | "tui-idle" => phase == SessionPhase::Idle,
        "exit" => phase.is_turn_terminal(),
        _ => false,
    }
}

fn phase_name(phase: SessionPhase) -> &'static str {
    match phase {
        SessionPhase::Starting => "starting",
        SessionPhase::Running => "running",
        SessionPhase::Idle => "idle",
        SessionPhase::Failed => "failed",
        SessionPhase::TimedOut => "timed_out",
        SessionPhase::Stopped => "stopped",
    }
}

fn append_limited(target: &mut String, value: &str, limit: usize) -> String {
    let remaining = limit.saturating_sub(target.len());
    if remaining == 0 {
        return String::new();
    }
    let mut end = remaining.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let accepted = &value[..end];
    target.push_str(accepted);
    accepted.to_owned()
}

fn clip_string(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n...[event truncated by grok-bridge]", &value[..end])
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

fn create_private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    open_private_file(&mut options, path)
}

fn open_private_file(options: &mut OpenOptions, path: &Path) -> Result<File> {
    configure_private_file_creation(options);
    let file = options.open(path)?;
    set_private_file_permissions(path)?;
    Ok(file)
}

#[cfg(unix)]
fn configure_private_file_creation(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_private_file_creation(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to protect state file: {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to protect state directory: {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn ensure_direct_child(root: &Path, directory: &Path) -> Result<()> {
    if directory.parent() == Some(root) {
        return Ok(());
    }
    bail!("refusing to remove a path outside the state directory")
}

fn state_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("GROK_BRIDGE_STATE_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        let base = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA is not set; set GROK_BRIDGE_STATE_DIR")?;
        return Ok(base.join("grok-bridge").join("sessions"));
    }
    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set; set GROK_BRIDGE_STATE_DIR")?;
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("grok-bridge")
            .join("sessions"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_STATE_HOME") {
            return Ok(PathBuf::from(path).join("grok-bridge").join("sessions"));
        }
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set; set GROK_BRIDGE_STATE_DIR")?;
        return Ok(home
            .join(".local")
            .join("state")
            .join("grok-bridge")
            .join("sessions"));
    }
    #[allow(unreachable_code)]
    Err(anyhow::anyhow!(
        "unsupported platform; set GROK_BRIDGE_STATE_DIR"
    ))
}

fn create_session_directory(root: &Path) -> Result<(String, SessionPaths)> {
    for attempt in 0..100_u32 {
        let handle = format!(
            "gb-{:x}-{:x}-{:x}",
            now_nanos(),
            std::process::id(),
            attempt
        );
        let paths = SessionPaths {
            state: root.join(&handle).join("state.json"),
            events: root.join(&handle).join("events.jsonl"),
            request: root.join(&handle).join("request.json"),
            stop: root.join(&handle).join("stop.requested"),
            lock: root.join(&handle).join("worker.lock"),
            directory: root.join(&handle),
        };
        match fs::create_dir(&paths.directory) {
            Ok(()) => {
                set_private_directory_permissions(&paths.directory)?;
                return Ok((handle, paths));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create session directory: {}",
                        paths.directory.display()
                    )
                });
            }
        }
    }
    bail!("failed to allocate a unique session handle")
}

fn save_state(paths: &SessionPaths, state: &SessionState) -> Result<()> {
    let data = serde_json::to_vec(state).context("failed to serialize session state")?;
    let mut file = create_private_file(&paths.state)
        .with_context(|| format!("failed to write state: {}", paths.state.display()))?;
    file.write_all(&data)
        .with_context(|| format!("failed to write state: {}", paths.state.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush state: {}", paths.state.display()))
}

fn load_state(paths: &SessionPaths) -> Result<SessionState> {
    let mut last_error = None;
    for attempt in 0..5 {
        match fs::read(&paths.state) {
            Ok(data) => match serde_json::from_slice(&data) {
                Ok(state) => return Ok(state),
                Err(error) => last_error = Some(anyhow::Error::from(error)),
            },
            Err(error) => last_error = Some(anyhow::Error::from(error)),
        }
        if attempt < 4 {
            thread::sleep(Duration::from_millis(20));
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("unknown state read error"))).with_context(
        || {
            format!(
                "session not found or state is unreadable: {}",
                paths.directory.display()
            )
        },
    )
}

fn append_event(
    paths: &SessionPaths,
    state: &mut SessionState,
    kind: &str,
    text: Option<String>,
    detail: Option<Value>,
) -> Result<()> {
    state.last_seq = state.last_seq.saturating_add(1);
    state.updated_at_ms = now_millis();
    let event = SessionEvent {
        seq: state.last_seq,
        timestamp_ms: state.updated_at_ms,
        turn: state.turn,
        kind: kind.to_owned(),
        text,
        detail,
    };
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    let mut file = open_private_file(&mut options, &paths.events)
        .with_context(|| format!("failed to append events: {}", paths.events.display()))?;
    serde_json::to_writer(&mut file, &event).context("failed to serialize session event")?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to append events: {}", paths.events.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush events: {}", paths.events.display()))?;
    save_state(paths, state)
}

fn write_turn_request(paths: &SessionPaths, request: &TurnRequest) -> Result<()> {
    if paths.request.exists() {
        bail!("session already has a pending request");
    }
    let data = serde_json::to_vec(request).context("failed to serialize turn request")?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    let mut file = open_private_file(&mut options, &paths.request)
        .with_context(|| format!("failed to write request: {}", paths.request.display()))?;
    file.write_all(&data)
        .with_context(|| format!("failed to write request: {}", paths.request.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush request: {}", paths.request.display()))
}

fn read_turn_request(paths: &SessionPaths) -> Result<TurnRequest> {
    let data = fs::read(&paths.request)
        .with_context(|| format!("failed to read request: {}", paths.request.display()))?;
    serde_json::from_slice(&data).context("pending turn request is invalid JSON")
}

fn read_stdin_json<T>() -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .context("failed to read request from STDIN")?;
    let input = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&input);
    serde_json::from_slice(input).context("STDIN must contain one UTF-8 JSON object")
}

fn write_success<T: Serialize>(result: &T) -> Result<()> {
    write_json(&SuccessEnvelope { ok: true, result })
}

fn write_error(code: &str, message: &str) -> Result<()> {
    write_json(&ErrorEnvelope {
        ok: false,
        error: ErrorBody { code, message },
    })
}

fn write_json<T: Serialize>(value: &T) -> Result<()> {
    let mut output = serde_json::to_vec(value).context("failed to serialize JSON response")?;
    output.push(b'\n');
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&output)
        .context("failed to write JSON response to STDOUT")?;
    stdout.flush().context("failed to flush STDOUT")
}

fn parse_options(arguments: &[OsString], allowed: &[&str]) -> Result<BTreeMap<String, String>> {
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < arguments.len() {
        let name = arguments[index].to_string_lossy().into_owned();
        if !allowed.iter().any(|allowed_name| *allowed_name == name) {
            bail!("unknown option: {name}");
        }
        let value = arguments
            .get(index + 1)
            .with_context(|| format!("missing value for {name}"))?
            .to_string_lossy()
            .into_owned();
        if value.starts_with("--") {
            bail!("missing value for {name}");
        }
        if options.insert(name.clone(), value).is_some() {
            bail!("duplicate option: {name}");
        }
        index += 2;
    }
    Ok(options)
}

fn required_option<'a>(options: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    options
        .get(name)
        .map(String::as_str)
        .with_context(|| format!("missing required option: {name}"))
}

fn parse_u64_option(options: &BTreeMap<String, String>, name: &str) -> Result<Option<u64>> {
    options
        .get(name)
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("{name} must be an unsigned integer"))
        })
        .transpose()
}

fn parse_usize_option(options: &BTreeMap<String, String>, name: &str) -> Result<Option<usize>> {
    options
        .get(name)
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("{name} must be an unsigned integer"))
        })
        .transpose()
}

fn ensure_no_arguments(arguments: &[OsString]) -> Result<()> {
    if arguments.is_empty() {
        return Ok(());
    }
    bail!("unexpected argument: {}", arguments[0].to_string_lossy())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_session_ids() {
        for value in [
            "550e8400-e29b-41d4-a716-446655440000",
            "019f6074-b7e0-71b3-b8a7-55b3ba4a0a52",
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
    fn builds_streaming_new_and_resume_commands() {
        let request = TurnRequest {
            prompt: "修复中文".to_owned(),
            timeout_seconds: 30,
        };
        let mut state = test_state();
        let initial = build_grok_args(Path::new("project"), &state, &request);
        assert!(
            initial
                .windows(2)
                .any(|pair| pair == ["--output-format", "streaming-json"])
        );
        assert!(!initial.iter().any(|value| value == "--resume"));

        state.provider_session_id = Some("550e8400-e29b-41d4-a716-446655440000".to_owned());
        let resumed = build_grok_args(Path::new("project"), &state, &request);
        let index = resumed
            .iter()
            .position(|value| value == "--resume")
            .unwrap();
        assert_eq!(resumed[index + 1], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(resumed.last().map(String::as_str), Some("修复中文"));
    }

    #[test]
    fn parses_utf8_bom_and_chinese_request() {
        let mut input = vec![0xEF, 0xBB, 0xBF];
        input.extend_from_slice(
            br#"{"prompt":"\u4fee\u590d\u4e2d\u6587","cwd":".","timeout_seconds":10,"auto_approve":false,"model":null}"#,
        );
        let input = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap();
        let request: StartRequest = serde_json::from_slice(input).unwrap();
        assert_eq!(request.prompt, "修复中文");
    }

    #[test]
    fn clips_at_utf8_boundaries() {
        let mut target = String::new();
        let accepted = append_limited(&mut target, "中文爸爸", 7);
        assert_eq!(accepted, "中文");
        assert_eq!(target, "中文");
    }

    #[test]
    fn wait_targets_match_expected_phases() {
        assert!(wait_target_reached("tui-idle", SessionPhase::Idle));
        assert!(!wait_target_reached("tui-idle", SessionPhase::Failed));
        assert!(wait_target_reached("exit", SessionPhase::Failed));
        assert!(!wait_target_reached("exit", SessionPhase::Running));
    }

    #[test]
    fn validates_session_handles() {
        assert!(validate_handle("gb-1234-abcd-0").is_ok());
        assert!(validate_handle("../outside").is_err());
        assert!(validate_handle("gb_has_underscore").is_err());
    }

    #[test]
    fn provider_stream_hides_thought_text_and_captures_result() {
        let directory = test_directory("provider-stream");
        fs::create_dir_all(&directory).unwrap();
        let paths = test_paths(directory.clone());
        let mut state = test_state();
        save_state(&paths, &state).unwrap();
        let mut final_text = String::new();
        let mut pending = String::new();
        let mut truncated = false;

        handle_provider_line(
            &paths,
            &mut state,
            r#"{"type":"thought","data":"private reasoning"}"#,
            &mut final_text,
            &mut pending,
            &mut truncated,
        )
        .unwrap();
        handle_provider_line(
            &paths,
            &mut state,
            r#"{"type":"text","data":"中文结果"}"#,
            &mut final_text,
            &mut pending,
            &mut truncated,
        )
        .unwrap();
        handle_provider_line(
            &paths,
            &mut state,
            r#"{"type":"end","stopReason":"EndTurn","sessionId":"550e8400-e29b-41d4-a716-446655440000","usage":{"total_tokens":12}}"#,
            &mut final_text,
            &mut pending,
            &mut truncated,
        )
        .unwrap();
        flush_text_event(&paths, &mut state, &mut pending).unwrap();

        let event_text = fs::read_to_string(&paths.events).unwrap();
        assert!(!event_text.contains("private reasoning"));
        assert!(event_text.contains("中文结果"));
        assert_eq!(final_text, "中文结果");
        assert_eq!(
            state.provider_session_id.as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cursor_reads_are_incremental_and_limited() {
        let directory = test_directory("cursor-read");
        fs::create_dir_all(&directory).unwrap();
        let paths = test_paths(directory.clone());
        let mut state = test_state();
        save_state(&paths, &state).unwrap();
        append_event(&paths, &mut state, "one", None, None).unwrap();
        append_event(&paths, &mut state, "two", None, None).unwrap();
        append_event(&paths, &mut state, "three", None, None).unwrap();

        let first = read_events(&paths, state.clone(), 0, 2).unwrap();
        assert_eq!(first.events.len(), 2);
        assert_eq!(first.next_cursor, 2);
        assert!(first.limited);
        let second = read_events(&paths, state, first.next_cursor, 2).unwrap();
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].kind, "three");
        assert!(!second.limited);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removal_target_must_be_a_direct_state_child() {
        let root = test_directory("remove-root");
        let session = root.join("gb-1234-abcd-0");
        let nested = session.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let session = fs::canonicalize(session).unwrap();
        let nested = fs::canonicalize(nested).unwrap();

        assert!(ensure_direct_child(&root, &session).is_ok());
        assert!(ensure_direct_child(&root, &nested).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_state_files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("private-permissions");
        fs::create_dir_all(&directory).unwrap();
        set_private_directory_permissions(&directory).unwrap();
        let file = directory.join("state.json");
        create_private_file(&file).unwrap();

        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(directory).unwrap();
    }

    fn test_state() -> SessionState {
        SessionState {
            handle: "gb-1234-abcd-0".to_owned(),
            phase: SessionPhase::Starting,
            activity: "queued".to_owned(),
            cwd: ".".to_owned(),
            provider_session_id: None,
            turn: 1,
            auto_approve: false,
            model: None,
            worker_pid: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            turn_started_at_ms: None,
            turn_finished_at_ms: None,
            exit_code: None,
            timed_out: false,
            last_seq: 0,
            last_text: String::new(),
            last_stderr: String::new(),
            last_stop_reason: None,
            last_usage: None,
            output_truncated: false,
            error: None,
        }
    }

    fn test_directory(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "grok-bridge-{name}-{}-{}",
            std::process::id(),
            now_nanos()
        ))
    }

    fn test_paths(directory: PathBuf) -> SessionPaths {
        SessionPaths {
            state: directory.join("state.json"),
            events: directory.join("events.jsonl"),
            request: directory.join("request.json"),
            stop: directory.join("stop.requested"),
            lock: directory.join("worker.lock"),
            directory,
        }
    }
}
