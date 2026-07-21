use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use tungstenite::{
    Message, WebSocket,
    handshake::derive_accept_key,
    protocol::{Role, WebSocketConfig},
};

use crate::{
    protocol::{
        Request, ResponseEnvelope, ResponseResult, ServerInfo, decode_request, decode_write_data,
        validate_client_session_id, validate_owner, validate_session_handle,
        validate_terminal_size,
    },
    session::{OrphanPolicy, SessionHost},
    transport::{call_anonymous, read_frame, runtime_name, write_response},
    version_check::{CHECK_INTERVAL, VersionChecker},
};

/// Bound WebSocket text/binary payload size (server and client).
const WEB_EVENTS_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// Max idle sleep between client-frame polls so interactive keys stay low-latency.
/// Host Condvar still wakes immediately on session revisions.
const WEB_EVENTS_CLIENT_POLL: Duration = Duration::from_millis(25);
/// Refresh every managed Codex lease while a WebUI event socket remains live.
/// This stays well below the minimum configurable 30-second lease.
const WEB_EVENTS_LEASE_REFRESH: Duration = Duration::from_secs(10);
/// Cap inbound request-id length for WebUI command frames.
const WEB_EVENTS_MAX_REQUEST_ID_BYTES: usize = 128;

pub(crate) fn run() -> Result<()> {
    let name = runtime_name()?;
    let listener = match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => listener,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::AddrInUse | ErrorKind::PermissionDenied
            ) =>
        {
            if call_anonymous(Request::ServerStatus, false).is_ok_and(|response| {
                response.ok && matches!(response.result, Some(ResponseResult::ServerInfo(_)))
            }) {
                return Ok(());
            }
            return Err(error).context("runtime pipe name is occupied by another process");
        }
        Err(error) => return Err(error).context("failed to bind the runtime named pipe"),
    };

    let web_listener = bind_web_ui();
    let web_url = web_listener
        .as_ref()
        .and_then(|listener| listener.local_addr().ok())
        .map(|address| format!("http://{address}/"));
    let state = Arc::new(RuntimeState {
        host: SessionHost::new(OrphanPolicy::from_env()?),
        started_at_ms: now_millis(),
        stopping: AtomicBool::new(false),
        web_url,
        version_checker: Arc::new(VersionChecker::new()),
    });
    if let Some(listener) = web_listener {
        let web_state = Arc::clone(&state);
        thread::spawn(move || run_web_ui(listener, web_state));
    }
    {
        let reaper_state = Arc::clone(&state);
        thread::spawn(move || run_orphan_reaper(reaper_state));
    }
    {
        let version_state = Arc::clone(&state);
        thread::spawn(move || run_version_checker(version_state));
    }

    for connection in listener.incoming() {
        let connection = match connection {
            Ok(connection) => connection,
            Err(error) => {
                if state.stopping.load(Ordering::Acquire) {
                    break;
                }
                eprintln!("grok-bridge server: failed to accept client: {error}");
                continue;
            }
        };
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let state = Arc::clone(&state);
        thread::spawn(move || handle_connection(connection, state));
    }

    state.host.shutdown_all()?;
    Ok(())
}

struct RuntimeState {
    host: SessionHost,
    started_at_ms: u64,
    stopping: AtomicBool,
    web_url: Option<String>,
    version_checker: Arc<VersionChecker>,
}

fn handle_connection(stream: Stream, state: Arc<RuntimeState>) {
    let mut connection = BufReader::new(stream);
    let frame = match read_frame(&mut connection) {
        Ok(frame) => frame,
        Err(error) => {
            let response =
                ResponseEnvelope::failure("invalid-request", "invalid_frame", format!("{error:#}"));
            let _ = write_response(connection.get_mut(), &response);
            return;
        }
    };
    let envelope = match decode_request(&frame) {
        Ok(envelope) => envelope,
        Err(error) => {
            let response = ResponseEnvelope::failure(
                "invalid-request",
                "invalid_request",
                format!("{error:#}"),
            );
            let _ = write_response(connection.get_mut(), &response);
            return;
        }
    };

    let request_id = envelope.id;
    let client_session_id = envelope.client_session_id;
    let refresh_after_response = !matches!(envelope.request, Request::CloseCodex);
    if let Some(client_session_id) = client_session_id.as_deref()
        && let Err(error) = state.host.touch_client(client_session_id)
    {
        let response =
            ResponseEnvelope::failure(request_id, "invalid_client_session", format!("{error:#}"));
        let _ = write_response(connection.get_mut(), &response);
        return;
    }
    let (response, stop_after_response) =
        match dispatch(&state, envelope.request, client_session_id.as_deref()) {
            Ok((result, stop)) => (ResponseEnvelope::success(request_id, result), stop),
            Err(error) => (
                ResponseEnvelope::failure(request_id, "request_failed", format!("{error:#}")),
                false,
            ),
        };
    let wrote_response = write_response(connection.get_mut(), &response).is_ok();
    if wrote_response
        && response.ok
        && refresh_after_response
        && let Some(client_session_id) = client_session_id.as_deref()
    {
        let _ = state.host.touch_client(client_session_id);
    }
    if stop_after_response {
        wake_listener();
    }
}

fn dispatch(
    state: &RuntimeState,
    request: Request,
    client_session_id: Option<&str>,
) -> Result<(ResponseResult, bool)> {
    let result = match request {
        Request::ServerStatus => ResponseResult::ServerInfo(state.server_info()),
        Request::ServerStop => {
            state.stopping.store(true, Ordering::Release);
            state.host.shutdown_all()?;
            return Ok((ResponseResult::Accepted { accepted: true }, true));
        }
        Request::Heartbeat => {
            let client_session_id = client_session_id.context(
                "heartbeat requires CODEX_THREAD_ID or CODEX_SESSION_ID in the client environment",
            )?;
            state.host.touch_client(client_session_id)?;
            ResponseResult::Accepted { accepted: true }
        }
        Request::CloseCodex => {
            let client_session_id = client_session_id.context(
                "close_codex requires CODEX_THREAD_ID or CODEX_SESSION_ID in the client environment",
            )?;
            ResponseResult::CloseGroup(state.host.close_client(client_session_id)?)
        }
        Request::Create {
            cwd,
            prompt,
            model,
            owner,
            always_approve,
        } => ResponseResult::Session(Box::new(state.host.create(
            &cwd,
            prompt,
            model,
            owner,
            always_approve,
            client_session_id.map(str::to_owned),
        )?)),
        Request::List => ResponseResult::Sessions {
            sessions: state.host.list()?,
        },
        Request::Show { session } => ResponseResult::Session(Box::new(state.host.show(&session)?)),
        Request::Read {
            session,
            cursor,
            limit,
            wait_ms,
        } => ResponseResult::Read(state.host.read(
            &session,
            cursor.unwrap_or(0),
            limit.unwrap_or(4096) as usize,
            wait_ms.unwrap_or(0),
        )?),
        Request::Send { session, input } => {
            ResponseResult::Session(Box::new(state.host.send(&session, input)?))
        }
        Request::Write {
            session,
            data_base64,
        } => {
            state
                .host
                .write_raw(&session, decode_write_data(&data_base64)?)?;
            ResponseResult::Accepted { accepted: true }
        }
        Request::Resize {
            session,
            cols,
            rows,
        } => {
            state.host.resize(&session, cols, rows)?;
            ResponseResult::Accepted { accepted: true }
        }
        Request::Wait {
            session,
            for_condition,
            timeout_ms,
        } => ResponseResult::Wait(state.host.wait(
            &session,
            for_condition,
            timeout_ms.unwrap_or(300_000),
        )?),
        Request::Close { session } => ResponseResult::Accepted {
            accepted: state.host.close(&session)?,
        },
        Request::HookEvent {
            provider_session_id,
            event,
        } => ResponseResult::Accepted {
            accepted: state.host.apply_hook_event(&provider_session_id, event)?,
        },
    };
    Ok((result, false))
}

fn run_orphan_reaper(state: Arc<RuntimeState>) {
    while !state.stopping.load(Ordering::Acquire) {
        thread::sleep(Duration::from_secs(5));
        if state.stopping.load(Ordering::Acquire) {
            return;
        }
        // Only real close/removal paths notify the WebUI revision bus.
        if let Err(error) = state.host.reap_orphans() {
            eprintln!("grok-bridge server: orphan cleanup failed: {error:#}");
        }
    }
}

fn run_version_checker(state: Arc<RuntimeState>) {
    loop {
        if state.stopping.load(Ordering::Acquire) {
            return;
        }
        state.version_checker.refresh();
        let mut remaining = CHECK_INTERVAL;
        while remaining > Duration::ZERO {
            if state.stopping.load(Ordering::Acquire) {
                return;
            }
            let slice = remaining.min(Duration::from_secs(30));
            thread::sleep(slice);
            remaining = remaining.saturating_sub(slice);
        }
    }
}

impl RuntimeState {
    fn server_info(&self) -> ServerInfo {
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            process_id: std::process::id(),
            started_at_ms: self.started_at_ms,
            active_sessions: self.host.active_count(),
            web_url: self.web_url.clone(),
            stopping: self.stopping.load(Ordering::Acquire),
        }
    }
}

fn bind_web_ui() -> Option<TcpListener> {
    let address = env::var("GROK_BRIDGE_WEB_ADDR").unwrap_or_else(|_| "127.0.0.1:47653".to_owned());
    match TcpListener::bind(&address) {
        Ok(listener) => Some(listener),
        Err(error) => {
            eprintln!("grok-bridge server: WebUI unavailable at {address}: {error}");
            None
        }
    }
}

fn run_web_ui(listener: TcpListener, state: Arc<RuntimeState>) {
    for connection in listener.incoming() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        match connection {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || handle_web_connection(stream, state));
            }
            Err(error) => eprintln!("grok-bridge server: WebUI accept failed: {error}"),
        }
    }
}

fn handle_web_connection(mut stream: TcpStream, state: Arc<RuntimeState>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = match read_http_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            let _ = write_http(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                &error,
            );
            return;
        }
    };
    if request.method == "GET" && request.path == "/api/events" {
        handle_events_websocket(stream, state, request);
        return;
    }
    if request.method == "GET"
        && let Some(asset) = static_web_asset(&request.path)
    {
        let _ = write_http_bytes(&mut stream, "200 OK", asset.content_type, asset.body);
        return;
    }
    let method = request.method.as_str();
    let path = request.path.as_str();
    let bridge_header = request.bridge_header;
    match (method, path) {
        ("GET", "/api/sessions") => match state.host.list_web().and_then(|sessions| {
            serde_json::to_string(&sessions).context("failed to encode WebUI sessions")
        }) {
            Ok(body) => {
                let _ = write_http(&mut stream, "200 OK", "application/json", &body);
            }
            Err(error) => {
                let _ = write_http(
                    &mut stream,
                    "500 Internal Server Error",
                    "text/plain; charset=utf-8",
                    &format!("{error:#}"),
                );
            }
        },
        ("GET", "/api/version") => {
            let body = serde_json::to_string(&state.version_checker.status())
                .unwrap_or_else(|_| {
                    r#"{"current":"unknown","update_available":false,"release_url":"https://github.com/luodaoyi/grok-bridge-rs/releases/latest"}"#.to_owned()
                });
            let _ = write_http(&mut stream, "200 OK", "application/json", &body);
        }
        ("POST", path) if path.starts_with("/api/clients/") => {
            let Some(encoded_client) = close_path_segment(path, "/api/clients/") else {
                let _ = write_http(
                    &mut stream,
                    "404 Not Found",
                    "text/plain; charset=utf-8",
                    "not found",
                );
                return;
            };
            if !bridge_header {
                let _ = write_http(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain; charset=utf-8",
                    "missing WebUI request header",
                );
                return;
            }
            let client_session_id = match percent_decode_path_segment(encoded_client) {
                Ok(client_session_id) => client_session_id,
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        &error,
                    );
                    return;
                }
            };
            if let Err(error) = validate_client_session_id(&client_session_id) {
                let _ = write_http(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    &format!("{error:#}"),
                );
                return;
            }
            match state.host.close_client(&client_session_id) {
                Ok(result) => {
                    let body = serde_json::to_string(&result)
                        .unwrap_or_else(|_| r#"{"matched":0,"closed":0,"failures":[]}"#.to_owned());
                    let _ = write_http(&mut stream, "200 OK", "application/json", &body);
                }
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain; charset=utf-8",
                        &format!("{error:#}"),
                    );
                }
            }
        }
        ("POST", path) if path.starts_with("/api/owners/") => {
            let Some(encoded_owner) = close_path_segment(path, "/api/owners/") else {
                let _ = write_http(
                    &mut stream,
                    "404 Not Found",
                    "text/plain; charset=utf-8",
                    "not found",
                );
                return;
            };
            if !bridge_header {
                let _ = write_http(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain; charset=utf-8",
                    "missing WebUI request header",
                );
                return;
            }
            let owner = match percent_decode_path_segment(encoded_owner) {
                Ok(owner) => owner,
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        &error,
                    );
                    return;
                }
            };
            if let Err(error) = validate_owner(&owner) {
                let _ = write_http(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    &format!("{error:#}"),
                );
                return;
            }
            match state.host.close_owner(&owner) {
                Ok(result) => {
                    let body = serde_json::json!({
                        "matched": result.matched,
                        "closed": result.closed,
                        "failures": result.failures,
                    })
                    .to_string();
                    let _ = write_http(&mut stream, "200 OK", "application/json", &body);
                }
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain; charset=utf-8",
                        &format!("{error:#}"),
                    );
                }
            }
        }
        ("POST", path) if path.starts_with("/api/sessions/") => {
            let Some(handle) = close_path_segment(path, "/api/sessions/") else {
                let _ = write_http(
                    &mut stream,
                    "404 Not Found",
                    "text/plain; charset=utf-8",
                    "not found",
                );
                return;
            };
            if !bridge_header {
                let _ = write_http(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain; charset=utf-8",
                    "missing WebUI request header",
                );
                return;
            }
            match state.host.close(handle) {
                Ok(closed) => {
                    let body = format!(r#"{{"accepted":{closed}}}"#);
                    let _ = write_http(&mut stream, "200 OK", "application/json", &body);
                }
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "404 Not Found",
                        "text/plain; charset=utf-8",
                        &format!("{error:#}"),
                    );
                }
            }
        }
        _ => {
            let _ = write_http(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                "not found",
            );
        }
    }
}

fn handle_events_websocket(
    mut stream: TcpStream,
    state: Arc<RuntimeState>,
    request: ParsedHttpRequest,
) {
    if let Err(error) = validate_events_websocket_request(&request) {
        let status = if error.starts_with("origin") {
            "403 Forbidden"
        } else {
            "400 Bad Request"
        };
        let _ = write_http(&mut stream, status, "text/plain; charset=utf-8", &error);
        return;
    }
    let key = request
        .sec_websocket_key
        .as_deref()
        .expect("validated websocket key");
    let accept = derive_accept_key(key.as_bytes());
    if write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .is_err()
    {
        return;
    }
    // Host Condvar is the primary sleep; keep socket reads short so a quiet
    // client socket cannot delay event pushes after a revision wake-up.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(WEB_EVENTS_MAX_MESSAGE_BYTES);
    config.max_frame_size = Some(WEB_EVENTS_MAX_MESSAGE_BYTES);
    // Eager writes so session events are not stuck in the 128 KiB default buffer.
    config.write_buffer_size = 0;
    config.read_buffer_size = 8 * 1024;
    let mut websocket = WebSocket::from_raw_socket(stream, Role::Server, Some(config));
    run_events_websocket(&mut websocket, &state);
}

fn run_events_websocket(websocket: &mut WebSocket<TcpStream>, state: &RuntimeState) {
    if let Err(error) = state.host.touch_web_clients() {
        eprintln!("grok-bridge server: WebUI lease refresh failed: {error:#}");
    }
    let mut next_lease_refresh = Instant::now() + WEB_EVENTS_LEASE_REFRESH;
    let mut cursors = HashMap::new();
    let mut seen_revision = state.host.revision();
    if !send_web_events(websocket, state, &mut cursors, true) {
        return;
    }

    while !state.stopping.load(Ordering::Acquire) {
        if Instant::now() >= next_lease_refresh {
            if let Err(error) = state.host.touch_web_clients() {
                eprintln!("grok-bridge server: WebUI lease refresh failed: {error:#}");
            }
            next_lease_refresh = Instant::now() + WEB_EVENTS_LEASE_REFRESH;
        }
        // Service inbound terminal_input / terminal_resize before sleeping so
        // interactive keystrokes never wait behind a multi-second idle timeout.
        match poll_websocket_client(websocket, state) {
            WsClientAction::Continue => {}
            WsClientAction::Close => return,
        }

        let now = now_millis();
        let lease_deadline = state
            .host
            .next_client_lifecycle_deadline_ms()
            .ok()
            .flatten();
        // Sleep until the next host revision *or* the next pure-time lease
        // transition, but never longer than WEB_EVENTS_CLIENT_POLL so client
        // frames stay low-latency. Timeouts without a revision/lease signal
        // never push frames.
        let wait = match lease_deadline {
            Some(deadline) if deadline > now => Duration::from_millis(deadline - now)
                .min(Duration::from_secs(30))
                .max(Duration::from_millis(1))
                .min(WEB_EVENTS_CLIENT_POLL),
            Some(_) => Duration::from_millis(1),
            None => WEB_EVENTS_CLIENT_POLL,
        };
        let current = state.host.wait_revision(seen_revision, wait);
        match poll_websocket_client(websocket, state) {
            WsClientAction::Continue => {}
            WsClientAction::Close => return,
        }
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let now_after = now_millis();
        let lease_due = lease_deadline.is_some_and(|deadline| now_after >= deadline);
        if current == seen_revision && !lease_due {
            continue;
        }
        if current != seen_revision {
            seen_revision = current;
        }
        if !send_web_events(websocket, state, &mut cursors, false) {
            return;
        }
    }

    let _ = websocket.close(None);
}

/// Plan frames from immutable cursors, send each frame, and commit cursor
/// advances only after that frame is successfully written. Oversize/encode
/// failures never advance cursors and never silently drop committed bytes.
fn send_web_events(
    websocket: &mut WebSocket<TcpStream>,
    state: &RuntimeState,
    cursors: &mut HashMap<String, u64>,
    force_reset: bool,
) -> bool {
    let frames =
        match state
            .host
            .plan_web_events(cursors, force_reset, WEB_EVENTS_MAX_MESSAGE_BYTES)
        {
            Ok(frames) => frames,
            Err(error) => {
                eprintln!("grok-bridge server: WebUI events plan failed: {error:#}");
                return true;
            }
        };

    for frame in frames {
        let payload = match serde_json::to_string(&frame.message) {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!("grok-bridge server: WebUI events encode failed: {error}");
                // Do not commit any remaining planned cursors.
                return true;
            }
        };
        if payload.len() > WEB_EVENTS_MAX_MESSAGE_BYTES {
            eprintln!(
                "grok-bridge server: WebUI events frame exceeds {} bytes; leaving cursors uncommitted",
                WEB_EVENTS_MAX_MESSAGE_BYTES
            );
            return true;
        }
        if websocket
            .send(Message::text(payload))
            .and_then(|()| websocket.flush())
            .is_err()
        {
            return false;
        }
        for (session, cursor) in frame.cursor_commits {
            cursors.insert(session, cursor);
        }
        for session in frame.cursor_drops {
            cursors.remove(&session);
        }
    }
    true
}

enum WsClientAction {
    Continue,
    Close,
}

fn poll_websocket_client(
    websocket: &mut WebSocket<TcpStream>,
    state: &RuntimeState,
) -> WsClientAction {
    // Drain a bounded number of control/application frames so a noisy client
    // cannot pin this connection thread forever.
    for _ in 0..32 {
        match websocket.read() {
            Ok(Message::Ping(payload)) => {
                if websocket.send(Message::Pong(payload)).is_err() {
                    return WsClientAction::Close;
                }
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                let _ = websocket.close(None);
                return WsClientAction::Close;
            }
            Ok(Message::Text(text)) => {
                if handle_web_events_client_text(websocket, state, text.as_str()).is_err() {
                    return WsClientAction::Close;
                }
            }
            // Binary frames are not part of the JSON command protocol.
            Ok(Message::Binary(_)) | Ok(Message::Frame(_)) => {}
            Err(tungstenite::Error::Io(error))
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
            {
                return WsClientAction::Continue;
            }
            Err(tungstenite::Error::ConnectionClosed)
            | Err(tungstenite::Error::AlreadyClosed)
            | Err(tungstenite::Error::Protocol(_)) => {
                return WsClientAction::Close;
            }
            Err(_) => return WsClientAction::Close,
        }
    }
    WsClientAction::Continue
}

/// Client → server command on `/api/events` (JSON text only).
#[derive(Clone, Debug, PartialEq, Eq)]
enum WebEventsClientCommand {
    TerminalInput {
        id: Option<String>,
        session: String,
        data_base64: String,
    },
    TerminalResize {
        id: Option<String>,
        session: String,
        cols: u16,
        rows: u16,
    },
}

/// Parse a single WebUI client command without panicking on junk.
/// Unknown types yield `Ok(None)` (ignored). Malformed known types yield `Err`.
fn parse_web_events_client_command(text: &str) -> Result<Option<WebEventsClientCommand>, String> {
    if text.len() > WEB_EVENTS_MAX_MESSAGE_BYTES {
        return Err("message exceeds size limit".to_owned());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("invalid JSON: {error}"))?;
    let Some(object) = value.as_object() else {
        return Err("message must be a JSON object".to_owned());
    };
    let message_type = object
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let id = match object.get("id") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => {
            if value.len() > WEB_EVENTS_MAX_REQUEST_ID_BYTES {
                return Err("request id is too long".to_owned());
            }
            Some(value.clone())
        }
        Some(_) => return Err("request id must be a string".to_owned()),
    };

    match message_type {
        "terminal_input" => {
            let session = object
                .get("session")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            validate_session_handle(&session).map_err(|error| format!("{error:#}"))?;
            let data_base64 = object
                .get("data_base64")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            if data_base64.is_empty() {
                return Err("data_base64 is required".to_owned());
            }
            Ok(Some(WebEventsClientCommand::TerminalInput {
                id,
                session,
                data_base64,
            }))
        }
        "terminal_resize" => {
            let session = object
                .get("session")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            validate_session_handle(&session).map_err(|error| format!("{error:#}"))?;
            let cols = object
                .get("cols")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| "cols is required".to_owned())?;
            let rows = object
                .get("rows")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| "rows is required".to_owned())?;
            let cols = u16::try_from(cols).map_err(|_| "cols out of range".to_owned())?;
            let rows = u16::try_from(rows).map_err(|_| "rows out of range".to_owned())?;
            Ok(Some(WebEventsClientCommand::TerminalResize {
                id,
                session,
                cols,
                rows,
            }))
        }
        // Unknown / push-only types (e.g. future clients): ignore safely.
        _ => Ok(None),
    }
}

/// Apply a parsed command through SessionHost, reusing write_raw / resize.
fn apply_web_events_client_command(
    host: &SessionHost,
    command: &WebEventsClientCommand,
) -> Result<(), String> {
    // An inbound command proves the WebUI is attached right now. Refresh before
    // touching the PTY so a provisional orphan claim is canceled before input
    // or resize is accepted.
    host.touch_web_clients()
        .map_err(|error| format!("{error:#}"))?;
    match command {
        WebEventsClientCommand::TerminalInput {
            session,
            data_base64,
            ..
        } => {
            let data = decode_write_data(data_base64).map_err(|error| format!("{error:#}"))?;
            host.write_raw(session, data)
                .map(|_| ())
                .map_err(|error| format!("{error:#}"))
        }
        WebEventsClientCommand::TerminalResize {
            session,
            cols,
            rows,
            ..
        } => {
            validate_terminal_size(*cols, *rows).map_err(|error| format!("{error:#}"))?;
            host.resize(session, *cols, *rows)
                .map(|_| ())
                .map_err(|error| format!("{error:#}"))
        }
    }
}

fn web_events_result_type(command: &WebEventsClientCommand) -> &'static str {
    match command {
        WebEventsClientCommand::TerminalInput { .. } => "input_result",
        WebEventsClientCommand::TerminalResize { .. } => "resize_result",
    }
}

fn web_events_command_session(command: &WebEventsClientCommand) -> &str {
    match command {
        WebEventsClientCommand::TerminalInput { session, .. }
        | WebEventsClientCommand::TerminalResize { session, .. } => session.as_str(),
    }
}

fn web_events_command_id(command: &WebEventsClientCommand) -> Option<&str> {
    match command {
        WebEventsClientCommand::TerminalInput { id, .. }
        | WebEventsClientCommand::TerminalResize { id, .. } => id.as_deref(),
    }
}

/// Build a result envelope that never collides with `{ type: "sessions" }` push frames.
fn build_web_events_command_result(
    result_type: &str,
    id: Option<&str>,
    session: Option<&str>,
    ok: bool,
    error: Option<&str>,
) -> String {
    let mut map = serde_json::Map::new();
    map.insert(
        "type".to_owned(),
        serde_json::Value::String(result_type.to_owned()),
    );
    map.insert("ok".to_owned(), serde_json::Value::Bool(ok));
    if let Some(id) = id {
        map.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    }
    if let Some(session) = session {
        map.insert(
            "session".to_owned(),
            serde_json::Value::String(session.to_owned()),
        );
    }
    if let Some(error) = error {
        map.insert(
            "error".to_owned(),
            serde_json::Value::String(error.to_owned()),
        );
    }
    serde_json::Value::Object(map).to_string()
}

fn send_web_events_command_result(
    websocket: &mut WebSocket<TcpStream>,
    result_type: &str,
    id: Option<&str>,
    session: Option<&str>,
    ok: bool,
    error: Option<&str>,
) -> Result<(), ()> {
    let payload = build_web_events_command_result(result_type, id, session, ok, error);
    if payload.len() > WEB_EVENTS_MAX_MESSAGE_BYTES {
        return Err(());
    }
    websocket
        .send(Message::text(payload))
        .and_then(|()| websocket.flush())
        .map_err(|_| ())
}

fn handle_web_events_client_text(
    websocket: &mut WebSocket<TcpStream>,
    state: &RuntimeState,
    text: &str,
) -> Result<(), ()> {
    match parse_web_events_client_command(text) {
        Ok(None) => Ok(()),
        Ok(Some(command)) => match apply_web_events_client_command(&state.host, &command) {
            Ok(()) => {
                // Success is silent for terminal_input (high frequency). Resize
                // still gets a positive ack so the UI can confirm PTY apply.
                if matches!(command, WebEventsClientCommand::TerminalResize { .. }) {
                    send_web_events_command_result(
                        websocket,
                        web_events_result_type(&command),
                        web_events_command_id(&command),
                        Some(web_events_command_session(&command)),
                        true,
                        None,
                    )?;
                }
                Ok(())
            }
            Err(error) => send_web_events_command_result(
                websocket,
                web_events_result_type(&command),
                web_events_command_id(&command),
                Some(web_events_command_session(&command)),
                false,
                Some(&error),
            ),
        },
        Err(error) => {
            // Malformed known-shape or generic junk that failed parse: never
            // touch PTY; return a generic input_result when possible.
            send_web_events_command_result(
                websocket,
                "input_result",
                None,
                None,
                false,
                Some(&error),
            )
        }
    }
}

fn validate_events_websocket_request(request: &ParsedHttpRequest) -> Result<(), String> {
    if request.method != "GET" {
        return Err("WebSocket upgrade requires GET".to_owned());
    }
    if request.path != "/api/events" {
        return Err("WebSocket path must be /api/events".to_owned());
    }
    if !request.upgrade_websocket || !request.connection_upgrade {
        return Err("missing WebSocket upgrade headers".to_owned());
    }
    if request.sec_websocket_version.as_deref() != Some("13") {
        return Err("unsupported Sec-WebSocket-Version".to_owned());
    }
    let key = request
        .sec_websocket_key
        .as_deref()
        .ok_or_else(|| "missing Sec-WebSocket-Key".to_owned())?;
    if !sec_websocket_key_valid(key) {
        return Err("invalid Sec-WebSocket-Key".to_owned());
    }
    if !web_origin_allowed(request.origin.as_deref(), request.host.as_deref()) {
        return Err("origin not allowed".to_owned());
    }
    Ok(())
}

/// RFC 6455: Sec-WebSocket-Key is a base64-encoded value that decodes to 16 bytes.
fn sec_websocket_key_valid(key: &str) -> bool {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let key = key.trim();
    if key.is_empty() {
        return false;
    }
    match BASE64.decode(key) {
        Ok(bytes) => bytes.len() == 16,
        Err(_) => false,
    }
}

/// Same-origin browser Origin on a loopback Host only.
fn web_origin_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let Some(host) = host.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let Some(origin_authority) = http_origin_authority(origin) else {
        return false;
    };
    if !authority_is_loopback(&origin_authority) || !authority_is_loopback(host) {
        return false;
    }
    authority_eq(&origin_authority, host)
}

fn http_origin_authority(origin: &str) -> Option<String> {
    let rest = origin.strip_prefix("http://")?;
    if rest.is_empty() || rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return None;
    }
    Some(rest.to_owned())
}

fn authority_is_loopback(authority: &str) -> bool {
    let host = authority_host(authority);
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn authority_host(authority: &str) -> &str {
    if authority.starts_with('[') {
        if let Some(end) = authority.find(']') {
            return &authority[..=end];
        }
        return authority;
    }
    if let Some((host, port)) = authority.rsplit_once(':')
        && !host.is_empty()
        && port.chars().all(|ch| ch.is_ascii_digit())
    {
        return host;
    }
    authority
}

fn authority_eq(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn close_path_segment<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let segment = path.strip_prefix(prefix)?.strip_suffix("/close")?;
    if segment.contains('/') || segment.contains('?') {
        return None;
    }
    Some(segment)
}

fn percent_decode_path_segment(value: &str) -> std::result::Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("owner contains an incomplete percent escape".to_owned());
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| "owner contains an invalid percent escape".to_owned())?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| "owner contains an invalid percent escape".to_owned())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "owner is not valid UTF-8".to_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone)]
struct ParsedHttpRequest {
    method: String,
    path: String,
    bridge_header: bool,
    host: Option<String>,
    origin: Option<String>,
    upgrade_websocket: bool,
    connection_upgrade: bool,
    sec_websocket_key: Option<String>,
    sec_websocket_version: Option<String>,
}

fn read_http_request(stream: &mut TcpStream) -> std::result::Result<ParsedHttpRequest, String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("missing HTTP method")?.to_owned();
    let path = parts.next().ok_or("missing HTTP path")?.to_owned();
    let mut bridge_header = false;
    let mut host = None;
    let mut origin = None;
    let mut upgrade_websocket = false;
    let mut connection_upgrade = false;
    let mut sec_websocket_key = None;
    let mut sec_websocket_version = None;
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        let header = line.trim_end_matches(['\r', '\n']);
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("X-Grok-Bridge-WebUI") && value == "1" {
            bridge_header = true;
        } else if name.eq_ignore_ascii_case("Host") {
            host = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("Origin") {
            origin = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("Upgrade") && value.eq_ignore_ascii_case("websocket") {
            upgrade_websocket = true;
        } else if name.eq_ignore_ascii_case("Connection") {
            connection_upgrade = value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("upgrade"));
        } else if name.eq_ignore_ascii_case("Sec-WebSocket-Key") {
            sec_websocket_key = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("Sec-WebSocket-Version") {
            sec_websocket_version = Some(value.to_owned());
        }
    }
    Ok(ParsedHttpRequest {
        method,
        path,
        bridge_header,
        host,
        origin,
        upgrade_websocket,
        connection_upgrade,
        sec_websocket_key,
        sec_websocket_version,
    })
}

fn write_http(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write_http_bytes(stream, status, content_type, body.as_bytes())
}

fn write_http_bytes(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

struct StaticWebAsset {
    content_type: &'static str,
    body: &'static [u8],
}

const WEB_UI_HTML: &[u8] = include_bytes!("../webui/dist/index.html");
const WEB_UI_JS: &[u8] = include_bytes!("../webui/dist/assets/app.js");
const WEB_UI_CSS: &[u8] = include_bytes!("../webui/dist/assets/app.css");

fn static_web_asset(path: &str) -> Option<StaticWebAsset> {
    match path {
        "/" => Some(StaticWebAsset {
            content_type: "text/html; charset=utf-8",
            body: WEB_UI_HTML,
        }),
        "/assets/app.js" => Some(StaticWebAsset {
            content_type: "text/javascript; charset=utf-8",
            body: WEB_UI_JS,
        }),
        "/assets/app.css" => Some(StaticWebAsset {
            content_type: "text/css; charset=utf-8",
            body: WEB_UI_CSS,
        }),
        _ => None,
    }
}

fn wake_listener() {
    if let Ok(name) = runtime_name() {
        let _ = Stream::connect(name);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    #[test]
    fn decodes_utf8_owner_path_segments_without_form_url_rules() {
        assert_eq!(
            percent_decode_path_segment("Codex-%E5%AF%B9%E8%AF%9D%2F100%25+ready").unwrap(),
            "Codex-对话/100%+ready"
        );
        assert_eq!(percent_decode_path_segment("A%2fb").unwrap(), "A/b");
    }

    #[test]
    fn rejects_malformed_owner_path_segments() {
        for value in ["owner%", "owner%2", "owner%GG", "%FF"] {
            assert!(percent_decode_path_segment(value).is_err(), "{value}");
        }
    }

    #[test]
    fn extracts_close_routes_without_overlapping_prefix_and_suffix() {
        assert_eq!(
            close_path_segment("/api/owners/Codex%20A/close", "/api/owners/"),
            Some("Codex%20A")
        );
        assert_eq!(
            close_path_segment("/api/owners//close", "/api/owners/"),
            Some("")
        );
        assert_eq!(
            close_path_segment("/api/owners/close", "/api/owners/"),
            None
        );
        assert_eq!(
            close_path_segment("/api/owners/a/b/close", "/api/owners/"),
            None
        );
        assert_eq!(
            close_path_segment("/api/sessions/close", "/api/sessions/"),
            None
        );
        assert_eq!(
            close_path_segment("/api/sessions/session-1/close", "/api/sessions/"),
            Some("session-1")
        );
    }

    #[test]
    fn serves_only_bundled_webui_distribution_assets() {
        for (path, content_type, expected_body) in [
            ("/", "text/html; charset=utf-8", WEB_UI_HTML),
            (
                "/assets/app.js",
                "text/javascript; charset=utf-8",
                WEB_UI_JS,
            ),
            ("/assets/app.css", "text/css; charset=utf-8", WEB_UI_CSS),
        ] {
            let asset = static_web_asset(path).expect("static route must exist");
            assert_eq!(asset.content_type, content_type);
            assert_eq!(asset.body, expected_body);
            assert!(!asset.body.is_empty());

            let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
            let response = serve_web_request(request.as_bytes());
            let (headers, body) = split_http_response(&response);
            assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(headers.contains(&format!("Content-Type: {content_type}")));
            assert_eq!(body, expected_body);
        }

        let html = std::str::from_utf8(WEB_UI_HTML).expect("index.html must be UTF-8");
        assert!(html.contains("/assets/app.js"));
        assert!(html.contains("/assets/app.css"));
        assert!(static_web_asset("/api/sessions").is_none());
        assert!(static_web_asset("/assets/missing.js").is_none());
    }

    #[test]
    fn sessions_api_remains_json_instead_of_static_content() {
        let response = serve_web_request(b"GET /api/sessions HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let (headers, body) = split_http_response(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(headers.contains("Content-Type: application/json"));
        assert_eq!(body, b"[]");
    }

    #[test]
    fn version_api_reports_current_package_version() {
        let response = serve_web_request(b"GET /api/version HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let (headers, body) = split_http_response(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(headers.contains("Content-Type: application/json"));
        let value: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(value["current"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["update_available"], false);
        assert!(
            value["release_url"]
                .as_str()
                .unwrap()
                .contains("github.com/luodaoyi/grok-bridge-rs/releases")
        );
    }

    #[test]
    fn byte_http_writer_uses_raw_body_length() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let body = [0, 0xff, b'\n', 0x80];

        write_http_bytes(&mut server, "200 OK", "application/octet-stream", &body).unwrap();
        drop(server);

        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let separator = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let headers = std::str::from_utf8(&response[..separator]).unwrap();
        assert!(headers.contains("Content-Length: 4"));
        assert_eq!(&response[separator + 4..], body);
    }

    #[test]
    fn close_api_still_requires_webui_header() {
        let response = serve_web_request(
            b"POST /api/sessions/missing/close HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(response.starts_with(b"HTTP/1.1 403 Forbidden\r\n"));
        assert!(response.ends_with(b"missing WebUI request header"));
    }

    #[test]
    fn web_origin_requires_matching_loopback_host() {
        assert!(web_origin_allowed(
            Some("http://127.0.0.1:47653"),
            Some("127.0.0.1:47653")
        ));
        assert!(web_origin_allowed(
            Some("http://localhost:47653"),
            Some("localhost:47653")
        ));
        assert!(!web_origin_allowed(
            Some("http://evil.example:47653"),
            Some("127.0.0.1:47653")
        ));
        assert!(!web_origin_allowed(
            Some("http://127.0.0.1:47653"),
            Some("127.0.0.1:9")
        ));
        assert!(!web_origin_allowed(None, Some("127.0.0.1:47653")));
        assert!(!web_origin_allowed(
            Some("https://127.0.0.1:47653"),
            Some("127.0.0.1:47653")
        ));
    }

    #[test]
    fn events_websocket_handshake_rejects_bad_origin_and_path() {
        let missing_upgrade = ParsedHttpRequest {
            method: "GET".to_owned(),
            path: "/api/events".to_owned(),
            bridge_header: false,
            host: Some("127.0.0.1:47653".to_owned()),
            origin: Some("http://127.0.0.1:47653".to_owned()),
            upgrade_websocket: false,
            connection_upgrade: false,
            sec_websocket_key: Some("dGhlIHNhbXBsZSBub25jZQ==".to_owned()),
            sec_websocket_version: Some("13".to_owned()),
        };
        assert!(validate_events_websocket_request(&missing_upgrade).is_err());

        let bad_origin = ParsedHttpRequest {
            upgrade_websocket: true,
            connection_upgrade: true,
            origin: Some("http://evil.example".to_owned()),
            ..missing_upgrade.clone()
        };
        assert_eq!(
            validate_events_websocket_request(&bad_origin).unwrap_err(),
            "origin not allowed"
        );

        let bad_path = ParsedHttpRequest {
            path: "/api/sessions".to_owned(),
            upgrade_websocket: true,
            connection_upgrade: true,
            origin: Some("http://127.0.0.1:47653".to_owned()),
            ..missing_upgrade
        };
        assert!(validate_events_websocket_request(&bad_path).is_err());
    }

    #[test]
    fn events_websocket_handshake_accepts_same_origin_loopback() {
        let request = ParsedHttpRequest {
            method: "GET".to_owned(),
            path: "/api/events".to_owned(),
            bridge_header: false,
            host: Some("127.0.0.1:47653".to_owned()),
            origin: Some("http://127.0.0.1:47653".to_owned()),
            upgrade_websocket: true,
            connection_upgrade: true,
            sec_websocket_key: Some("dGhlIHNhbXBsZSBub25jZQ==".to_owned()),
            sec_websocket_version: Some("13".to_owned()),
        };
        assert!(validate_events_websocket_request(&request).is_ok());
    }

    #[test]
    fn sec_websocket_key_must_be_rfc6455_16_byte_base64() {
        // RFC 6455 example key decodes to 16 bytes.
        assert!(sec_websocket_key_valid("dGhlIHNhbXBsZSBub25jZQ=="));
        assert!(!sec_websocket_key_valid(""));
        assert!(!sec_websocket_key_valid("   "));
        assert!(!sec_websocket_key_valid("not-base64!!!"));
        // Valid base64 but wrong decoded length (3 bytes).
        assert!(!sec_websocket_key_valid("YWJj"));
        // Valid base64 of 15 bytes.
        assert!(!sec_websocket_key_valid("AAAAAAAAAAAAAAAAAAAA"));
        // Valid base64 of 17 bytes.
        assert!(!sec_websocket_key_valid("AQIDBAUGBwgJCgsMDQ4PEBE="));

        let mut request = ParsedHttpRequest {
            method: "GET".to_owned(),
            path: "/api/events".to_owned(),
            bridge_header: false,
            host: Some("127.0.0.1:47653".to_owned()),
            origin: Some("http://127.0.0.1:47653".to_owned()),
            upgrade_websocket: true,
            connection_upgrade: true,
            sec_websocket_key: Some("YWJj".to_owned()),
            sec_websocket_version: Some("13".to_owned()),
        };
        assert_eq!(
            validate_events_websocket_request(&request).unwrap_err(),
            "invalid Sec-WebSocket-Key"
        );
        request.sec_websocket_key = Some("dGhlIHNhbXBsZSBub25jZQ==".to_owned());
        assert!(validate_events_websocket_request(&request).is_ok());
    }

    #[test]
    fn events_api_without_upgrade_stays_http_error() {
        let response = serve_web_request(
            b"GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:47653\r\nOrigin: http://127.0.0.1:47653\r\n\r\n",
        );
        assert!(response.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
    }

    #[test]
    fn parse_terminal_input_and_resize_commands() {
        let input = parse_web_events_client_command(
            r#"{"type":"terminal_input","id":"r1","session":"gbt-1","data_base64":"YQ=="}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            input,
            WebEventsClientCommand::TerminalInput {
                id: Some("r1".to_owned()),
                session: "gbt-1".to_owned(),
                data_base64: "YQ==".to_owned(),
            }
        );

        let resize = parse_web_events_client_command(
            r#"{"type":"terminal_resize","id":"r2","session":"gbt-1","cols":120,"rows":40}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            resize,
            WebEventsClientCommand::TerminalResize {
                id: Some("r2".to_owned()),
                session: "gbt-1".to_owned(),
                cols: 120,
                rows: 40,
            }
        );

        // Unknown types are ignored (push-only / future).
        assert_eq!(
            parse_web_events_client_command(r#"{"type":"sessions","sessions":[]}"#).unwrap(),
            None
        );
    }

    #[test]
    fn parse_rejects_malformed_and_oversized_input_without_command() {
        assert!(parse_web_events_client_command("not-json").is_err());
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","session":"","data_base64":"YQ=="}"#
            )
            .is_err()
        );
        assert!(
            parse_web_events_client_command(r#"{"type":"terminal_input","session":"gbt-1"}"#)
                .is_err()
        );
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_resize","session":"gbt-1","cols":1,"rows":40}"#
            )
            .is_ok()
        );
        // cols=1 is parsed; apply-time validate_terminal_size rejects it.
        let cmd = parse_web_events_client_command(
            r#"{"type":"terminal_resize","session":"gbt-1","cols":1,"rows":40}"#,
        )
        .unwrap()
        .unwrap();
        let host = SessionHost::new(OrphanPolicy {
            lease_ms: 120_000,
            grace_ms: 600_000,
        });
        assert!(apply_web_events_client_command(&host, &cmd).is_err());
    }

    #[test]
    fn apply_terminal_input_uses_decode_write_data_and_rejects_unknown_session() {
        let host = SessionHost::new(OrphanPolicy {
            lease_ms: 120_000,
            grace_ms: 600_000,
        });
        // Exact raw bytes for "A" (0x41) via standard base64.
        let cmd = WebEventsClientCommand::TerminalInput {
            id: Some("n1".to_owned()),
            session: "missing-session".to_owned(),
            data_base64: "QQ==".to_owned(),
        };
        let data = decode_write_data("QQ==").unwrap();
        assert_eq!(data, vec![0x41]);
        // Unknown session: error, never panics, never writes.
        let err = apply_web_events_client_command(&host, &cmd).unwrap_err();
        assert!(!err.is_empty());

        // Empty / oversized rejected by decode_write_data before host write.
        let empty = WebEventsClientCommand::TerminalInput {
            id: None,
            session: "missing-session".to_owned(),
            data_base64: "".to_owned(),
        };
        // Empty base64 fails at parse (required).
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","session":"s","data_base64":""}"#
            )
            .is_err()
        );
        let _ = empty;

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let oversize = BASE64.encode(vec![0x5a; crate::protocol::MAX_WRITE_BYTES + 1]);
        let over = WebEventsClientCommand::TerminalInput {
            id: None,
            session: "missing-session".to_owned(),
            data_base64: oversize,
        };
        assert!(apply_web_events_client_command(&host, &over).is_err());
    }

    #[test]
    fn command_result_json_is_not_sessions_type() {
        let payload = build_web_events_command_result(
            "input_result",
            Some("r1"),
            Some("gbt-1"),
            false,
            Some("nope"),
        );
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["type"], "input_result");
        assert_eq!(value["ok"], false);
        assert_eq!(value["id"], "r1");
        assert_eq!(value["session"], "gbt-1");
        assert_eq!(value["error"], "nope");
        assert!(value.get("sessions").is_none());
        assert!(value.get("terminals").is_none());
    }

    #[test]
    fn client_poll_interval_is_bounded_for_interactive_latency() {
        // Guard against regressions that reintroduce multi-second waits before
        // reading client frames.
        assert!(WEB_EVENTS_CLIENT_POLL <= Duration::from_millis(100));
        assert!(WEB_EVENTS_CLIENT_POLL >= Duration::from_millis(1));
        assert!(WEB_EVENTS_LEASE_REFRESH < Duration::from_secs(30));
        assert!(WEB_EVENTS_LEASE_REFRESH >= Duration::from_secs(1));
    }

    #[test]
    fn parse_rejects_invalid_session_handles_before_host_touch() {
        let long = "x".repeat(200);
        let bad_cases = [
            "",
            "has space",
            "bad/id",
            long.as_str(),
            "ctrl\n",
            "unicode-\u{4e2d}",
        ];
        for bad in bad_cases {
            let payload = format!(
                r#"{{"type":"terminal_input","session":{session},"data_base64":"YQ=="}}"#,
                session = serde_json::to_string(bad).unwrap()
            );
            let err = parse_web_events_client_command(&payload).unwrap_err();
            assert!(
                err.contains("session handle") || err.contains("session"),
                "bad={bad:?} err={err}"
            );
            let resize = format!(
                r#"{{"type":"terminal_resize","session":{session},"cols":80,"rows":24}}"#,
                session = serde_json::to_string(bad).unwrap()
            );
            assert!(parse_web_events_client_command(&resize).is_err());
        }
        // Valid handle shape is accepted by the parser (host still enforces existence).
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","session":"gbt-1","data_base64":"YQ=="}"#
            )
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn client_close_api_reports_an_exact_empty_group() {
        let response = serve_web_request(
            b"POST /api/clients/codex-thread-42/close HTTP/1.1\r\nHost: localhost\r\nX-Grok-Bridge-WebUI: 1\r\n\r\n",
        );
        let (headers, body) = split_http_response(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(body).unwrap(),
            serde_json::json!({ "matched": 0, "closed": 0, "failures": [] })
        );
    }

    fn serve_web_request(request: &[u8]) -> Vec<u8> {
        let timeout = Duration::from_secs(10);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        client.set_read_timeout(Some(timeout)).unwrap();
        client.set_write_timeout(Some(timeout)).unwrap();
        server.set_read_timeout(Some(timeout)).unwrap();
        server.set_write_timeout(Some(timeout)).unwrap();
        client.write_all(request).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let handler = std::thread::spawn(move || {
            handle_web_connection(
                server,
                Arc::new(RuntimeState {
                    host: SessionHost::new(OrphanPolicy {
                        lease_ms: 120_000,
                        grace_ms: 600_000,
                    }),
                    started_at_ms: 0,
                    stopping: AtomicBool::new(false),
                    web_url: None,
                    version_checker: Arc::new(VersionChecker::new()),
                }),
            );
        });

        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        handler.join().unwrap();
        response
    }

    fn split_http_response(response: &[u8]) -> (&str, &[u8]) {
        let separator = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        (
            std::str::from_utf8(&response[..separator + 2]).unwrap(),
            &response[separator + 4..],
        )
    }
}
