use std::{
    env,
    io::{BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};

use crate::{
    protocol::{
        Request, ResponseEnvelope, ResponseResult, ServerInfo, decode_request, decode_write_data,
        validate_client_session_id, validate_owner,
    },
    session::{OrphanPolicy, SessionHost},
    transport::{call_anonymous, read_frame, runtime_name, write_response},
    version_check::{CHECK_INTERVAL, VersionChecker},
};

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
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
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
    let (method, path, bridge_header) = request;
    if method == "GET"
        && let Some(asset) = static_web_asset(&path)
    {
        let _ = write_http_bytes(&mut stream, "200 OK", asset.content_type, asset.body);
        return;
    }
    match (method.as_str(), path.as_str()) {
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

fn read_http_request(
    stream: &mut TcpStream,
) -> std::result::Result<(String, String, bool), String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("missing HTTP method")?.to_owned();
    let path = parts.next().ok_or("missing HTTP path")?.to_owned();
    let mut bridge_header = false;
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if line.trim().eq_ignore_ascii_case("X-Grok-Bridge-WebUI: 1") {
            bridge_header = true;
        }
    }
    Ok((method, path, bridge_header))
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
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        client.write_all(request).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

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

        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
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
