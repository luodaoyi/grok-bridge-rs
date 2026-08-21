use std::{
    collections::{HashMap, HashSet},
    env, fmt,
    io::{self, BufRead, BufReader, ErrorKind, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
#[cfg(not(unix))]
use interprocess::local_socket::ListenerOptions;
#[cfg(unix)]
use interprocess::local_socket::Name;
use interprocess::local_socket::{Listener, Stream, prelude::*};
use tungstenite::{
    Message, WebSocket,
    handshake::derive_accept_key,
    protocol::{Role, WebSocketConfig},
};

#[cfg(windows)]
use crate::transport::RemapNowaitEmptyRead;
#[cfg(not(unix))]
use crate::transport::call_anonymous;
use crate::{
    protocol::{
        Request, ResponseEnvelope, ResponseResult, ServerInfo, decode_request, decode_write_data,
        validate_client_session_id, validate_owner, validate_session_handle,
        validate_terminal_size,
    },
    session::{CloseError, OrphanPolicy, SessionHost},
    transport::{
        IPC_FRAME_READ_DEADLINE, IPC_REJECT_DEADLINE, read_frame, runtime_name, write_response,
        write_response_with_deadline,
    },
    version_check::{CHECK_INTERVAL, VersionChecker},
};

/// Bound WebSocket text/binary payload size (server and client).
const WEB_EVENTS_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// Max idle sleep between client-frame polls so interactive keys stay low-latency.
/// Host Condvar still wakes immediately on session revisions.
const WEB_EVENTS_CLIENT_POLL: Duration = Duration::from_millis(25);
/// Cap inbound request-id length for WebUI command frames.
const WEB_EVENTS_MAX_REQUEST_ID_BYTES: usize = 128;
/// Hard per-connection bound on tracked request ids. This keeps request-id
/// memory within a small per-connection budget; when full, the next new command
/// is rejected with an explicit reconnect result and the connection is closed.
/// Old ids are never evicted or replayable within one connection.
const WEB_EVENTS_MAX_TRACKED_REQUEST_IDS: usize = 8192;

/// Active connection caps: the Runtime never spawns an unbounded number of
/// handler threads for either local listener. Reaching a cap rejects new
/// connections with a bounded error instead of creating another thread.
const WEB_MAX_ACTIVE_CONNECTIONS: usize = 64;
const IPC_MAX_ACTIVE_CONNECTIONS: usize = 64;

/// HTTP parser and I/O bounds for the loopback WebUI.
/// Request line and per-header line byte caps.
const WEB_HTTP_MAX_REQUEST_LINE_BYTES: usize = 8 * 1024;
const WEB_HTTP_MAX_HEADER_LINE_BYTES: usize = 8 * 1024;
/// Maximum number of header lines per request.
const WEB_HTTP_MAX_HEADER_COUNT: usize = 64;
/// Maximum total header bytes (names, values, CRLFs) per request.
const WEB_HTTP_MAX_HEADER_BYTES: usize = 32 * 1024;
/// Maximum response body the WebUI will serialize onto the wire. Bodies above
/// this are replaced with a bounded 500 error instead of being sent verbatim.
const WEB_HTTP_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// Per-read socket timeout: a stalled peer fails one read quickly.
const WEB_HTTP_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Total wall-clock budget for reading one HTTP request; a trickling client
/// cannot hold a handler thread forever one byte at a time.
const WEB_HTTP_REQUEST_DEADLINE: Duration = Duration::from_secs(30);
/// Socket write timeout for WebUI HTTP and WebSocket writes.
const WEB_HTTP_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// Short total deadline for a WebUI rejection response written directly from
/// the accept thread: a peer that never drains must not pin acceptance of
/// further connections for the full write timeout.
const WEB_HTTP_REJECT_DEADLINE: Duration = Duration::from_secs(2);
/// Bounded backoff while an HTTP response write is blocked on a full socket
/// buffer, so a stalled peer is retried with sleeps instead of a busy loop.
const WEB_HTTP_WRITE_POLL_MIN: Duration = Duration::from_millis(1);
const WEB_HTTP_WRITE_POLL_MAX: Duration = Duration::from_millis(50);
/// Idle poll interval while waiting for HTTP request bytes.
const WEB_HTTP_POLL: Duration = Duration::from_millis(5);

pub(crate) fn run() -> Result<()> {
    // On Unix one owner-only lock serializes bind, liveness probe, stale
    // cleanup, and rebind. It is released immediately after listener ownership
    // is established, and by the OS if a starter crashes.
    #[cfg(unix)]
    let startup_lock = crate::transport::acquire_runtime_startup_lock()?;
    // Upgrade compatibility: a Runtime started by an older binary still owns
    // the legacy GenericNamespaced endpoint. Binding the new filesystem socket
    // would succeed (different name) and create a second Runtime, stranding old
    // sessions and contending for the WebUI port. Keep the singleton: exit
    // quietly so clients keep using the legacy Runtime until it stops.
    #[cfg(unix)]
    match legacy_runtime_server_probe() {
        LegacyProbe::Active => {
            eprintln!(
                "grok-bridge server: an older Runtime is still running on the legacy endpoint \
                 `{}`; exiting so clients keep using it. Stop that Runtime (close its terminal \
                 or press Ctrl-C, or kill the grok-bridge process) and start this version again.",
                crate::transport::runtime_identity().to_string_lossy()
            );
            return Ok(());
        }
        LegacyProbe::Uncertain(error) => {
            eprintln!(
                "grok-bridge server: cannot determine whether an older Runtime still owns the \
                 legacy endpoint `{}` (probe failed: {error}); exiting rather than risk a second \
                 Runtime. Stop the older Runtime, or if none is running remove or repair the \
                 stale legacy endpoint, then start this version again.",
                crate::transport::runtime_identity().to_string_lossy()
            );
            return Ok(());
        }
        LegacyProbe::Absent => {}
    }
    let listener = match bind_runtime_listener() {
        Ok(listener) => listener,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::AddrInUse | ErrorKind::PermissionDenied
            ) =>
        {
            if runtime_server_is_alive() {
                // Another Runtime already owns the IPC name; the singleton is
                // running, so this duplicate server exits quietly.
                return Ok(());
            }
            rebind_after_stale_socket(
                error,
                #[cfg(unix)]
                &startup_lock,
            )?
        }
        Err(error) => {
            #[cfg(not(unix))]
            return Err(error).context("failed to bind the runtime named pipe");
            #[cfg(unix)]
            return Err(error).context("failed to bind the runtime IPC listener");
        }
    };
    #[cfg(unix)]
    drop(startup_lock);

    let web_listener = bind_web_ui()?;
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
        web_connections: AtomicUsize::new(0),
        ipc_connections: AtomicUsize::new(0),
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
        if !admit_connection(&state.ipc_connections, IPC_MAX_ACTIVE_CONNECTIONS) {
            // At the IPC connection cap: answer with a bounded error frame
            // under a short deadline and close without spawning another
            // handler thread. The short deadline keeps a peer that never
            // drains from pinning the accept loop.
            let nonblocking = connection.set_nonblocking(true).is_ok();
            let busy = ResponseEnvelope::failure(
                "invalid-request",
                "server_busy",
                "the runtime is at its IPC connection limit; retry shortly",
            );
            let mut connection = connection;
            if nonblocking {
                let _ = write_response_with_deadline(&mut connection, &busy, IPC_REJECT_DEADLINE);
            }
            continue;
        }
        let state = Arc::clone(&state);
        thread::spawn(move || {
            // Keep a dedicated Arc for the slot so the counter stays alive for
            // the whole handler while `state` moves into the handler.
            let slot_state = Arc::clone(&state);
            let _slot = ConnectionSlot {
                slots: &slot_state.ipc_connections,
            };
            handle_connection(connection, state);
        });
    }

    state.host.shutdown_all()?;
    Ok(())
}

/// Bind the Runtime IPC listener. On Unix this secures a filesystem socket to
/// owner-only (0600) inside the private owner-only runtime directory; on other
/// platforms it keeps the historical named-pipe bind. Returns the raw I/O
/// error so `run` can distinguish an occupied name from other bind failures.
fn bind_runtime_listener() -> io::Result<Listener> {
    #[cfg(unix)]
    {
        let path = crate::transport::runtime_socket_path().map_err(io::Error::other)?;
        crate::transport::bind_listener_at(&path)
    }
    #[cfg(not(unix))]
    {
        let name = runtime_name().map_err(io::Error::other)?;
        ListenerOptions::new().name(name).create_sync()
    }
}

/// Whether another Runtime owns the IPC name. On Unix this probes the socket
/// directly: a live listener accepts connections even while at its connection
/// cap, whereas a stale socket file refuses them. Other platforms keep the
/// historical `ServerStatus` round-trip.
fn runtime_server_is_alive() -> bool {
    #[cfg(unix)]
    {
        runtime_name()
            .map(|name| runtime_server_is_alive_for(&name))
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        call_anonymous(Request::ServerStatus, false).is_ok_and(|response| {
            response.ok && matches!(response.result, Some(ResponseResult::ServerInfo(_)))
        })
    }
}

/// Whether a Runtime started by an older binary still answers on the legacy
/// GenericNamespaced endpoint (Unix only; other platforms share one endpoint
/// between old and new binaries, so the normal liveness probe covers them).
/// The legacy name never collides with the new filesystem socket, so without
/// this probe a duplicate server could bind the new socket and create a second
/// Runtime. Same connect-based semantics as [`runtime_server_is_alive_for`]:
/// a live listener is active, a missing or refused name is absent, and any
/// other connect error is surfaced as `Uncertain` so startup can report why it
/// refused to start rather than mislabeling a live Runtime as a stale socket.
#[cfg(unix)]
fn legacy_runtime_server_probe() -> LegacyProbe {
    match crate::transport::legacy_runtime_name() {
        Ok(name) => match probe_runtime_name(&name) {
            ProbeOutcome::Live => LegacyProbe::Active,
            ProbeOutcome::Dead => LegacyProbe::Absent,
            ProbeOutcome::Uncertain(error) => LegacyProbe::Uncertain(error),
        },
        // An unconstructible legacy name cannot be probed; treat it as absent
        // so startup proceeds to bind the filesystem socket.
        Err(_) => LegacyProbe::Absent,
    }
}

/// Outcome of probing the legacy endpoint. Mirrors [`runtime_server_is_alive_for`]
/// except that an inconclusive probe error is retained instead of being
/// collapsed into a boolean.
#[cfg(unix)]
enum LegacyProbe {
    /// A live listener accepted the connection: an older Runtime is running.
    Active,
    /// Definitively no older Runtime (missing or refused endpoint).
    Absent,
    /// The probe could not conclude; `error` is the original connect error.
    Uncertain(io::Error),
}

/// Unix liveness probe for a single name: a live listener accepts connections,
/// while a stale socket file refuses them. ENOENT (no socket file) and
/// ECONNREFUSED (a socket file with nothing listening behind it) are the only
/// definitive "dead" signals. Any other connect error — EACCES, EMFILE,
/// transient failures — is reported as `Uncertain` with the original error
/// instead of being collapsed into a boolean. Never removes anything.
#[cfg(unix)]
fn probe_runtime_name(name: &Name<'_>) -> ProbeOutcome {
    match Stream::connect(name.clone()) {
        Ok(_) => ProbeOutcome::Live,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ) =>
        {
            ProbeOutcome::Dead
        }
        Err(error) => ProbeOutcome::Uncertain(error),
    }
}

/// Outcome of a single-name Unix liveness probe.
#[cfg(unix)]
#[derive(Debug)]
enum ProbeOutcome {
    Live,
    Dead,
    Uncertain(io::Error),
}

/// Conservative bool for the current-endpoint probe: any outcome other than a
/// definitive "dead" must count as alive, or a live Runtime's socket could be
/// removed. This is the semantic [`rebind_after_stale_socket`] relies on.
#[cfg(unix)]
fn runtime_server_is_alive_for(name: &Name<'_>) -> bool {
    !matches!(probe_runtime_name(name), ProbeOutcome::Dead)
}

/// The IPC name is held by a process that did not answer the liveness probe.
/// On Unix this is typically a stale socket file left by a crashed Runtime:
/// remove it only when it is provably our own socket file, then retry the bind
/// once. Unsafe paths — symlinks, wrong file types, foreign owners — are
/// refused with a diagnosable error instead of being deleted. Other platforms
/// keep the historical "occupied" error.
fn rebind_after_stale_socket(
    error: io::Error,
    #[cfg(unix)] startup_lock: &crate::transport::RuntimeStartupLock,
) -> Result<Listener> {
    #[cfg(unix)]
    {
        // Re-probe under the startup lock before removing anything: a live
        // Runtime may have claimed the name between the first probe and now,
        // and deleting its socket would break the running server.
        if runtime_server_is_alive() {
            return Err(error).context("runtime socket path is occupied by another process");
        }
        if crate::transport::remove_stale_runtime_socket(startup_lock)? {
            match bind_runtime_listener() {
                Ok(listener) => Ok(listener),
                Err(retry_error) => Err(retry_error)
                    .context("runtime socket was re-occupied after removing the stale socket"),
            }
        } else {
            Err(error).context("runtime socket path is occupied by another process")
        }
    }
    #[cfg(not(unix))]
    {
        Err(error).context("runtime pipe name is occupied by another process")
    }
}

struct RuntimeState {
    host: SessionHost,
    started_at_ms: u64,
    stopping: AtomicBool,
    web_url: Option<String>,
    version_checker: Arc<VersionChecker>,
    /// Active WebUI HTTP/WebSocket handler threads.
    web_connections: AtomicUsize,
    /// Active IPC request handler threads.
    ipc_connections: AtomicUsize,
}

/// Atomically reserve one of `limit` handler slots; `false` means the cap is
/// reached and the caller must reject the connection without spawning a thread.
fn admit_connection(slots: &AtomicUsize, limit: usize) -> bool {
    slots
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < limit).then_some(current + 1)
        })
        .is_ok()
}

/// Releases a reserved connection slot when the handler thread finishes.
struct ConnectionSlot<'a> {
    slots: &'a AtomicUsize,
}

impl Drop for ConnectionSlot<'_> {
    fn drop(&mut self) {
        self.slots.fetch_sub(1, Ordering::AcqRel);
    }
}

fn handle_connection(stream: Stream, state: Arc<RuntimeState>) {
    // Non-blocking I/O lets the request read and the response write be bounded
    // by wall-clock deadlines on every platform (Windows named pipes have no
    // native socket timeouts).
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    #[cfg(windows)]
    let mut connection = BufReader::new(RemapNowaitEmptyRead(stream));
    #[cfg(not(windows))]
    let mut connection = BufReader::new(stream);
    let frame = match read_frame(
        &mut connection,
        Some(Instant::now() + IPC_FRAME_READ_DEADLINE),
    ) {
        Ok(frame) => frame,
        Err(error) => {
            // The peer did not deliver a complete request (silent, closed, or
            // oversized). Record the bounded error and close; there is no
            // request to answer and writing would only wait another write
            // deadline on a peer that is not reading.
            eprintln!("grok-bridge server: IPC request read failed: {error:#}");
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
    let stop_requested = matches!(envelope.request, Request::ServerStop);
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
                // A ServerStop whose shutdown still failed must wake the accept
                // loop anyway so the process can exit; otherwise the loop would
                // wait for a client that never comes while the server holds the
                // session registry and its process ownership.
                stop_requested,
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

/// Default WebUI listener. Kept loopback-only because the WebUI has no user
/// authentication; `GROK_BRIDGE_WEB_ADDR` may only select a loopback address.
const DEFAULT_WEB_ADDR: &str = "127.0.0.1:47653";

/// Why a `GROK_BRIDGE_WEB_ADDR` value cannot be used as the WebUI listener.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WebAddrError {
    /// Not parseable as a literal `IpAddr:port`. Hostnames are never resolved.
    InvalidAddress(String),
    /// Parsed, but the IP is not a loopback address.
    NotLoopback(SocketAddr),
}

impl fmt::Display for WebAddrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebAddrError::InvalidAddress(value) => write!(
                formatter,
                "GROK_BRIDGE_WEB_ADDR {value:?} is not a literal IP address with a port; \
                 the local-only WebUI never resolves hostnames, use a loopback address \
                 such as 127.0.0.1:47653 or [::1]:47653"
            ),
            WebAddrError::NotLoopback(address) => write!(
                formatter,
                "GROK_BRIDGE_WEB_ADDR {address} is not a loopback address; the local-only \
                 WebUI may only bind to IPv4 127.0.0.0/8 or IPv6 ::1"
            ),
        }
    }
}

impl std::error::Error for WebAddrError {}

/// Parse a `GROK_BRIDGE_WEB_ADDR` value into a bindable `SocketAddr` and
/// require it to be loopback (IPv4 127.0.0.0/8 or IPv6 ::1). Pure: never
/// resolves hostnames, never binds, never reads the environment.
fn parse_web_addr(value: &str) -> Result<SocketAddr, WebAddrError> {
    match value.parse::<SocketAddr>() {
        Ok(address) if address.ip().is_loopback() => Ok(address),
        Ok(address) => Err(WebAddrError::NotLoopback(address)),
        Err(_) => Err(WebAddrError::InvalidAddress(value.to_owned())),
    }
}

/// Shared by `bind_web_ui` and tests; `value` is the raw `GROK_BRIDGE_WEB_ADDR`.
fn bind_web_ui_from(value: &str) -> Result<Option<TcpListener>> {
    // Parse and validate before binding so a wildcard, LAN, public, hostname,
    // or unresolvable value is a startup error and never reaches `bind`.
    let socket_addr = parse_web_addr(value)?;
    classify_web_ui_bind(value, TcpListener::bind(socket_addr))
}

fn classify_web_ui_bind(
    value: &str,
    result: io::Result<TcpListener>,
) -> Result<Option<TcpListener>> {
    match result {
        Ok(listener) => Ok(Some(listener)),
        // A legal loopback address whose port is taken only disables the
        // WebUI; JSON CLI and PTY sessions keep running.
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            eprintln!("grok-bridge server: WebUI unavailable at {value}: {error}");
            Ok(None)
        }
        Err(error) => Err(error).with_context(|| format!("failed to bind WebUI at {value}")),
    }
}

fn bind_web_ui() -> Result<Option<TcpListener>> {
    let address = env::var("GROK_BRIDGE_WEB_ADDR").unwrap_or_else(|_| DEFAULT_WEB_ADDR.to_owned());
    bind_web_ui_from(&address)
}

fn run_web_ui(listener: TcpListener, state: Arc<RuntimeState>) {
    for connection in listener.incoming() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        match connection {
            Ok(mut stream) => {
                if !admit_connection(&state.web_connections, WEB_MAX_ACTIVE_CONNECTIONS) {
                    // At the Web connection cap: answer with a bounded 503
                    // under a short total deadline and close without spawning
                    // another handler thread, so a peer that never drains
                    // cannot pin the accept loop.
                    let _ = write_http_bytes_with_deadline(
                        &mut stream,
                        "503 Service Unavailable",
                        "text/plain; charset=utf-8",
                        b"too many active WebUI connections; retry shortly",
                        WEB_HTTP_REJECT_DEADLINE,
                    );
                    continue;
                }
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    // Keep a dedicated Arc for the slot so the counter stays alive for
                    // the whole handler while `state` moves into the handler.
                    let slot_state = Arc::clone(&state);
                    let _slot = ConnectionSlot {
                        slots: &slot_state.web_connections,
                    };
                    handle_web_connection(stream, state);
                });
            }
            Err(error) => eprintln!("grok-bridge server: WebUI accept failed: {error}"),
        }
    }
}

fn handle_web_connection(mut stream: TcpStream, state: Arc<RuntimeState>) {
    let _ = stream.set_read_timeout(Some(WEB_HTTP_READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WEB_HTTP_WRITE_TIMEOUT));
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
                    let body = serde_json::to_string(&result).unwrap_or_else(|_| {
                        r#"{"matched":0,"closed":0,"timed_out":[],"failures":[]}"#.to_owned()
                    });
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
                        "timed_out": result.timed_out,
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
                        close_error_http_status(&error),
                        "text/plain; charset=utf-8",
                        &error.to_string(),
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

fn close_error_http_status(error: &CloseError) -> &'static str {
    match error {
        CloseError::NotFound(_) => "404 Not Found",
        CloseError::Timeout => "504 Gateway Timeout",
        CloseError::Failed(_) => "500 Internal Server Error",
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
    let _ = stream.set_write_timeout(Some(WEB_HTTP_WRITE_TIMEOUT));

    let mut websocket = WebSocket::from_raw_socket(stream, Role::Server, Some(web_socket_config()));
    run_events_websocket(&mut websocket, &state);
}

/// Bounded WebSocket configuration: every frame, message, read and write
/// buffer has an explicit cap so a hostile or broken peer can never grow
/// memory without bound or pin the connection thread.
fn web_socket_config() -> WebSocketConfig {
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(WEB_EVENTS_MAX_MESSAGE_BYTES);
    config.max_frame_size = Some(WEB_EVENTS_MAX_MESSAGE_BYTES);
    // Eager writes so session events are not stuck in the 128 KiB default buffer.
    config.write_buffer_size = 0;
    // Explicit write-buffer cap: tungstenite's default is unbounded
    // (usize::MAX). With eager writes the buffer only grows while the peer is
    // not draining, so one max-size frame is the most backpressure we allow
    // before the connection fails with WriteBufferFull.
    config.max_write_buffer_size = WEB_EVENTS_MAX_MESSAGE_BYTES;
    config.read_buffer_size = 8 * 1024;
    config
}

fn run_events_websocket(websocket: &mut WebSocket<TcpStream>, state: &RuntimeState) {
    let mut cursors = HashMap::new();
    // Request ids must be unique within this connection; duplicates are
    // rejected before any PTY side effect.
    let mut seen_ids = HashSet::new();
    let mut seen_revision = state.host.revision();
    if !send_web_events(websocket, state, &mut cursors, true) {
        return;
    }

    while !state.stopping.load(Ordering::Acquire) {
        // Service inbound terminal_input / terminal_resize before sleeping so
        // interactive keystrokes never wait behind a multi-second idle timeout.
        match poll_websocket_client(websocket, state, &mut seen_ids) {
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
        // never push frames. The event stream itself never refreshes Codex
        // leases (only inbound terminal commands do), so real lease
        // transitions still arrive promptly for display.
        let wait = match lease_deadline {
            Some(deadline) if deadline > now => Duration::from_millis(deadline - now)
                .min(Duration::from_secs(30))
                .max(Duration::from_millis(1))
                .min(WEB_EVENTS_CLIENT_POLL),
            Some(_) => Duration::from_millis(1),
            None => WEB_EVENTS_CLIENT_POLL,
        };
        let current = state.host.wait_revision(seen_revision, wait);
        match poll_websocket_client(websocket, state, &mut seen_ids) {
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
    seen_ids: &mut HashSet<String>,
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
                if matches!(
                    handle_web_events_client_text(websocket, state, seen_ids, text.as_str()),
                    WsClientAction::Close
                ) {
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
        id: String,
        session: String,
        data_base64: String,
    },
    TerminalResize {
        id: String,
        session: String,
        cols: u16,
        rows: u16,
    },
}

/// A parse failure that may still carry enough structure — a legal request id,
/// a recognized command type, and a cleanly extractable session — to correlate
/// the error result with the browser's pending command. Only messages where
/// nothing usable can be identified (invalid JSON, non-object, oversized, or a
/// malformed id) stay id-less with the generic `input_result` type.
#[derive(Debug)]
struct WebEventsParseError {
    message: String,
    id: Option<String>,
    session: Option<String>,
    result_type: &'static str,
}

impl WebEventsParseError {
    fn generic(message: impl Into<String>) -> Self {
        WebEventsParseError {
            message: message.into(),
            id: None,
            session: None,
            result_type: "input_result",
        }
    }

    fn with_context(
        message: impl Into<String>,
        id: Option<String>,
        session: Option<String>,
        result_type: &'static str,
    ) -> Self {
        WebEventsParseError {
            message: message.into(),
            id,
            session,
            result_type,
        }
    }
}

/// Parse a single WebUI client command without panicking on junk.
/// Unknown types yield `Ok(None)` (ignored). Malformed known types yield `Err`
/// carrying the recognized request id and result type when available.
fn parse_web_events_client_command(
    text: &str,
) -> Result<Option<WebEventsClientCommand>, WebEventsParseError> {
    if text.len() > WEB_EVENTS_MAX_MESSAGE_BYTES {
        return Err(WebEventsParseError::generic("message exceeds size limit"));
    }
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| WebEventsParseError::generic(format!("invalid JSON: {error}")))?;
    let Some(object) = value.as_object() else {
        return Err(WebEventsParseError::generic(
            "message must be a JSON object",
        ));
    };
    let message_type = object
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    // Best-effort extraction before the field checks below, so a malformed
    // known-shape command can still be settled by its request id and type.
    let id = match object.get("id") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => {
            if value.len() > WEB_EVENTS_MAX_REQUEST_ID_BYTES {
                return Err(WebEventsParseError::generic("request id is too long"));
            }
            Some(value.clone())
        }
        Some(_) => return Err(WebEventsParseError::generic("request id must be a string")),
    };
    let session = object
        .get("session")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let result_type = match message_type {
        "terminal_input" => "input_result",
        "terminal_resize" => "resize_result",
        // Unknown / push-only types (e.g. future clients): ignore safely.
        _ => return Ok(None),
    };
    // Errors past this point keep the recognized id/session/type so the
    // browser can release its pending entry and see a correlated failure.
    // The closure owns its own clones so `id` below can still be moved into
    // the command variants.
    let context_error = {
        let id = id.clone();
        let session = session.clone();
        move |message: String| {
            WebEventsParseError::with_context(message, id.clone(), session.clone(), result_type)
        }
    };

    match message_type {
        "terminal_input" => {
            let Some(id) = id else {
                return Err(context_error("request id is required".to_owned()));
            };
            if id.is_empty() {
                return Err(context_error("request id is required".to_owned()));
            }
            let session = object
                .get("session")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            validate_session_handle(&session).map_err(|err| context_error(format!("{err:#}")))?;
            let data_base64 = object
                .get("data_base64")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            if data_base64.is_empty() {
                return Err(context_error("data_base64 is required".to_owned()));
            }
            Ok(Some(WebEventsClientCommand::TerminalInput {
                id,
                session,
                data_base64,
            }))
        }
        "terminal_resize" => {
            let Some(id) = id else {
                return Err(context_error("request id is required".to_owned()));
            };
            if id.is_empty() {
                return Err(context_error("request id is required".to_owned()));
            }
            let session = object
                .get("session")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            validate_session_handle(&session).map_err(|err| context_error(format!("{err:#}")))?;
            let cols = object
                .get("cols")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| context_error("cols is required".to_owned()))?;
            let rows = object
                .get("rows")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| context_error("rows is required".to_owned()))?;
            let cols =
                u16::try_from(cols).map_err(|_| context_error("cols out of range".to_owned()))?;
            let rows =
                u16::try_from(rows).map_err(|_| context_error("rows out of range".to_owned()))?;
            Ok(Some(WebEventsClientCommand::TerminalResize {
                id,
                session,
                cols,
                rows,
            }))
        }
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

fn web_events_command_id(command: &WebEventsClientCommand) -> &str {
    match command {
        WebEventsClientCommand::TerminalInput { id, .. }
        | WebEventsClientCommand::TerminalResize { id, .. } => id.as_str(),
    }
}

/// The single result a client command resolves to: applied, rejected by apply
/// validation, or rejected as a duplicate request id. Exactly one result is
/// always sent back so the caller can match it to its pending command.
struct WebEventsCommandOutcome {
    result_type: &'static str,
    id: Option<String>,
    session: Option<String>,
    ok: bool,
    error: Option<String>,
    reconnect: bool,
}

fn plan_web_events_command_outcome(
    host: &SessionHost,
    command: &WebEventsClientCommand,
    seen_ids: &mut HashSet<String>,
) -> WebEventsCommandOutcome {
    let result_type = web_events_result_type(command);
    let id = web_events_command_id(command).to_owned();
    let session = Some(web_events_command_session(command).to_owned());
    // Strict per-connection uniqueness with a hard memory bound. Eviction
    // would let a previously-applied non-idempotent input replay, so a full
    // window rejects one new command and explicitly rotates the connection.
    if seen_ids.contains(&id) {
        return WebEventsCommandOutcome {
            result_type,
            id: Some(id),
            session,
            ok: false,
            error: Some("duplicate request id".to_owned()),
            reconnect: false,
        };
    }
    if seen_ids.len() >= WEB_EVENTS_MAX_TRACKED_REQUEST_IDS {
        return WebEventsCommandOutcome {
            result_type,
            id: Some(id),
            session,
            ok: false,
            error: Some("request id window exhausted; reconnect required".to_owned()),
            reconnect: true,
        };
    }
    seen_ids.insert(id.clone());
    match apply_web_events_client_command(host, command) {
        Ok(()) => WebEventsCommandOutcome {
            result_type,
            id: Some(id),
            session,
            ok: true,
            error: None,
            reconnect: false,
        },
        Err(error) => WebEventsCommandOutcome {
            result_type,
            id: Some(id),
            session,
            ok: false,
            error: Some(error),
            reconnect: false,
        },
    }
}

/// Build a result envelope that never collides with `{ type: "sessions" }` push frames.
fn build_web_events_command_result(
    result_type: &str,
    id: Option<&str>,
    session: Option<&str>,
    ok: bool,
    error: Option<&str>,
    reconnect: bool,
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
    if reconnect {
        map.insert("reconnect".to_owned(), serde_json::Value::Bool(true));
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
    reconnect: bool,
) -> Result<(), ()> {
    let payload = build_web_events_command_result(result_type, id, session, ok, error, reconnect);
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
    seen_ids: &mut HashSet<String>,
    text: &str,
) -> WsClientAction {
    match parse_web_events_client_command(text) {
        Ok(None) => WsClientAction::Continue,
        Ok(Some(command)) => {
            // terminal_input and terminal_resize always resolve to exactly one
            // result frame (input acks included) keyed by the connection-unique
            // request id, so the browser can settle its pending command.
            let outcome = plan_web_events_command_outcome(&state.host, &command, seen_ids);
            let sent = send_web_events_command_result(
                websocket,
                outcome.result_type,
                outcome.id.as_deref(),
                outcome.session.as_deref(),
                outcome.ok,
                outcome.error.as_deref(),
                outcome.reconnect,
            );
            if sent.is_err() {
                return WsClientAction::Close;
            }
            if outcome.reconnect {
                let _ = websocket.close(None);
                WsClientAction::Close
            } else {
                WsClientAction::Continue
            }
        }
        Err(error) => {
            // Malformed known-shape or generic junk that failed parse: never
            // touch PTY. If the parse preserved a recognized id/type/session,
            // send a correlated failure so the browser can settle (and release
            // the admission bytes of) its pending command; fully-unidentifiable
            // junk falls back to a generic input_result the browser ignores.
            let sent = send_web_events_command_result(
                websocket,
                error.result_type,
                error.id.as_deref(),
                error.session.as_deref(),
                false,
                Some(&error.message),
                false,
            );
            if sent.is_err() {
                WsClientAction::Close
            } else {
                WsClientAction::Continue
            }
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

#[derive(Clone, Debug)]
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
    read_http_request_with_deadline(stream, Instant::now() + WEB_HTTP_REQUEST_DEADLINE)
}

/// Parse one HTTP request, bounded by the documented request line/header caps
/// and a wall-clock deadline. `deadline` is injectable for tests.
fn read_http_request_with_deadline(
    stream: &mut TcpStream,
    deadline: Instant,
) -> std::result::Result<ParsedHttpRequest, String> {
    let mut reader = BufReader::new(stream);
    let request_line = read_http_line(&mut reader, WEB_HTTP_MAX_REQUEST_LINE_BYTES, deadline)?;
    let request_line = std::str::from_utf8(&request_line)
        .map_err(|_| "request line is not valid UTF-8".to_owned())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing HTTP method")?.to_owned();
    let path = parts.next().ok_or("missing HTTP path")?.to_owned();
    let mut bridge_header = false;
    let mut host = None;
    let mut origin = None;
    let mut upgrade_websocket = false;
    let mut connection_upgrade = false;
    let mut sec_websocket_key = None;
    let mut sec_websocket_version = None;
    let mut header_count = 0usize;
    let mut header_bytes = 0usize;
    loop {
        let line = read_http_line(&mut reader, WEB_HTTP_MAX_HEADER_LINE_BYTES, deadline)?;
        if line.is_empty() {
            break;
        }
        header_count += 1;
        header_bytes += line.len();
        if header_count > WEB_HTTP_MAX_HEADER_COUNT {
            return Err(format!(
                "too many HTTP headers; the limit is {WEB_HTTP_MAX_HEADER_COUNT}"
            ));
        }
        if header_bytes > WEB_HTTP_MAX_HEADER_BYTES {
            return Err(format!(
                "HTTP headers exceed the total size limit of {WEB_HTTP_MAX_HEADER_BYTES} bytes"
            ));
        }
        let header =
            std::str::from_utf8(&line).map_err(|_| "header is not valid UTF-8".to_owned())?;
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

/// Read one CRLF/LF-terminated line with a hard byte cap and a wall-clock
/// deadline, so a hostile or broken peer can neither make the server allocate
/// an unbounded line nor hold a handler thread past the request budget.
///
/// An empty line (blank separator or EOF) yields an empty `Vec`. A line longer
/// than `cap` or a request that outlives `deadline` fails the parse; idle reads
/// are retried with a short sleep (mirroring the IPC frame poll) so a trickling
/// client cannot stall the deadline.
fn read_http_line(
    reader: &mut impl BufRead,
    cap: usize,
    deadline: Instant,
) -> std::result::Result<Vec<u8>, String> {
    let mut line = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err("HTTP request read timed out".to_owned());
        }
        let buffer = match reader.fill_buf() {
            Ok(buffer) => buffer,
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(WEB_HTTP_POLL);
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        if buffer.is_empty() {
            return Ok(line);
        }
        if let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
            line.extend_from_slice(&buffer[..index]);
            reader.consume(index + 1);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > cap {
                return Err(format!("HTTP line exceeds the {cap}-byte size limit"));
            }
            return Ok(line);
        }
        let length = buffer.len();
        line.extend_from_slice(buffer);
        reader.consume(length);
        if line.len() > cap {
            return Err(format!("HTTP line exceeds the {cap}-byte size limit"));
        }
    }
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
    write_http_bytes_with_deadline(stream, status, content_type, body, WEB_HTTP_WRITE_TIMEOUT)
}

/// Like [`write_http_bytes`], but under an explicit total deadline. Used by
/// the connection-cap rejection path, which writes directly from the accept
/// thread and must not be pinned by a peer that never drains.
fn write_http_bytes_with_deadline(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    deadline: Duration,
) -> std::io::Result<()> {
    if body.len() > WEB_HTTP_MAX_RESPONSE_BYTES {
        // Keep the wire response bounded even if a route produced an oversized
        // body; the replacement body is tiny and cannot recurse.
        return write_http_bytes_with_deadline(
            stream,
            "500 Internal Server Error",
            "text/plain; charset=utf-8",
            b"server response exceeds the size limit",
            deadline,
        );
    }
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    );
    let mut response = Vec::with_capacity(headers.len() + body.len());
    response.extend_from_slice(headers.as_bytes());
    response.extend_from_slice(body);
    write_tcp_all_with_deadline(stream, &response, deadline)
}

/// Minimal write surface for [`write_tcp_all_with_deadline`]: a blocking
/// `write` plus a per-syscall write timeout. The deadline loop is generic
/// over this trait so tests can drive it with a fake writer that reports
/// `WouldBlock` forever, instead of depending on kernel socket buffers,
/// which std::net does not expose and whose sizes differ per platform.
trait DeadlineWrite: Write {
    fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()>;
}

impl DeadlineWrite for TcpStream {
    fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_write_timeout(self, timeout)
    }
}

/// Write one bounded HTTP response under a total wall-clock deadline. A
/// socket write timeout alone is per syscall, so a peer that accepts a few
/// bytes repeatedly could otherwise keep resetting `write_all` forever.
/// Blocked writes (full receive buffer) are retried with a bounded 1–50 ms
/// backoff instead of a busy loop, and the per-syscall timeout is never set
/// below 1 ms so a nearly-expired deadline cannot degrade into spinning.
fn write_tcp_all_with_deadline(
    stream: &mut impl DeadlineWrite,
    mut data: &[u8],
    timeout: Duration,
) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    let mut poll_delay = WEB_HTTP_WRITE_POLL_MIN;
    while !data.is_empty() {
        let now = Instant::now();
        if now >= deadline {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "HTTP response write exceeded its total I/O deadline",
            ));
        }
        stream.set_write_timeout(Some(
            deadline
                .saturating_duration_since(now)
                .max(WEB_HTTP_WRITE_POLL_MIN),
        ))?;
        match stream.write(data) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    ErrorKind::WriteZero,
                    "peer closed while receiving the HTTP response",
                ));
            }
            Ok(written) => {
                data = &data[written..];
                poll_delay = WEB_HTTP_WRITE_POLL_MIN;
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "HTTP response write exceeded its total I/O deadline",
                    ));
                }
                thread::sleep(poll_delay.min(remaining));
                poll_delay = (poll_delay * 2).min(WEB_HTTP_WRITE_POLL_MAX);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
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
    use crate::protocol::SessionPhase;
    use crate::session::tests::{
        test_host_with_poisoned_close_lock, with_test_host_holding_close_lock,
    };
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
    fn close_api_distinguishes_missing_timeout_and_internal_failures() {
        // 404: a missing session through the real request path.
        let response = serve_web_request(
            b"POST /api/sessions/missing/close HTTP/1.1\r\nHost: localhost\r\nX-Grok-Bridge-WebUI: 1\r\n\r\n",
        );
        let (headers, body) = split_http_response(&response);
        assert!(
            headers.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "{headers}"
        );
        assert!(
            std::str::from_utf8(body)
                .unwrap()
                .contains("session not found: missing"),
            "404 body: {body:?}"
        );

        // 504: the close lock is held past the deadline, so close really times
        // out and the route maps it to Gateway Timeout with the error body.
        let response = with_test_host_holding_close_lock(
            "provider-timeout",
            SessionPhase::Running,
            |host| {
                serve_web_request_with_host(
                    b"POST /api/sessions/gbt-test/close HTTP/1.1\r\nHost: localhost\r\nX-Grok-Bridge-WebUI: 1\r\n\r\n",
                    host,
                )
            },
        );
        let (headers, body) = split_http_response(&response);
        assert!(
            headers.starts_with("HTTP/1.1 504 Gateway Timeout\r\n"),
            "{headers}"
        );
        assert!(
            std::str::from_utf8(body)
                .unwrap()
                .contains("close timed out"),
            "504 body: {body:?}"
        );

        // 500: a poisoned close lock is an internal failure through the real
        // request path and surfaces as Internal Server Error.
        let host = test_host_with_poisoned_close_lock("provider-failed", SessionPhase::Running);
        let response = serve_web_request_with_host(
            b"POST /api/sessions/gbt-test/close HTTP/1.1\r\nHost: localhost\r\nX-Grok-Bridge-WebUI: 1\r\n\r\n",
            host,
        );
        let (headers, body) = split_http_response(&response);
        assert!(
            headers.starts_with("HTTP/1.1 500 Internal Server Error\r\n"),
            "{headers}"
        );
        assert!(
            std::str::from_utf8(body)
                .unwrap()
                .contains("session close lock was poisoned"),
            "500 body: {body:?}"
        );
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
    fn parses_only_loopback_web_addr_as_socket_addr() {
        // Default and other IPv4 127.0.0.0/8 addresses, including port 0.
        for value in [
            "127.0.0.1:47653",
            "127.0.0.0:80",
            "127.255.255.254:1",
            "127.0.0.1:0",
        ] {
            assert_eq!(
                parse_web_addr(value).unwrap(),
                value.parse::<SocketAddr>().unwrap(),
                "{value}"
            );
        }
        // IPv6 ::1 is accepted; this only parses, it never needs the machine
        // to support binding an IPv6 socket.
        for value in ["[::1]:47653", "[::1]:0"] {
            assert_eq!(
                parse_web_addr(value).unwrap(),
                value.parse::<SocketAddr>().unwrap(),
                "{value}"
            );
        }
    }

    #[test]
    fn rejects_wildcard_lan_and_public_web_addr_before_binding() {
        for value in [
            "0.0.0.0:47653",            // IPv4 wildcard
            "[::]:47653",               // IPv6 wildcard
            "192.168.1.10:47653",       // LAN (RFC 1918)
            "10.0.0.5:47653",           // LAN (RFC 1918)
            "172.16.0.1:47653",         // LAN (RFC 1918)
            "169.254.1.1:47653",        // link-local
            "8.8.8.8:47653",            // public IPv4
            "1.2.3.4:80",               // public IPv4
            "[2001:db8::1]:47653",      // public IPv6
            "[::ffff:127.0.0.1]:47653", // IPv4-mapped IPv6 is not loopback
        ] {
            let err = parse_web_addr(value).unwrap_err();
            assert!(
                matches!(err, WebAddrError::NotLoopback(_)),
                "expected NotLoopback for {value:?}, got {err:?}"
            );
            let message = err.to_string();
            assert!(message.contains("local-only"), "{value:?}: {message}");
            assert!(message.contains("127.0.0.0/8"), "{value:?}: {message}");
        }
    }

    #[test]
    fn rejects_hostname_and_unresolvable_web_addr_without_dns() {
        for value in [
            "",
            "   ",
            "not-an-address",
            "127.0.0.1",          // missing port
            "127.0.0.1:notaport", // non-numeric port
            "127.0.0.1:70000",    // port out of range
            "localhost:47653",    // hostname must not be resolved
            "myhost.local:47653", // hostname must not be resolved
        ] {
            let err = parse_web_addr(value).unwrap_err();
            assert!(
                matches!(err, WebAddrError::InvalidAddress(_)),
                "expected InvalidAddress for {value:?}, got {err:?}"
            );
            let message = err.to_string();
            assert!(message.contains("local-only"), "{value:?}: {message}");
            assert!(message.contains("hostnames"), "{value:?}: {message}");
        }
    }

    #[test]
    fn non_loopback_or_invalid_web_addr_is_a_startup_error() {
        // These must fail before any TcpListener::bind attempt.
        for value in [
            "0.0.0.0:47653",
            "8.8.8.8:47653",
            "localhost:47653",
            "garbage",
        ] {
            let err = bind_web_ui_from(value).unwrap_err();
            assert!(err.to_string().contains("local-only"), "{value:?}: {err:#}");
        }
    }

    #[test]
    fn occupied_loopback_port_disables_webui_without_startup_error() {
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = occupied.local_addr().unwrap().to_string();
        assert!(
            parse_web_addr(&address).is_ok(),
            "{address} must be loopback"
        );
        let result = bind_web_ui_from(&address).unwrap();
        assert!(
            result.is_none(),
            "occupied port must keep WebUI unavailable"
        );
    }

    #[test]
    fn non_addr_in_use_web_bind_errors_remain_startup_errors() {
        for kind in [ErrorKind::PermissionDenied, ErrorKind::AddrNotAvailable] {
            let error = classify_web_ui_bind(
                "127.0.0.1:47653",
                Err(io::Error::new(kind, "injected bind failure")),
            )
            .unwrap_err();
            assert!(error.to_string().contains("failed to bind WebUI"));
            assert_eq!(error.downcast_ref::<io::Error>().unwrap().kind(), kind);
        }
    }

    #[test]
    fn http_line_reading_is_capped_and_strips_line_endings() {
        let mut reader = BufReader::new(&b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"[..]);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_eq!(
            read_http_line(&mut reader, 64, deadline).unwrap(),
            b"GET / HTTP/1.1"
        );
        assert_eq!(
            read_http_line(&mut reader, 64, deadline).unwrap(),
            b"Host: x"
        );
        assert!(
            read_http_line(&mut reader, 64, deadline)
                .unwrap()
                .is_empty()
        );

        let long = [b'a'; 16];
        let mut reader = BufReader::new(&long[..]);
        assert!(read_http_line(&mut reader, 8, deadline).is_err());
    }

    #[test]
    fn oversized_request_line_is_rejected_with_400() {
        let path = format!("/{}", "x".repeat(WEB_HTTP_MAX_REQUEST_LINE_BYTES));
        let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let response = serve_web_request(request.as_bytes());
        assert!(response.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
        let (_, body) = split_http_response(&response);
        assert!(
            String::from_utf8_lossy(body).contains("size limit"),
            "{}",
            String::from_utf8_lossy(body)
        );
    }

    #[test]
    fn too_many_headers_are_rejected_with_400() {
        let mut request = String::from("GET / HTTP/1.1\r\nHost: localhost\r\n");
        for _ in 0..(WEB_HTTP_MAX_HEADER_COUNT + 1) {
            request.push_str("X-Pad: 1\r\n");
        }
        request.push_str("\r\n");
        let response = serve_web_request(request.as_bytes());
        assert!(response.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
        let (_, body) = split_http_response(&response);
        assert!(String::from_utf8_lossy(body).contains("headers"));
    }

    #[test]
    fn oversized_total_header_bytes_are_rejected_with_400() {
        // Five ~7 KiB headers stay under the per-line cap but exceed the
        // 32 KiB total header budget.
        let mut request = String::from("GET / HTTP/1.1\r\nHost: localhost\r\n");
        for index in 0..5 {
            request.push_str(&format!("X-Pad-{index}: {}\r\n", "x".repeat(7000)));
        }
        request.push_str("\r\n");
        let response = serve_web_request(request.as_bytes());
        assert!(response.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
        let (_, body) = split_http_response(&response);
        assert!(
            String::from_utf8_lossy(body).contains("size limit"),
            "{}",
            String::from_utf8_lossy(body)
        );
    }

    #[test]
    fn http_request_read_stops_at_the_wall_clock_deadline_for_a_trickling_client() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.set_nonblocking(true).unwrap();
        client.write_all(b"GET / HT").unwrap(); // incomplete request line

        let deadline = Instant::now() + Duration::from_millis(300);
        let started = Instant::now();
        let err = read_http_request_with_deadline(&mut server, deadline).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn http_response_body_over_the_limit_is_replaced_with_a_bounded_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let oversized = vec![0u8; WEB_HTTP_MAX_RESPONSE_BYTES + 1];

        write_http_bytes(
            &mut server,
            "200 OK",
            "application/octet-stream",
            &oversized,
        )
        .unwrap();
        drop(server);

        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let (headers, body) = split_http_response(&response);
        assert!(headers.starts_with("HTTP/1.1 500 Internal Server Error\r\n"));
        assert_eq!(body, b"server response exceeds the size limit");
    }

    #[test]
    fn http_write_stops_at_the_total_deadline_for_a_peer_that_stops_draining() {
        // Deterministic fake peer: accepts a bounded prefix, then reports
        // WouldBlock forever — the same failure mode as a real receive
        // buffer filling up, without depending on per-platform kernel socket
        // buffers (which std::net does not expose, and whose sizes differ
        // between Windows loopback and Unix).
        struct StalledWrite {
            accepted: usize,
        }

        impl Write for StalledWrite {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if self.accepted >= 4 * 1024 {
                    return Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "peer stopped draining",
                    ));
                }
                let take = (4 * 1024 - self.accepted).min(buf.len());
                self.accepted += take;
                Ok(take)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl DeadlineWrite for StalledWrite {
            fn set_write_timeout(&mut self, _timeout: Option<Duration>) -> io::Result<()> {
                Ok(())
            }
        }

        // The payload far exceeds what the peer accepts; the write must
        // stall until the total deadline, with the bounded backoff keeping
        // the loop from running past it by much.
        let payload = vec![0x55u8; 8 * 1024];
        let started = Instant::now();
        let error = write_tcp_all_with_deadline(
            &mut StalledWrite { accepted: 0 },
            &payload,
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(150), "{elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    }

    #[test]
    fn connection_admission_is_bounded_and_releases_slots() {
        let slots = AtomicUsize::new(0);
        for _ in 0..3 {
            assert!(admit_connection(&slots, 3));
        }
        assert!(!admit_connection(&slots, 3));
        assert_eq!(slots.load(Ordering::Relaxed), 3);

        drop(ConnectionSlot { slots: &slots });
        assert_eq!(slots.load(Ordering::Relaxed), 2);
        assert!(admit_connection(&slots, 3));
    }

    #[test]
    fn websocket_config_has_explicit_frame_message_and_buffer_limits() {
        let config = web_socket_config();
        assert_eq!(config.max_message_size, Some(WEB_EVENTS_MAX_MESSAGE_BYTES));
        assert_eq!(config.max_frame_size, Some(WEB_EVENTS_MAX_MESSAGE_BYTES));
        assert_eq!(config.write_buffer_size, 0);
        assert_eq!(config.max_write_buffer_size, WEB_EVENTS_MAX_MESSAGE_BYTES);
        assert_eq!(config.read_buffer_size, 8 * 1024);
    }

    #[test]
    fn connection_caps_and_http_limits_are_bounded() {
        assert!((1..=256).contains(&WEB_MAX_ACTIVE_CONNECTIONS));
        assert!((1..=256).contains(&IPC_MAX_ACTIVE_CONNECTIONS));
        // Compile-time invariants: header lines nest inside the header budget
        // and the request line/response budgets stay large enough to be useful.
        const {
            assert!(WEB_HTTP_MAX_HEADER_LINE_BYTES <= WEB_HTTP_MAX_HEADER_BYTES);
            assert!(WEB_HTTP_MAX_REQUEST_LINE_BYTES >= 256);
            assert!(WEB_HTTP_MAX_RESPONSE_BYTES >= 1024 * 1024);
        }
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
                id: "r1".to_owned(),
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
                id: "r2".to_owned(),
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
    fn parse_rejects_missing_empty_or_oversized_request_ids() {
        let missing = r#"{"type":"terminal_input","session":"gbt-1","data_base64":"YQ=="}"#;
        assert!(parse_web_events_client_command(missing).is_err());
        let empty = r#"{"type":"terminal_resize","id":"","session":"gbt-1","cols":80,"rows":24}"#;
        assert!(parse_web_events_client_command(empty).is_err());
        let numeric = r#"{"type":"terminal_input","id":7,"session":"gbt-1","data_base64":"YQ=="}"#;
        assert!(parse_web_events_client_command(numeric).is_err());
        let long_id = format!(
            r#"{{"type":"terminal_input","id":{id},"session":"gbt-1","data_base64":"YQ=="}}"#,
            id = serde_json::to_string(&"x".repeat(WEB_EVENTS_MAX_REQUEST_ID_BYTES + 1)).unwrap()
        );
        assert!(parse_web_events_client_command(&long_id).is_err());
    }

    #[test]
    fn parse_rejects_malformed_and_oversized_input_without_command() {
        assert!(parse_web_events_client_command("not-json").is_err());
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","id":"r1","session":"","data_base64":"YQ=="}"#
            )
            .is_err()
        );
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","id":"r1","session":"gbt-1"}"#
            )
            .is_err()
        );
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_resize","id":"r1","session":"gbt-1","cols":1,"rows":40}"#
            )
            .is_ok()
        );
        // cols=1 is parsed; apply-time validate_terminal_size rejects it.
        let cmd = parse_web_events_client_command(
            r#"{"type":"terminal_resize","id":"r1","session":"gbt-1","cols":1,"rows":40}"#,
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
            id: "n1".to_owned(),
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
            id: "e1".to_owned(),
            session: "missing-session".to_owned(),
            data_base64: "".to_owned(),
        };
        // Empty base64 fails at parse (required).
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","id":"r1","session":"s","data_base64":""}"#
            )
            .is_err()
        );
        let _ = empty;

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let oversize = BASE64.encode(vec![0x5a; crate::protocol::MAX_WRITE_BYTES + 1]);
        let over = WebEventsClientCommand::TerminalInput {
            id: "o1".to_owned(),
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
            false,
        );
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["type"], "input_result");
        assert_eq!(value["ok"], false);
        assert_eq!(value["id"], "r1");
        assert_eq!(value["session"], "gbt-1");
        assert_eq!(value["error"], "nope");
        assert!(value.get("reconnect").is_none());
        assert!(value.get("sessions").is_none());
        assert!(value.get("terminals").is_none());

        let rotate = build_web_events_command_result(
            "input_result",
            Some("r2"),
            Some("gbt-1"),
            false,
            Some("request id window exhausted; reconnect required"),
            true,
        );
        let rotate: serde_json::Value = serde_json::from_str(&rotate).unwrap();
        assert_eq!(rotate["reconnect"], true);
    }

    #[test]
    fn client_poll_interval_is_bounded_for_interactive_latency() {
        // Guard against regressions that reintroduce multi-second waits before
        // reading client frames.
        assert!(WEB_EVENTS_CLIENT_POLL <= Duration::from_millis(100));
        assert!(WEB_EVENTS_CLIENT_POLL >= Duration::from_millis(1));
    }

    #[test]
    fn web_events_command_plan_rejects_duplicate_ids_and_acks_input_success() {
        use std::collections::HashSet as StdSet;

        let host = SessionHost::new(OrphanPolicy {
            lease_ms: 120_000,
            grace_ms: 600_000,
        });
        let command = WebEventsClientCommand::TerminalInput {
            id: "k1".to_owned(),
            session: "missing-session".to_owned(),
            data_base64: "YQ==".to_owned(),
        };
        let mut seen = StdSet::new();
        // Unknown session fails apply, but still resolves to exactly one result.
        let outcome = plan_web_events_command_outcome(&host, &command, &mut seen);
        assert_eq!(outcome.result_type, "input_result");
        assert_eq!(outcome.id.as_deref(), Some("k1"));
        assert!(!outcome.ok);
        assert!(outcome.error.is_some());

        // Same id again on the same connection is a strict duplicate rejection,
        // even though the first attempt never touched a PTY.
        let duplicate = plan_web_events_command_outcome(&host, &command, &mut seen);
        assert_eq!(duplicate.id.as_deref(), Some("k1"));
        assert!(!duplicate.ok);
        assert!(
            duplicate
                .error
                .as_deref()
                .is_some_and(|error| error.contains("duplicate request id"))
        );

        // A fresh id resolves independently.
        let other = WebEventsClientCommand::TerminalInput {
            id: "k2".to_owned(),
            session: "missing-session".to_owned(),
            data_base64: "YQ==".to_owned(),
        };
        let second = plan_web_events_command_outcome(&host, &other, &mut seen);
        assert_eq!(second.id.as_deref(), Some("k2"));
        assert!(!second.ok);

        // A missing session fails apply for resize too, but still resolves to
        // exactly one resize_result carrying the id.
        let resize = WebEventsClientCommand::TerminalResize {
            id: "k3".to_owned(),
            session: "missing-session".to_owned(),
            cols: 80,
            rows: 24,
        };
        let resize_outcome = plan_web_events_command_outcome(&host, &resize, &mut seen);
        assert_eq!(resize_outcome.result_type, "resize_result");
        assert_eq!(resize_outcome.id.as_deref(), Some("k3"));
        assert!(!resize_outcome.ok);
    }

    #[test]
    fn request_id_window_rotates_connection_when_full_without_evicting_old_ids() {
        use std::collections::HashSet as StdSet;

        let host = SessionHost::new(OrphanPolicy {
            lease_ms: 120_000,
            grace_ms: 600_000,
        });
        let mut seen = StdSet::new();
        let first_id = "bulk-0".to_owned();
        for index in 0..WEB_EVENTS_MAX_TRACKED_REQUEST_IDS {
            let command = WebEventsClientCommand::TerminalInput {
                id: format!("bulk-{index}"),
                session: "missing-session".to_owned(),
                data_base64: "YQ==".to_owned(),
            };
            let outcome = plan_web_events_command_outcome(&host, &command, &mut seen);
            assert!(
                !outcome
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("duplicate request id"))
            );
        }
        assert_eq!(seen.len(), WEB_EVENTS_MAX_TRACKED_REQUEST_IDS);

        // Window is full: the new command is rejected before apply and the
        // result instructs the browser to rotate this exhausted connection.
        let extra = WebEventsClientCommand::TerminalInput {
            id: "bulk-overflow".to_owned(),
            session: "missing-session".to_owned(),
            data_base64: "YQ==".to_owned(),
        };
        let overflow = plan_web_events_command_outcome(&host, &extra, &mut seen);
        assert!(!overflow.ok);
        assert_eq!(
            overflow.error.as_deref(),
            Some("request id window exhausted; reconnect required")
        );
        assert!(overflow.reconnect);
        assert_eq!(seen.len(), WEB_EVENTS_MAX_TRACKED_REQUEST_IDS);

        // An OLD applied-or-rejected id is never re-admitted for replay: it is
        // explicitly rejected as a duplicate, never re-applied.
        let replay = WebEventsClientCommand::TerminalInput {
            id: first_id,
            session: "missing-session".to_owned(),
            data_base64: "YQ==".to_owned(),
        };
        let replay_outcome = plan_web_events_command_outcome(&host, &replay, &mut seen);
        assert!(!replay_outcome.ok);
        assert_eq!(
            replay_outcome.error.as_deref(),
            Some("duplicate request id")
        );
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
                r#"{{"type":"terminal_input","id":"r1","session":{session},"data_base64":"YQ=="}}"#,
                session = serde_json::to_string(bad).unwrap()
            );
            let err = parse_web_events_client_command(&payload).unwrap_err();
            assert!(
                err.message.contains("session handle") || err.message.contains("session"),
                "bad={bad:?} err={:?}",
                err.message
            );
            let resize = format!(
                r#"{{"type":"terminal_resize","id":"r1","session":{session},"cols":80,"rows":24}}"#,
                session = serde_json::to_string(bad).unwrap()
            );
            assert!(parse_web_events_client_command(&resize).is_err());
        }
        // Valid handle shape is accepted by the parser (host still enforces existence).
        assert!(
            parse_web_events_client_command(
                r#"{"type":"terminal_input","id":"r1","session":"gbt-1","data_base64":"YQ=="}"#
            )
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn parse_errors_keep_recognized_id_session_and_result_type() {
        // Malformed known-shape command: the error still carries the request
        // id, session, and result type so the browser can settle its pending
        // entry and release the admission bytes it holds.
        let err = parse_web_events_client_command(
            r#"{"type":"terminal_input","id":"r1","session":"gbt-1"}"#,
        )
        .unwrap_err();
        assert_eq!(err.id.as_deref(), Some("r1"));
        assert_eq!(err.session.as_deref(), Some("gbt-1"));
        assert_eq!(err.result_type, "input_result");
        assert!(err.message.contains("data_base64"));

        // Invalid session handle: id and result type survive.
        let err = parse_web_events_client_command(
            r#"{"type":"terminal_input","id":"r2","session":"bad id","data_base64":"YQ=="}"#,
        )
        .unwrap_err();
        assert_eq!(err.id.as_deref(), Some("r2"));
        assert_eq!(err.result_type, "input_result");
        assert_eq!(err.session.as_deref(), Some("bad id"));

        // resize errors keep resize_result.
        let err = parse_web_events_client_command(
            r#"{"type":"terminal_resize","id":"r3","session":"gbt-1"}"#,
        )
        .unwrap_err();
        assert_eq!(err.id.as_deref(), Some("r3"));
        assert_eq!(err.result_type, "resize_result");
        assert!(err.message.contains("cols"));

        // Missing id: nothing to correlate, generic fallback type.
        let err = parse_web_events_client_command(
            r#"{"type":"terminal_input","session":"gbt-1","data_base64":"YQ=="}"#,
        )
        .unwrap_err();
        assert_eq!(err.id, None);
        assert_eq!(err.session.as_deref(), Some("gbt-1"));
        assert_eq!(err.result_type, "input_result");
    }

    #[test]
    fn generic_parse_errors_stay_id_less_with_input_result_type() {
        // Junk that carries no recognizable id keeps the generic fallback.
        for payload in ["not-json", r#"[]"#, r#""plain string""#] {
            let err = parse_web_events_client_command(payload).unwrap_err();
            assert_eq!(err.id, None);
            assert_eq!(err.result_type, "input_result");
        }
        // A non-object with an unknown type is ignored, not an error.
        assert_eq!(
            parse_web_events_client_command(r#"{"type":7}"#).unwrap(),
            None
        );
        // Oversized / non-string id is also not correlatable.
        let err = parse_web_events_client_command(
            r#"{"type":"terminal_input","id":7,"session":"gbt-1","data_base64":"YQ=="}"#,
        )
        .unwrap_err();
        assert_eq!(err.id, None);
        assert_eq!(err.result_type, "input_result");
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
            serde_json::json!({ "matched": 0, "closed": 0, "timed_out": [], "failures": [] })
        );
    }

    fn serve_web_request(request: &[u8]) -> Vec<u8> {
        serve_web_request_with_host(
            request,
            SessionHost::new(OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            }),
        )
    }

    fn serve_web_request_with_host(request: &[u8], host: SessionHost) -> Vec<u8> {
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
                    host,
                    started_at_ms: 0,
                    stopping: AtomicBool::new(false),
                    web_url: None,
                    version_checker: Arc::new(VersionChecker::new()),
                    web_connections: AtomicUsize::new(0),
                    ipc_connections: AtomicUsize::new(0),
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

    /// The Unix liveness probe must accept a live listener and refuse a stale
    /// socket file, on a throwaway path that never touches the real runtime.
    /// Covered: no socket file (ENOENT), a live listener, and a socket file
    /// with no listener behind it (ECONNREFUSED). The throwaway directory is
    /// [`crate::transport::ShortTempDir`], a canonical-`/tmp` short path whose
    /// Drop-based cleanup runs even when an assertion fails.
    #[cfg(unix)]
    #[test]
    fn unix_liveness_probe_distinguishes_live_and_stale_sockets() {
        use std::os::unix::io::IntoRawFd as _;

        let temp = crate::transport::ShortTempDir::new("probe");

        // No socket file at the path: definitively not alive.
        let missing = temp.path().join("missing.sock");
        let missing_name = missing
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .unwrap();
        assert!(!runtime_server_is_alive_for(&missing_name));

        // A live listener behind the path: must probe as alive.
        let socket = temp.path().join("probe.sock");
        let name = socket
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .unwrap();
        let listener = interprocess::local_socket::ListenerOptions::new()
            .name(name.clone())
            .create_sync()
            .unwrap();
        assert!(runtime_server_is_alive_for(&name));
        drop(listener);

        // A socket file with no listener behind it (a crashed Runtime's
        // leftover): refuses the probe and must not count as alive.
        let stale = temp.path().join("stale.sock");
        let stale_listener = std::os::unix::net::UnixListener::bind(&stale).unwrap();
        let fd = stale_listener.into_raw_fd();
        unsafe {
            libc::close(fd);
        }
        assert!(stale.exists(), "stale socket file must remain on disk");
        let stale_name = stale
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .unwrap();
        assert!(!runtime_server_is_alive_for(&stale_name));
    }

    /// An inconclusive probe (any connect error other than missing/refused)
    /// must be surfaced as `Uncertain` with the original error retained, and
    /// the conservative [`runtime_server_is_alive_for`] bool must still read
    /// it as alive so a live Runtime's socket is never treated as stale.
    #[cfg(unix)]
    #[test]
    fn uncertain_probe_error_is_retained_and_counts_as_alive() {
        let temp = crate::transport::ShortTempDir::new("probe-uncertain");

        // A path whose parent is a regular file makes connect fail with
        // ENOTDIR — neither "no socket file" nor "connection refused", so the
        // probe must stay Uncertain instead of collapsing into a boolean.
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let name = blocker
            .join("runtime.sock")
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .unwrap();
        match probe_runtime_name(&name) {
            ProbeOutcome::Uncertain(error) => {
                assert_eq!(error.kind(), ErrorKind::NotADirectory);
            }
            outcome => panic!("expected Uncertain, got {outcome:?}"),
        }
        assert!(
            runtime_server_is_alive_for(&name),
            "an uncertain probe must still count as alive for the current endpoint"
        );

        // Missing and refused stay definitively dead, so startup and stale
        // cleanup treat them as absent.
        let missing = temp.path().join("missing.sock");
        let missing_name = missing
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .unwrap();
        assert!(matches!(
            probe_runtime_name(&missing_name),
            ProbeOutcome::Dead
        ));
        assert!(!runtime_server_is_alive_for(&missing_name));
    }
}
