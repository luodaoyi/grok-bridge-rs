use std::fmt::Display;

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(crate) const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_WRITE_BYTES: usize = 64 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_OWNER_BYTES: usize = 128;
const PROVIDER_SESSION_ID_BYTES: usize = 36;
const MAX_HOOK_CWD_BYTES: usize = 4096;
const MAX_HOOK_SHORT_TEXT_BYTES: usize = 128;
const MAX_HOOK_MESSAGE_BYTES: usize = 1024;
const MAX_READ_LIMIT: u32 = 65_536;
const MIN_TERMINAL_COLS: u16 = 20;
const MAX_TERMINAL_COLS: u16 = 500;
const MIN_TERMINAL_ROWS: u16 = 5;
const MAX_TERMINAL_ROWS: u16 = 200;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestEnvelope {
    pub(crate) id: String,
    pub(crate) request: Request,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Request {
    ServerStatus,
    ServerStop,
    Create {
        cwd: String,
        prompt: Option<String>,
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
        #[serde(default)]
        always_approve: bool,
    },
    List,
    Show {
        session: String,
    },
    Read {
        session: String,
        cursor: Option<u64>,
        limit: Option<u32>,
        wait_ms: Option<u64>,
    },
    Send {
        session: String,
        input: String,
    },
    Write {
        session: String,
        data_base64: String,
    },
    Resize {
        session: String,
        cols: u16,
        rows: u16,
    },
    Wait {
        session: String,
        for_condition: WaitCondition,
        timeout_ms: Option<u64>,
    },
    Close {
        session: String,
    },
    HookEvent {
        provider_session_id: String,
        event: HookEvent,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HookEventKind {
    SessionStart,
    UserPromptSubmit,
    Stop,
    StopFailure,
    SessionEnd,
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    Notification,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HookActivity {
    #[default]
    Unknown,
    Working,
    Waiting,
    Done,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HookEvent {
    pub(crate) kind: HookEventKind,
    pub(crate) cwd: Option<String>,
    pub(crate) tool_name: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) notification_type: Option<String>,
    pub(crate) level: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WaitCondition {
    TuiIdle,
    Exit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseEnvelope {
    pub(crate) id: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<ResponseResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<ResponseError>,
}

impl ResponseEnvelope {
    pub(crate) fn success(id: impl Into<String>, result: ResponseResult) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub(crate) fn failure(
        id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(ResponseError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum ResponseResult {
    ServerInfo(ServerInfo),
    Session(SessionState),
    Sessions { sessions: Vec<SessionState> },
    Read(ReadResult),
    Wait(WaitResult),
    Accepted { accepted: bool },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseError {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServerInfo {
    pub(crate) version: String,
    pub(crate) process_id: u32,
    pub(crate) started_at_ms: u64,
    pub(crate) active_sessions: u32,
    pub(crate) web_url: Option<String>,
    pub(crate) stopping: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionPhase {
    Starting,
    Running,
    Idle,
    Exited,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionState {
    pub(crate) session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) owner: Option<String>,
    pub(crate) phase: SessionPhase,
    pub(crate) title: Option<String>,
    pub(crate) cwd: String,
    pub(crate) model: Option<String>,
    pub(crate) always_approve: bool,
    pub(crate) process_id: Option<u32>,
    pub(crate) screen: Option<String>,
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) screen_ansi_base64: String,
    pub(crate) last_cursor: u64,
    pub(crate) last_output_at_ms: Option<u64>,
    pub(crate) created_at_ms: u64,
    pub(crate) updated_at_ms: u64,
    pub(crate) exit_code: Option<u32>,
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) activity: HookActivity,
    pub(crate) hook_event: Option<HookEventKind>,
    pub(crate) hook_at_ms: Option<u64>,
    pub(crate) tool_name: Option<String>,
    pub(crate) waiting_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadResult {
    pub(crate) session: String,
    /// Inclusive byte offset of the first byte returned in `data_base64`.
    pub(crate) cursor: u64,
    /// Exclusive byte offset after the returned bytes.
    pub(crate) next_cursor: u64,
    pub(crate) data_base64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) plain_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) screen: Option<String>,
    pub(crate) truncated: bool,
    pub(crate) eof: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WaitResult {
    pub(crate) session: String,
    pub(crate) condition: WaitCondition,
    pub(crate) satisfied: bool,
    pub(crate) timed_out: bool,
    pub(crate) phase: SessionPhase,
    pub(crate) exit_code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) blocked_reason: Option<String>,
}

pub(crate) fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut frame = serde_json::to_vec(value).context("failed to encode protocol frame")?;
    if frame.len() + 1 > MAX_FRAME_BYTES {
        bail!("protocol frame exceeds the 1 MiB limit");
    }
    frame.push(b'\n');
    Ok(frame)
}

pub(crate) fn decode_request(frame: &[u8]) -> Result<RequestEnvelope> {
    let envelope: RequestEnvelope = decode_frame(frame)?;
    validate_identifier(&envelope.id, "request id")?;
    validate_request(&envelope.request)?;
    Ok(envelope)
}

pub(crate) fn decode_response(frame: &[u8]) -> Result<ResponseEnvelope> {
    let envelope: ResponseEnvelope = decode_frame(frame)?;
    validate_identifier(&envelope.id, "response id")?;
    validate_response(&envelope)?;
    Ok(envelope)
}

pub(crate) fn decode_write_data(data_base64: &str) -> Result<Vec<u8>> {
    let data = BASE64
        .decode(data_base64)
        .context("terminal data_base64 is invalid")?;
    if data.is_empty() {
        bail!("terminal data must not be empty");
    }
    if data.len() > MAX_WRITE_BYTES {
        bail!("terminal data exceeds the 64 KiB limit");
    }
    Ok(data)
}

pub(crate) fn validate_terminal_size(cols: u16, rows: u16) -> Result<()> {
    if !(MIN_TERMINAL_COLS..=MAX_TERMINAL_COLS).contains(&cols) {
        bail!("terminal cols must be between {MIN_TERMINAL_COLS} and {MAX_TERMINAL_COLS}");
    }
    if !(MIN_TERMINAL_ROWS..=MAX_TERMINAL_ROWS).contains(&rows) {
        bail!("terminal rows must be between {MIN_TERMINAL_ROWS} and {MAX_TERMINAL_ROWS}");
    }
    Ok(())
}

fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T> {
    let payload = single_frame_payload(frame)?;
    serde_json::from_slice(payload).context("invalid JSON protocol frame")
}

fn single_frame_payload(frame: &[u8]) -> Result<&[u8]> {
    if frame.is_empty() {
        bail!("protocol frame is empty");
    }
    if frame.len() > MAX_FRAME_BYTES {
        bail!("protocol frame exceeds the 1 MiB limit");
    }

    let payload = if let Some(without_lf) = frame.strip_suffix(b"\n") {
        without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
    } else {
        frame
    };
    if payload.is_empty() {
        bail!("protocol frame is empty");
    }
    if payload.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
        bail!("protocol input must contain exactly one NDJSON frame");
    }
    Ok(payload)
}

fn validate_request(request: &Request) -> Result<()> {
    match request {
        Request::ServerStatus | Request::ServerStop | Request::List => Ok(()),
        Request::Create {
            cwd, model, owner, ..
        } => {
            if cwd.trim().is_empty() {
                bail!("cwd must not be empty");
            }
            if model
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            {
                bail!("model must not be empty when provided");
            }
            if let Some(owner) = owner.as_deref() {
                validate_owner(owner)?;
            }
            Ok(())
        }
        Request::Show { session }
        | Request::Read { session, .. }
        | Request::Send { session, .. }
        | Request::Write { session, .. }
        | Request::Resize { session, .. }
        | Request::Wait { session, .. }
        | Request::Close { session } => {
            validate_identifier(session, "session handle")?;
            if let Request::Read {
                limit: Some(limit), ..
            } = request
                && (*limit == 0 || *limit > MAX_READ_LIMIT)
            {
                bail!("read limit must be between 1 and {MAX_READ_LIMIT}");
            }
            if let Request::Send { input, .. } = request
                && input.is_empty()
            {
                bail!("terminal input must not be empty");
            }
            if let Request::Write { data_base64, .. } = request {
                decode_write_data(data_base64)?;
            }
            if let Request::Resize { cols, rows, .. } = request {
                validate_terminal_size(*cols, *rows)?;
            }
            Ok(())
        }
        Request::HookEvent {
            provider_session_id,
            event,
        } => {
            validate_provider_session_id(provider_session_id)?;
            validate_hook_event(event)
        }
    }
}

fn validate_response(envelope: &ResponseEnvelope) -> Result<()> {
    match (envelope.ok, &envelope.result, &envelope.error) {
        (true, Some(result), None) => validate_response_result(result),
        (false, None, Some(error)) => {
            validate_identifier(&error.code, "error code")?;
            if error.message.trim().is_empty() {
                bail!("response error message must not be empty");
            }
            Ok(())
        }
        (true, _, _) => bail!("successful response must contain result and no error"),
        (false, _, _) => bail!("failed response must contain error and no result"),
    }
}

fn validate_response_result(result: &ResponseResult) -> Result<()> {
    match result {
        ResponseResult::ServerInfo(_) | ResponseResult::Accepted { .. } => Ok(()),
        ResponseResult::Session(session) => validate_session_state(session),
        ResponseResult::Sessions { sessions } => {
            for session in sessions {
                validate_session_state(session)?;
            }
            Ok(())
        }
        ResponseResult::Read(read) => {
            validate_identifier(&read.session, "session handle")?;
            let bytes = BASE64
                .decode(&read.data_base64)
                .context("read data_base64 is invalid")?;
            let expected_next_cursor = read
                .cursor
                .checked_add(bytes.len() as u64)
                .context("read cursor overflowed")?;
            if read.next_cursor != expected_next_cursor {
                bail!("read next_cursor must equal cursor plus the returned byte count");
            }
            Ok(())
        }
        ResponseResult::Wait(wait) => {
            validate_identifier(&wait.session, "session handle")?;
            if wait.satisfied && wait.timed_out {
                bail!("wait result cannot be satisfied and timed out");
            }
            if let Some(reason) = wait.blocked_reason.as_deref() {
                validate_identifier(reason, "wait blocked reason")?;
                if wait.satisfied || wait.timed_out {
                    bail!("a blocked wait cannot be satisfied or timed out");
                }
                if wait.condition != WaitCondition::TuiIdle {
                    bail!("only tui_idle waits can report a blocked reason");
                }
            }
            Ok(())
        }
    }
}

fn validate_session_state(session: &SessionState) -> Result<()> {
    validate_identifier(&session.session, "session handle")?;
    if let Some(owner) = session.owner.as_deref() {
        validate_owner(owner)?;
    }
    if session.cwd.trim().is_empty() {
        bail!("session cwd must not be empty");
    }
    if session.updated_at_ms < session.created_at_ms {
        bail!("session updated_at_ms predates created_at_ms");
    }
    validate_terminal_size(session.cols, session.rows)?;
    BASE64
        .decode(&session.screen_ansi_base64)
        .context("session screen_ansi_base64 is invalid")?;
    validate_optional_hook_text(
        session.tool_name.as_deref(),
        "session tool_name",
        MAX_HOOK_SHORT_TEXT_BYTES,
    )?;
    validate_optional_hook_text(
        session.waiting_reason.as_deref(),
        "session waiting_reason",
        MAX_HOOK_MESSAGE_BYTES,
    )?;
    Ok(())
}

pub(crate) fn validate_owner(owner: &str) -> Result<()> {
    if owner.trim().is_empty()
        || owner.len() > MAX_OWNER_BYTES
        || owner.chars().any(char::is_control)
    {
        bail!("owner must contain between 1 and {MAX_OWNER_BYTES} bytes and no control characters");
    }
    Ok(())
}

fn validate_provider_session_id(provider_session_id: &str) -> Result<()> {
    let valid = provider_session_id.len() == PROVIDER_SESSION_ID_BYTES
        && provider_session_id
            .bytes()
            .enumerate()
            .all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            });
    if !valid {
        bail!("provider_session_id must be a canonical UUID");
    }
    Ok(())
}

fn validate_hook_event(event: &HookEvent) -> Result<()> {
    if let Some(cwd) = event.cwd.as_deref()
        && (cwd.len() > MAX_HOOK_CWD_BYTES || cwd.contains('\0'))
    {
        bail!("hook cwd must contain at most {MAX_HOOK_CWD_BYTES} bytes and no NUL characters");
    }
    validate_optional_hook_text(
        event.tool_name.as_deref(),
        "hook tool_name",
        MAX_HOOK_SHORT_TEXT_BYTES,
    )?;
    validate_optional_hook_text(
        event.message.as_deref(),
        "hook message",
        MAX_HOOK_MESSAGE_BYTES,
    )?;
    validate_optional_hook_text(
        event.notification_type.as_deref(),
        "hook notification_type",
        MAX_HOOK_SHORT_TEXT_BYTES,
    )?;
    validate_optional_hook_text(
        event.level.as_deref(),
        "hook level",
        MAX_HOOK_SHORT_TEXT_BYTES,
    )
}

fn validate_optional_hook_text(value: Option<&str>, field: &str, max_bytes: usize) -> Result<()> {
    if let Some(value) = value
        && (value.len() > max_bytes || value.chars().any(char::is_control))
    {
        bail!("{field} must contain at most {max_bytes} bytes and no control characters");
    }
    Ok(())
}

fn validate_identifier(value: &str, field: impl Display) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid {
        bail!(
            "{field} must contain only ASCII letters, digits, '-', '_' or '.', up to {MAX_IDENTIFIER_BYTES} bytes"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook_event(kind: HookEventKind) -> HookEvent {
        HookEvent {
            kind,
            cwd: None,
            tool_name: None,
            message: None,
            notification_type: None,
            level: None,
        }
    }

    fn hook_request(provider_session_id: &str, event: HookEvent) -> RequestEnvelope {
        RequestEnvelope {
            id: "request-hook".to_owned(),
            request: Request::HookEvent {
                provider_session_id: provider_session_id.to_owned(),
                event,
            },
        }
    }

    fn valid_provider_session_id() -> &'static str {
        "01234567-89ab-4def-8123-456789abcdef"
    }

    #[test]
    fn request_round_trip_preserves_create_fields() {
        let request = RequestEnvelope {
            id: "request-1".to_owned(),
            request: Request::Create {
                cwd: r"C:\work tree\repo".to_owned(),
                prompt: Some("fix the terminal".to_owned()),
                model: Some("grok-code-fast-1".to_owned()),
                owner: Some("codex-thread-42".to_owned()),
                always_approve: true,
            },
        };

        let frame = encode_frame(&request).unwrap();
        assert_eq!(frame.last(), Some(&b'\n'));
        assert_eq!(decode_request(&frame).unwrap(), request);
    }

    #[test]
    fn request_round_trip_preserves_hook_event_fields() {
        let request = hook_request(
            "01234567-89AB-4DEF-8123-456789ABCDEF",
            HookEvent {
                kind: HookEventKind::Notification,
                cwd: Some(r"C:\work tree\repo".to_owned()),
                tool_name: Some("ask_user_question".to_owned()),
                message: Some("需要用户确认".to_owned()),
                notification_type: Some("permission_prompt".to_owned()),
                level: Some("info".to_owned()),
            },
        );

        let frame = encode_frame(&request).unwrap();
        assert_eq!(decode_request(&frame).unwrap(), request);
    }

    #[test]
    fn decodes_every_request_kind() {
        let requests = [
            Request::ServerStatus,
            Request::ServerStop,
            Request::List,
            Request::Show {
                session: "session-1".to_owned(),
            },
            Request::Read {
                session: "session-1".to_owned(),
                cursor: Some(12),
                limit: Some(200),
                wait_ms: Some(500),
            },
            Request::Send {
                session: "session-1".to_owned(),
                input: "\r".to_owned(),
            },
            Request::Write {
                session: "session-1".to_owned(),
                data_base64: BASE64.encode([0_u8, 0xff, b'\r']),
            },
            Request::Resize {
                session: "session-1".to_owned(),
                cols: 120,
                rows: 36,
            },
            Request::Wait {
                session: "session-1".to_owned(),
                for_condition: WaitCondition::TuiIdle,
                timeout_ms: Some(30_000),
            },
            Request::Close {
                session: "session-1".to_owned(),
            },
            Request::HookEvent {
                provider_session_id: valid_provider_session_id().to_owned(),
                event: hook_event(HookEventKind::Stop),
            },
        ];

        for (index, request) in requests.into_iter().enumerate() {
            let envelope = RequestEnvelope {
                id: format!("request-{index}"),
                request,
            };
            assert_eq!(
                decode_request(&encode_frame(&envelope).unwrap()).unwrap(),
                envelope
            );
        }
    }

    #[test]
    fn accepts_one_optional_line_ending_and_rejects_multiple_frames() {
        let json = br#"{"id":"r1","request":{"method":"server_status"}}"#;
        assert!(decode_request(json).is_ok());

        let mut crlf = json.to_vec();
        crlf.extend_from_slice(b"\r\n");
        assert!(decode_request(&crlf).is_ok());

        let mut multiple = json.to_vec();
        multiple.push(b'\n');
        multiple.extend_from_slice(json);
        multiple.push(b'\n');
        assert!(decode_request(&multiple).is_err());
    }

    #[test]
    fn rejects_oversized_frame() {
        let frame = vec![b'x'; MAX_FRAME_BYTES + 1];
        assert!(decode_request(&frame).is_err());
        assert!(encode_frame(&"x".repeat(MAX_FRAME_BYTES)).is_err());
    }

    #[test]
    fn rejects_invalid_request_ids_and_session_handles() {
        let invalid_id = br#"{"id":"bad id","request":{"method":"server_status"}}"#;
        assert!(decode_request(invalid_id).is_err());

        let invalid_handle =
            br#"{"id":"r1","request":{"method":"show","params":{"session":"../bad"}}}"#;
        assert!(decode_request(invalid_handle).is_err());
    }

    #[test]
    fn validates_optional_session_owner() {
        let request = |owner: Option<String>| RequestEnvelope {
            id: "request-1".to_owned(),
            request: Request::Create {
                cwd: ".".to_owned(),
                prompt: None,
                model: None,
                owner,
                always_approve: false,
            },
        };

        for owner in [
            None,
            Some("codex-thread-42".to_owned()),
            Some("人工会话".to_owned()),
        ] {
            let envelope = request(owner);
            assert!(decode_request(&encode_frame(&envelope).unwrap()).is_ok());
        }
        for owner in [Some(" ".to_owned()), Some("bad\nowner".to_owned())] {
            let envelope = request(owner);
            assert!(decode_request(&encode_frame(&envelope).unwrap()).is_err());
        }
        let oversized = request(Some("x".repeat(MAX_OWNER_BYTES + 1)));
        assert!(decode_request(&encode_frame(&oversized).unwrap()).is_err());
    }

    #[test]
    fn validates_provider_session_id_as_canonical_uuid() {
        for provider_session_id in [
            valid_provider_session_id(),
            "01234567-89AB-4DEF-8123-456789ABCDEF",
        ] {
            let request =
                hook_request(provider_session_id, hook_event(HookEventKind::SessionStart));
            assert!(decode_request(&encode_frame(&request).unwrap()).is_ok());
        }

        for provider_session_id in [
            "0123456789ab-4def-8123-456789abcdef",
            "01234567-89ab4-def-8123-456789abcdef",
            "01234567-89ab-4def-8123-456789abcdeg",
            "{01234567-89ab-4def-8123-456789abcdef}",
            "01234567-89ab-4def-8123-456789abcde",
        ] {
            let request =
                hook_request(provider_session_id, hook_event(HookEventKind::SessionStart));
            assert!(decode_request(&encode_frame(&request).unwrap()).is_err());
        }
    }

    #[test]
    fn validates_hook_event_field_limits() {
        let maximum = HookEvent {
            kind: HookEventKind::PreToolUse,
            cwd: Some("c".repeat(MAX_HOOK_CWD_BYTES)),
            tool_name: Some("t".repeat(MAX_HOOK_SHORT_TEXT_BYTES)),
            message: Some("m".repeat(MAX_HOOK_MESSAGE_BYTES)),
            notification_type: Some("n".repeat(MAX_HOOK_SHORT_TEXT_BYTES)),
            level: Some("l".repeat(MAX_HOOK_SHORT_TEXT_BYTES)),
        };
        let request = hook_request(valid_provider_session_id(), maximum);
        assert!(decode_request(&encode_frame(&request).unwrap()).is_ok());

        let oversized = [
            HookEvent {
                cwd: Some("c".repeat(MAX_HOOK_CWD_BYTES + 1)),
                ..hook_event(HookEventKind::SessionStart)
            },
            HookEvent {
                tool_name: Some("t".repeat(MAX_HOOK_SHORT_TEXT_BYTES + 1)),
                ..hook_event(HookEventKind::PreToolUse)
            },
            HookEvent {
                message: Some("m".repeat(MAX_HOOK_MESSAGE_BYTES + 1)),
                ..hook_event(HookEventKind::Notification)
            },
            HookEvent {
                notification_type: Some("n".repeat(MAX_HOOK_SHORT_TEXT_BYTES + 1)),
                ..hook_event(HookEventKind::Notification)
            },
            HookEvent {
                level: Some("l".repeat(MAX_HOOK_SHORT_TEXT_BYTES + 1)),
                ..hook_event(HookEventKind::Notification)
            },
        ];
        for event in oversized {
            let request = hook_request(valid_provider_session_id(), event);
            assert!(decode_request(&encode_frame(&request).unwrap()).is_err());
        }
    }

    #[test]
    fn rejects_forbidden_hook_event_characters() {
        let invalid = [
            HookEvent {
                cwd: Some("bad\0cwd".to_owned()),
                ..hook_event(HookEventKind::SessionStart)
            },
            HookEvent {
                tool_name: Some("bad\rtool".to_owned()),
                ..hook_event(HookEventKind::PreToolUse)
            },
            HookEvent {
                message: Some("bad\u{7f}message".to_owned()),
                ..hook_event(HookEventKind::Notification)
            },
            HookEvent {
                notification_type: Some("bad\tnotification".to_owned()),
                ..hook_event(HookEventKind::Notification)
            },
            HookEvent {
                level: Some("bad\nlevel".to_owned()),
                ..hook_event(HookEventKind::Notification)
            },
        ];
        for event in invalid {
            let request = hook_request(valid_provider_session_id(), event);
            assert!(decode_request(&encode_frame(&request).unwrap()).is_err());
        }
    }

    #[test]
    fn validates_raw_terminal_writes() {
        let request = |data_base64: String| RequestEnvelope {
            id: "request-1".to_owned(),
            request: Request::Write {
                session: "session-1".to_owned(),
                data_base64,
            },
        };

        let maximum = request(BASE64.encode(vec![0x5a; MAX_WRITE_BYTES]));
        assert!(decode_request(&encode_frame(&maximum).unwrap()).is_ok());

        let empty = request(String::new());
        assert!(decode_request(&encode_frame(&empty).unwrap()).is_err());

        let oversized = request(BASE64.encode(vec![0x5a; MAX_WRITE_BYTES + 1]));
        assert!(decode_request(&encode_frame(&oversized).unwrap()).is_err());

        let malformed = request("not-base64!".to_owned());
        assert!(decode_request(&encode_frame(&malformed).unwrap()).is_err());
    }

    #[test]
    fn validates_terminal_resize_bounds() {
        let request = |cols, rows| RequestEnvelope {
            id: "request-1".to_owned(),
            request: Request::Resize {
                session: "session-1".to_owned(),
                cols,
                rows,
            },
        };

        for (cols, rows) in [(20, 5), (500, 200), (120, 36)] {
            let envelope = request(cols, rows);
            assert!(decode_request(&encode_frame(&envelope).unwrap()).is_ok());
        }
        for (cols, rows) in [(19, 36), (501, 36), (120, 4), (120, 201)] {
            let envelope = request(cols, rows);
            assert!(decode_request(&encode_frame(&envelope).unwrap()).is_err());
        }
    }

    #[test]
    fn response_round_trip_validates_session_and_read_cursor() {
        let response = ResponseEnvelope::success(
            "request-1",
            ResponseResult::Read(ReadResult {
                session: "session-1".to_owned(),
                cursor: 10,
                next_cursor: 13,
                data_base64: BASE64.encode(b"abc"),
                plain_text: Some("abc".to_owned()),
                screen: Some("screen snapshot".to_owned()),
                truncated: false,
                eof: false,
            }),
        );

        assert_eq!(
            decode_response(&encode_frame(&response).unwrap()).unwrap(),
            response
        );
    }

    #[test]
    fn read_cursor_is_a_byte_offset() {
        for next_cursor in [9, 10, 15] {
            let response = ResponseEnvelope::success(
                "request-1",
                ResponseResult::Read(ReadResult {
                    session: "session-1".to_owned(),
                    cursor: 10,
                    next_cursor,
                    data_base64: BASE64.encode(b"data"),
                    plain_text: None,
                    screen: None,
                    truncated: false,
                    eof: false,
                }),
            );
            assert!(decode_response(&encode_frame(&response).unwrap()).is_err());
        }
    }

    #[test]
    fn rejects_inconsistent_response_envelopes() {
        let missing_result = br#"{"id":"r1","ok":true}"#;
        assert!(decode_response(missing_result).is_err());

        let both = br#"{"id":"r1","ok":false,"result":{"type":"accepted","value":{"accepted":true}},"error":{"code":"failed","message":"no"}}"#;
        assert!(decode_response(both).is_err());

        let empty_error = ResponseEnvelope::failure("r1", "failed", " ");
        assert!(decode_response(&encode_frame(&empty_error).unwrap()).is_err());
    }

    #[test]
    fn serializes_all_session_phases_in_snake_case() {
        let phases = [
            (SessionPhase::Starting, "\"starting\""),
            (SessionPhase::Running, "\"running\""),
            (SessionPhase::Idle, "\"idle\""),
            (SessionPhase::Exited, "\"exited\""),
            (SessionPhase::Failed, "\"failed\""),
            (SessionPhase::Stopped, "\"stopped\""),
        ];
        for (phase, expected) in phases {
            assert_eq!(serde_json::to_string(&phase).unwrap(), expected);
        }
        assert_eq!(
            serde_json::to_string(&WaitCondition::TuiIdle).unwrap(),
            "\"tui_idle\""
        );
    }

    #[test]
    fn serializes_all_hook_event_kinds_in_snake_case() {
        let kinds = [
            (HookEventKind::SessionStart, "\"session_start\""),
            (HookEventKind::UserPromptSubmit, "\"user_prompt_submit\""),
            (HookEventKind::Stop, "\"stop\""),
            (HookEventKind::StopFailure, "\"stop_failure\""),
            (HookEventKind::SessionEnd, "\"session_end\""),
            (HookEventKind::PreToolUse, "\"pre_tool_use\""),
            (HookEventKind::PostToolUse, "\"post_tool_use\""),
            (
                HookEventKind::PostToolUseFailure,
                "\"post_tool_use_failure\"",
            ),
            (HookEventKind::Notification, "\"notification\""),
        ];
        for (kind, expected) in kinds {
            assert_eq!(serde_json::to_string(&kind).unwrap(), expected);
        }

        let activities = [
            (HookActivity::Unknown, "\"unknown\""),
            (HookActivity::Working, "\"working\""),
            (HookActivity::Waiting, "\"waiting\""),
            (HookActivity::Done, "\"done\""),
        ];
        for (activity, expected) in activities {
            assert_eq!(serde_json::to_string(&activity).unwrap(), expected);
        }
    }

    #[test]
    fn validates_session_timestamps_and_wait_state() {
        let invalid_session = ResponseEnvelope::success(
            "r1",
            ResponseResult::Session(SessionState {
                session: "session-1".to_owned(),
                owner: Some("codex-thread-42".to_owned()),
                phase: SessionPhase::Running,
                title: Some("Grok".to_owned()),
                cwd: ".".to_owned(),
                model: None,
                always_approve: false,
                process_id: Some(42),
                screen: Some("screen".to_owned()),
                rows: 36,
                cols: 120,
                screen_ansi_base64: BASE64.encode(b"screen"),
                last_cursor: 12,
                last_output_at_ms: Some(15),
                created_at_ms: 20,
                updated_at_ms: 10,
                exit_code: None,
                error: None,
                activity: HookActivity::Waiting,
                hook_event: Some(HookEventKind::PreToolUse),
                hook_at_ms: Some(10),
                tool_name: Some("ask_user_question".to_owned()),
                waiting_reason: Some("Waiting for an answer".to_owned()),
            }),
        );
        assert!(decode_response(&encode_frame(&invalid_session).unwrap()).is_err());

        let invalid_wait = ResponseEnvelope::success(
            "r2",
            ResponseResult::Wait(WaitResult {
                session: "session-1".to_owned(),
                condition: WaitCondition::TuiIdle,
                satisfied: true,
                timed_out: true,
                phase: SessionPhase::Idle,
                exit_code: None,
                blocked_reason: None,
            }),
        );
        assert!(decode_response(&encode_frame(&invalid_wait).unwrap()).is_err());
    }
}
