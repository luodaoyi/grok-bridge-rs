#[cfg(unix)]
use std::io;
#[cfg(any(windows, test))]
use std::io::Read;
use std::{
    env,
    ffi::OsString,
    io::{BufRead, BufReader, ErrorKind, Write},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
#[cfg(not(unix))]
use interprocess::local_socket::GenericNamespaced;
#[cfg(any(unix, test))]
use interprocess::local_socket::ListenerOptions;
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, GenericNamespaced, Listener};
use interprocess::local_socket::{Name, Stream, prelude::*};
#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CreateProcessW, PROCESS_INFORMATION,
        STARTUPINFOW,
    },
};

use crate::protocol::{
    MAX_FRAME_BYTES, Request, RequestEnvelope, ResponseEnvelope, decode_response, encode_frame,
    validate_client_session_id,
};

const START_RETRIES: usize = 50;
const START_RETRY_DELAY: Duration = Duration::from_millis(100);
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Server-side deadline for receiving a complete request frame from an IPC
/// client. A connected-but-silent or trickling client never pins a handler
/// thread beyond this.
pub(crate) const IPC_FRAME_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Deadline for any single frame write (client request or server response).
/// Frames are at most 1 MiB and written in one burst, so a peer that stops
/// draining surfaces an error within this window.
pub(crate) const IPC_WRITE_DEADLINE: Duration = Duration::from_secs(30);
/// Short total deadline for a connection-cap rejection frame written on the
/// accept thread: a peer that never drains must not pin acceptance of further
/// clients for the full IPC write deadline.
pub(crate) const IPC_REJECT_DEADLINE: Duration = Duration::from_secs(2);
#[cfg(unix)]
const RUNTIME_STARTUP_LOCK_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll backoff bounds while an IPC peer is idle, so a waiting thread sleeps
/// instead of spinning yet still notices data promptly.
const IPC_POLL_MIN: Duration = Duration::from_millis(1);
const IPC_POLL_MAX: Duration = Duration::from_millis(50);
/// Extra headroom the client grants the server beyond the request's own
/// documented timeout (`read` wait and `wait` timeout are bounded server-side
/// in session.rs) so legitimate long operations are never cut short.
const IPC_RESPONSE_SLACK: Duration = Duration::from_secs(60);

pub(crate) fn call(request: Request, auto_start: bool) -> Result<ResponseEnvelope> {
    call_with_client_session(request, auto_start, current_client_session_id()?)
}

pub(crate) fn call_anonymous(request: Request, auto_start: bool) -> Result<ResponseEnvelope> {
    call_with_client_session(request, auto_start, None)
}

fn call_with_client_session(
    request: Request,
    auto_start: bool,
    client_session_id: Option<String>,
) -> Result<ResponseEnvelope> {
    let envelope = RequestEnvelope {
        id: next_request_id(),
        client_session_id,
        request,
    };
    let stream = match connect() {
        Ok(stream) => stream,
        Err(first_error) if auto_start => {
            start_detached_server().context("failed to launch the Grok runtime server")?;
            let mut last_error = first_error;
            for _ in 0..START_RETRIES {
                thread::sleep(START_RETRY_DELAY);
                match connect() {
                    Ok(stream) => return call_over_stream(stream, &envelope),
                    Err(error) => last_error = error,
                }
            }
            return Err(last_error)
                .context("runtime server did not become ready within five seconds");
        }
        Err(error) => return Err(error),
    };
    call_over_stream(stream, &envelope)
}

fn current_client_session_id() -> Result<Option<String>> {
    client_session_id_from(
        env::var("CODEX_THREAD_ID").ok(),
        env::var("CODEX_SESSION_ID").ok(),
    )
}

fn client_session_id_from(
    thread_id: Option<String>,
    session_id: Option<String>,
) -> Result<Option<String>> {
    let value = thread_id
        .filter(|value| !value.trim().is_empty())
        .or_else(|| session_id.filter(|value| !value.trim().is_empty()));
    if let Some(value) = value.as_deref() {
        validate_client_session_id(value)?;
    }
    Ok(value)
}

/// Connect to the Runtime IPC endpoint. On Unix the client tries the current
/// filesystem endpoint first and, when that fails, probes the legacy
/// GenericNamespaced endpoint so a Runtime started by an older binary is still
/// found and used instead of starting a second one. Only when neither endpoint
/// answers does the caller fall back to auto-starting a new Runtime. A
/// candidate whose name cannot even be constructed (e.g. a path that would
/// overflow the socket address) is skipped, never fatal: the legacy endpoint
/// must still be probed. If every candidate fails to construct, that error is
/// preserved for diagnosis instead of being replaced by a generic connect
/// failure.
fn connect() -> Result<Stream> {
    #[cfg(unix)]
    let candidates = [runtime_name(), legacy_runtime_name()];
    #[cfg(not(unix))]
    let candidates = [runtime_name()];
    let mut names = Vec::with_capacity(candidates.len());
    let mut construction_error = None;
    for candidate in candidates {
        match candidate {
            Ok(name) => names.push(name),
            Err(error) => {
                if construction_error.is_none() {
                    construction_error = Some(error);
                }
            }
        }
    }
    connect_first(&names).map_err(|error| match construction_error {
        Some(name_error) => error.context(format!(
            "a runtime endpoint name could not be constructed: {name_error:#}"
        )),
        None => error,
    })
}

/// Try each IPC endpoint in order and return the first live connection.
/// `candidates` are injected so tests can prove the fallback order against
/// throwaway names without touching the real runtime endpoints. An empty
/// candidate list is a diagnosable error, never a panic.
fn connect_first(candidates: &[Name<'_>]) -> Result<Stream> {
    let mut last_error = None;
    for name in candidates {
        match Stream::connect(name.clone()) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error).context("runtime server is not running"),
        None => bail!("no runtime IPC endpoint candidates are available"),
    }
}

/// Windows named pipes in PIPE_NOWAIT mode report "no data yet" as a
/// successful zero-byte read (`ERROR_NO_DATA` → `Ok(0)`), not `WouldBlock`.
/// `BufReader` treats `Ok(0)` as sticky EOF, so a reader that polls before
/// the peer has written would permanently see "protocol peer closed" even
/// after the bytes arrive. Remap empty reads to `WouldBlock` *before* they
/// reach `BufReader`. Real disconnects still surface as `ERROR_BROKEN_PIPE`
/// / `ERROR_PIPE_NOT_CONNECTED` errors. Used on the client response-read
/// path and the server request-read path. Unix is not wrapped: Unix `Ok(0)`
/// is real EOF. Matches `write_frame_all`, which already treats Windows
/// `Ok(0)` as stall.
#[cfg(any(windows, test))]
pub(crate) struct RemapNowaitEmptyRead<R>(pub(crate) R);

#[cfg(any(windows, test))]
impl<R: Read> Read for RemapNowaitEmptyRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.0.read(buf) {
            Ok(0) => Err(std::io::Error::from(ErrorKind::WouldBlock)),
            other => other,
        }
    }
}

#[cfg(any(windows, test))]
impl<W: Write> Write for RemapNowaitEmptyRead<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

fn call_over_stream(stream: Stream, envelope: &RequestEnvelope) -> Result<ResponseEnvelope> {
    // Non-blocking I/O lets both sides enforce I/O deadlines on every platform
    // (Windows named pipes do not support native socket timeouts).
    stream
        .set_nonblocking(true)
        .context("failed to enable non-blocking runtime I/O")?;
    let response_deadline = Instant::now() + response_deadline_for(&envelope.request);
    #[cfg(windows)]
    let mut connection = BufReader::new(RemapNowaitEmptyRead(stream));
    #[cfg(not(windows))]
    let mut connection = BufReader::new(stream);
    write_frame_all(
        connection.get_mut(),
        &encode_frame(envelope)?,
        IPC_WRITE_DEADLINE,
    )
    .context("failed to write runtime request")?;
    let frame = read_frame(&mut connection, Some(response_deadline))
        .context("failed to read runtime response")?;
    let response = decode_response(&frame)?;
    if response.id != envelope.id {
        bail!(
            "runtime response id mismatch: expected {}, received {}",
            envelope.id,
            response.id
        );
    }
    Ok(response)
}

/// Response read deadline mirroring the server's per-operation timeouts plus
/// slack. `Session::read` clamps its wait to 300 s and `Session::wait` clamps
/// its timeout to 7_200_000 ms (session.rs); a request that uses the full
/// documented timeout must not be cut short by an unrelated default deadline.
fn response_deadline_for(request: &Request) -> Duration {
    let base_ms = match request {
        Request::Read { wait_ms, .. } => wait_ms.unwrap_or(0).min(300_000),
        Request::Wait { timeout_ms, .. } => timeout_ms.unwrap_or(300_000).min(7_200_000),
        _ => 0,
    };
    Duration::from_millis(base_ms) + IPC_RESPONSE_SLACK
}

/// The Runtime IPC name as understood by the local socket API. On Unix this is
/// a filesystem socket path inside the private owner-only runtime directory; on
/// Windows (and other platforms) it remains a namespaced named pipe. Server and
/// client both compute the name through this function, so the two sides always
/// agree within the same environment.
#[cfg(unix)]
pub(crate) fn runtime_name() -> Result<Name<'static>> {
    runtime_socket_path()?
        .to_fs_name::<GenericFilePath>()
        .context("failed to construct the runtime socket path")
}

/// The legacy Runtime IPC name, kept for upgrade compatibility on Unix.
/// Binaries before the filesystem-socket move bound a GenericNamespaced pipe;
/// a Runtime still running from an older build answers on this name, so the
/// client probes it before starting a new Runtime. This preserves access to
/// existing sessions and avoids a second Runtime contending for the WebUI
/// port. Windows has no migration: the namespaced pipe never changed.
#[cfg(unix)]
pub(crate) fn legacy_runtime_name() -> Result<Name<'static>> {
    runtime_identity()
        .to_ns_name::<GenericNamespaced>()
        .context("failed to construct the legacy runtime pipe name")
}

#[cfg(not(unix))]
pub(crate) fn runtime_name() -> Result<Name<'static>> {
    let identity = runtime_identity();
    identity
        .to_ns_name::<GenericNamespaced>()
        .context("failed to construct the runtime pipe name")
}

/// On Unix, the private per-user directory that holds the Runtime IPC socket.
/// A configured XDG base is used only when it is an absolute, owner-only real
/// directory owned by this uid. Invalid values fall back to an absolute temp
/// base; the private child remains uid-namespaced and is secured to 0700.
#[cfg(unix)]
pub(crate) fn runtime_dir() -> Result<PathBuf> {
    runtime_dir_from(
        env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty()),
        env::temp_dir(),
        current_uid(),
    )
}

/// Conservative cross-platform byte budget for a Unix socket address path,
/// excluding the NUL terminator. Linux `sockaddr_un.sun_path` holds 108
/// bytes (107 usable) and macOS holds 104 (103 usable); 100 leaves headroom
/// on both while keeping the path plus its NUL under every supported limit.
#[cfg(unix)]
const UNIX_SOCKET_PATH_MAX_BYTES: usize = 100;

/// Raw byte length of a path as it will be stored in a `sockaddr_un.sun_path`.
#[cfg(unix)]
fn socket_path_bytes(path: &Path) -> usize {
    path.as_os_str().as_bytes().len()
}

/// Whether the runtime socket at `base/identity/runtime.sock` fits the Unix
/// socket address limit. An arbitrarily deep XDG_RUNTIME_DIR must never push
/// the socket path past the platform `sockaddr_un` budget, so over-long bases
/// are skipped in favor of the next trusted base.
#[cfg(unix)]
fn socket_path_fits(base: &Path, identity: &OsStr, max_bytes: usize) -> bool {
    socket_path_bytes(&base.join(identity).join("runtime.sock")) <= max_bytes
}

/// Select the runtime base directory. Candidates are tried in order (XDG,
/// temp, `/tmp`); each must pass the existing trust checks and keep the final
/// socket path within `max_bytes`. `/tmp` is always short enough to fit, so
/// the fallback chain is deterministic: the same environment always selects
/// the same base, and per-user isolation (the uid-suffixed identity child),
/// the 0700 directory tightening, and the allowed-roots trust checks are all
/// preserved by [`trusted_runtime_base`].
#[cfg(unix)]
fn runtime_dir_from(xdg: Option<OsString>, temp: PathBuf, expected_uid: u32) -> Result<PathBuf> {
    runtime_dir_from_with_limit(xdg, temp, expected_uid, UNIX_SOCKET_PATH_MAX_BYTES)
}

#[cfg(unix)]
fn runtime_dir_from_with_limit(
    xdg: Option<OsString>,
    temp: PathBuf,
    expected_uid: u32,
    max_socket_bytes: usize,
) -> Result<PathBuf> {
    let identity = runtime_identity();
    let fits = |base: &Path| socket_path_fits(base, &identity, max_socket_bytes);
    let base = xdg
        .map(PathBuf::from)
        .and_then(|path| trusted_runtime_base(&path, expected_uid, true))
        .filter(|base| fits(base))
        .or_else(|| trusted_runtime_base(&temp, expected_uid, false))
        .filter(|base| fits(base))
        .or_else(|| trusted_runtime_base(Path::new("/tmp"), expected_uid, false))
        .filter(|base| fits(base))
        .context("no trusted absolute runtime base fits the Unix socket path limit")?;
    Ok(base.join(identity))
}

#[cfg(all(unix, test))]
fn trusted_xdg_runtime_base(path: &Path, expected_uid: u32) -> bool {
    trusted_runtime_base(path, expected_uid, true).is_some()
}

/// Resolve an existing base and verify that no directory in its canonical
/// ancestor chain can be replaced by another user. Root and the current uid
/// are trusted owners. A group/world-writable directory is accepted only with
/// the sticky bit, which prevents peers from renaming another owner's entry.
#[cfg(unix)]
fn trusted_runtime_base(
    path: &Path,
    expected_uid: u32,
    require_owner_only: bool,
) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let original = std::fs::symlink_metadata(path).ok()?;
    if require_owner_only && (original.file_type().is_symlink() || !original.is_dir()) {
        return None;
    }
    if !require_owner_only && !original.is_dir() && !original.file_type().is_symlink() {
        return None;
    }
    if require_owner_only
        && (original.uid() != expected_uid || original.permissions().mode() & 0o077 != 0)
    {
        return None;
    }
    let canonical = std::fs::canonicalize(path).ok()?;
    if !canonical.is_absolute() {
        return None;
    }
    if !trusted_ancestor_chain(path, expected_uid, true)
        || !trusted_ancestor_chain(&canonical, expected_uid, false)
    {
        return None;
    }
    Some(canonical)
}

#[cfg(unix)]
fn trusted_ancestor_chain(path: &Path, expected_uid: u32, allow_symlinks: bool) -> bool {
    for ancestor in path.ancestors() {
        let Ok(meta) = std::fs::symlink_metadata(ancestor) else {
            return false;
        };
        if meta.file_type().is_symlink() {
            if !allow_symlinks || (meta.uid() != 0 && meta.uid() != expected_uid) {
                return false;
            }
            continue;
        }
        if !meta.is_dir() {
            return false;
        }
        if meta.uid() != 0 && meta.uid() != expected_uid {
            return false;
        }
        let mode = meta.permissions().mode();
        if mode & 0o022 != 0 && mode & 0o1000 == 0 {
            return false;
        }
    }
    true
}

/// On Unix, the socket file lives inside the private runtime directory.
#[cfg(unix)]
pub(crate) fn runtime_socket_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("runtime.sock"))
}

/// Read one NDJSON frame, bounded by `MAX_FRAME_BYTES` and an optional read
/// deadline.
///
/// With `deadline` set, the peer is expected to keep the stream non-blocking:
/// idle reads poll with a short backoff and the call fails once the deadline
/// passes, so a silent or trickling peer can never pin the calling thread
/// indefinitely. With `deadline = None` the read blocks like a plain
/// `fill_buf` loop and preserves the historical behavior.
pub(crate) fn read_frame(reader: &mut impl BufRead, deadline: Option<Instant>) -> Result<Vec<u8>> {
    let mut frame = Vec::with_capacity(4096);
    let mut poll_delay = IPC_POLL_MIN;
    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            bail!(
                "protocol frame read timed out; the peer did not complete the frame within the I/O deadline"
            );
        }
        let buffer = match reader.fill_buf() {
            Ok(buffer) => {
                poll_delay = IPC_POLL_MIN;
                buffer
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if deadline.is_none() {
                    return Err(error).context("failed to buffer protocol data");
                }
                thread::sleep(poll_delay);
                poll_delay = (poll_delay * 2).min(IPC_POLL_MAX);
                continue;
            }
            Err(error) => return Err(error).context("failed to buffer protocol data"),
        };
        if buffer.is_empty() {
            if frame.is_empty() {
                bail!("protocol peer closed before sending a frame");
            }
            return Ok(frame);
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        if frame.len() + length > MAX_FRAME_BYTES {
            bail!("protocol frame exceeds the 1 MiB limit");
        }
        frame.extend_from_slice(&buffer[..length]);
        reader.consume(length);
        if frame.last() == Some(&b'\n') {
            return Ok(frame);
        }
    }
}

/// Write `data` to the stream within `deadline`, polling with backoff while
/// the peer's receive buffer is full. Non-blocking peers that stop draining
/// produce a bounded write error instead of blocking the caller forever.
///
/// Windows named pipes in nonblocking (PIPE_NOWAIT) mode report a full buffer
/// as `Ok(0)`: a successful zero-byte completion, not a peer close. Such a
/// zero-byte write is retried under the same bounded backoff and deadline.
/// On every other platform `Ok(0)` still means the peer closed the stream.
///
/// There is deliberately no trailing `flush()`: these frames are written
/// directly to the underlying OS handle (never through a userspace
/// `BufWriter`), so there is no buffered data to flush, and a flush on a
/// Windows named pipe (`FlushFileBuffers`) can block until the peer reads —
/// exactly the unbounded wait this function exists to avoid.
fn write_frame_all(stream: &mut impl Write, mut data: &[u8], deadline: Duration) -> Result<()> {
    let deadline = Instant::now() + deadline;
    let mut poll_delay = IPC_POLL_MIN;
    while !data.is_empty() {
        if Instant::now() >= deadline {
            bail!(
                "protocol frame write timed out; the peer did not drain the data within the I/O deadline"
            );
        }
        let stalled = match stream.write(data) {
            #[cfg(windows)]
            Ok(0) => true,
            #[cfg(not(windows))]
            Ok(0) => bail!("protocol peer closed while receiving the frame"),
            Ok(written) => {
                data = &data[written..];
                poll_delay = IPC_POLL_MIN;
                false
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                true
            }
            Err(error) => return Err(error.into()),
        };
        if stalled {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!(
                    "protocol frame write timed out; the peer did not drain the data within the I/O deadline"
                );
            }
            thread::sleep(poll_delay.min(remaining));
            poll_delay = (poll_delay * 2).min(IPC_POLL_MAX);
        }
    }
    Ok(())
}

pub(crate) fn write_response(stream: &mut impl Write, response: &ResponseEnvelope) -> Result<()> {
    write_response_with_deadline(stream, response, IPC_WRITE_DEADLINE)
}

/// Like [`write_response`], but under an explicit total deadline — used for
/// connection-cap rejections written directly from the accept thread, where
/// a slow peer must not stall acceptance of further clients.
pub(crate) fn write_response_with_deadline(
    stream: &mut impl Write,
    response: &ResponseEnvelope,
    deadline: Duration,
) -> Result<()> {
    write_frame_all(stream, &encode_frame(response)?, deadline)
        .context("failed to write runtime response")
}

#[cfg(windows)]
fn start_detached_server() -> Result<()> {
    let executable = env::current_exe().context("failed to locate grok-bridge executable")?;
    let mut application = executable.as_os_str().encode_wide().collect::<Vec<_>>();
    application.push(0);
    let mut command_line = OsString::from(format!("\"{}\" __server", executable.display()))
        .encode_wide()
        .collect::<Vec<_>>();
    command_line.push(0);
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(std::io::Error::last_os_error()).context("failed to spawn runtime server");
    }
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    Ok(())
}

#[cfg(unix)]
fn start_detached_server() -> Result<()> {
    let executable = env::current_exe().context("failed to locate grok-bridge executable")?;
    let mut command = Command::new(executable);
    command
        .arg("__server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and the callback does not access shared state.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    command.spawn().context("failed to spawn runtime server")?;
    Ok(())
}

#[cfg(not(any(windows, unix)))]
fn start_detached_server() -> Result<()> {
    let executable = env::current_exe().context("failed to locate grok-bridge executable")?;
    Command::new(executable)
        .arg("__server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn runtime server")?;
    Ok(())
}

#[cfg(windows)]
fn runtime_identity() -> OsString {
    let user = env::var("USERNAME").unwrap_or_else(|_| "default".to_owned());
    let domain = env::var("USERDOMAIN").unwrap_or_default();
    let suffix = format!("{domain}-{user}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    OsString::from(format!("grok-bridge-runtime-v1-{suffix}"))
}

#[cfg(unix)]
pub(crate) fn runtime_identity() -> OsString {
    let uid = unsafe { libc::getuid() };
    OsString::from(format!("grok-bridge-runtime-v1-u{uid}"))
}

#[cfg(not(any(windows, unix)))]
fn runtime_identity() -> OsString {
    OsString::from("grok-bridge-runtime-v1-default")
}

/// Owner-only mode for the Runtime IPC directory on Unix.
#[cfg(unix)]
pub(crate) const RUNTIME_DIR_MODE: u32 = 0o700;
/// Owner-only mode for the Runtime IPC socket on Unix. Unix sockets authorize
/// connection attempts with the write bits of the socket file, so 0600 admits
/// only the owner.
#[cfg(unix)]
pub(crate) const RUNTIME_SOCKET_MODE: u32 = 0o600;

#[cfg(unix)]
fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

/// Holds the advisory startup lock while one Runtime binds, probes, removes a
/// stale socket, and retries the bind. Closing the file releases the lock.
#[cfg(unix)]
pub(crate) struct RuntimeStartupLock {
    _file: File,
}

#[cfg(unix)]
pub(crate) fn acquire_runtime_startup_lock() -> Result<RuntimeStartupLock> {
    let dir = ensure_runtime_dir()?;
    acquire_runtime_startup_lock_at(&dir, RUNTIME_STARTUP_LOCK_TIMEOUT)
}

#[cfg(unix)]
fn acquire_runtime_startup_lock_at(dir: &Path, timeout: Duration) -> Result<RuntimeStartupLock> {
    let path = dir.join("runtime.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(RUNTIME_SOCKET_MODE)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&path)
        .with_context(|| format!("failed to open the Runtime startup lock {path:?}"))?;
    let meta = file
        .metadata()
        .with_context(|| format!("failed to inspect the Runtime startup lock {path:?}"))?;
    if !meta.is_file() || meta.uid() != current_uid() {
        bail!(
            "Runtime startup lock {path:?} is not a regular file owned by uid {}",
            current_uid()
        );
    }
    file.set_permissions(std::fs::Permissions::from_mode(RUNTIME_SOCKET_MODE))
        .with_context(|| format!("failed to secure the Runtime startup lock {path:?}"))?;

    let deadline = Instant::now() + timeout;
    loop {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(RuntimeStartupLock { _file: file });
        }
        let error = io::Error::last_os_error();
        let raw_error = error.raw_os_error();
        if raw_error != Some(libc::EWOULDBLOCK) && raw_error != Some(libc::EAGAIN) {
            return Err(error)
                .with_context(|| format!("failed to lock Runtime startup file {path:?}"));
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for the Runtime startup lock {path:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Create or verify the private Runtime IPC directory. The directory must be a
/// real directory owned by the current user; a symlink, another file type, or a
/// foreign owner is refused with a diagnosable error instead of being touched.
/// An owned directory is then tightened to owner-only (0700) and the resulting
/// mode is verified on disk.
#[cfg(unix)]
pub(crate) fn ensure_runtime_dir() -> Result<PathBuf> {
    let dir = runtime_dir()?;
    ensure_private_dir(&dir, current_uid())
        .with_context(|| format!("failed to secure the runtime IPC directory {dir:?}"))?;
    Ok(dir)
}

/// Verify that `path` is a directory owned by `expected_uid` and make it
/// owner-only. Uses `symlink_metadata` (lstat) so a symlink at the path is
/// rejected instead of followed. `expected_uid` is a parameter so tests can
/// exercise the ownership check without impersonating another user.
#[cfg(unix)]
fn ensure_private_dir(path: &Path, expected_uid: u32) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                bail!(
                    "{path:?} is a symlink; refusing to use or modify it — remove the link manually"
                );
            }
            if !meta.is_dir() {
                bail!("{path:?} is {:?}, not a directory", meta.file_type());
            }
            if meta.uid() != expected_uid {
                bail!(
                    "{path:?} is owned by uid {} (current user uid {}); refusing to use it",
                    meta.uid(),
                    expected_uid
                );
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(RUNTIME_DIR_MODE);
            builder
                .create(path)
                .with_context(|| format!("failed to create the runtime IPC directory {path:?}"))?;
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {path:?}"));
        }
    }
    set_owner_only_mode(path)
}

/// Apply mode 0700 to a directory already verified as ours and confirm the mode
/// on disk. Chmod by path is safe here: only the owner of a directory can
/// change its permissions, so no other actor can widen them in between.
#[cfg(unix)]
fn set_owner_only_mode(path: &Path) -> Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(RUNTIME_DIR_MODE))
        .with_context(|| format!("failed to set owner-only permissions on {path:?}"))?;
    let mode = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to re-inspect {path:?} after chmod"))?
        .permissions()
        .mode()
        & 0o777;
    if mode != RUNTIME_DIR_MODE {
        bail!("{path:?} has mode {mode:o} after chmod; expected owner-only {RUNTIME_DIR_MODE:o}");
    }
    Ok(())
}

/// Whether the path currently holds a stale socket that may be removed.
/// Returns `Ok(true)` when the path is a socket file owned by `expected_uid`,
/// and `Ok(false)` when nothing is there. A symlink, any other file type, or a
/// foreign owner is refused with a diagnosable error; the path is never
/// followed or removed in those cases.
#[cfg(unix)]
fn stale_socket_safe_to_remove(path: &Path, expected_uid: u32) -> Result<bool> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(|| format!("failed to inspect {path:?}")),
    };
    if meta.file_type().is_symlink() {
        bail!("{path:?} is a symlink; refusing to remove it — delete the link manually and re-run");
    }
    if !meta.file_type().is_socket() {
        bail!(
            "{path:?} is {:?}, not a Unix domain socket; refusing to remove it",
            meta.file_type()
        );
    }
    if meta.uid() != expected_uid {
        bail!(
            "{path:?} is owned by uid {} (current user uid {}); refusing to remove it",
            meta.uid(),
            expected_uid
        );
    }
    Ok(true)
}

/// Remove a stale Runtime socket, but only when it is provably our own socket
/// file: never a symlink, never another file type, never a foreign owner.
/// `Ok(false)` means the path was already gone. Unsafe paths surface as
/// diagnosable errors instead of being deleted.
#[cfg(unix)]
fn remove_stale_socket_at(path: &Path) -> Result<bool> {
    if !stale_socket_safe_to_remove(path, current_uid())? {
        return Ok(false);
    }
    std::fs::remove_file(path)
        .with_context(|| format!("failed to remove the stale runtime socket {path:?}"))?;
    Ok(true)
}

/// Remove the stale socket at the current Runtime IPC path, if one exists and
/// is safe to remove. Called after a bind failure when no live Runtime
/// answered, so a crashed server's leftover socket can be reclaimed.
#[cfg(unix)]
pub(crate) fn remove_stale_runtime_socket(_lock: &RuntimeStartupLock) -> Result<bool> {
    remove_stale_socket_at(&runtime_socket_path()?)
}

/// Apply owner-only (0600) permissions to a freshly bound Runtime socket and
/// verify its on-disk state: a socket file owned by the current user with
/// exactly mode 0600. Chmod by path is safe because the socket lives inside the
/// 0700 owner-only runtime directory, so no other user can replace the file
/// between our syscalls.
#[cfg(unix)]
pub(crate) fn verify_runtime_socket(path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect the runtime socket {path:?}"))?;
    if meta.file_type().is_symlink() {
        bail!("the runtime socket {path:?} is a symlink; refusing to serve it");
    }
    if !meta.file_type().is_socket() {
        bail!(
            "the runtime socket {path:?} is {:?}, not a Unix domain socket",
            meta.file_type()
        );
    }
    if meta.uid() != current_uid() {
        bail!(
            "the runtime socket {path:?} is owned by uid {} (current user uid {}); refusing to serve it",
            meta.uid(),
            current_uid()
        );
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode != RUNTIME_SOCKET_MODE {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(RUNTIME_SOCKET_MODE))
            .with_context(|| {
                format!("failed to set owner-only permissions on the runtime socket {path:?}")
            })?;
        let mode = std::fs::symlink_metadata(path)
            .with_context(|| {
                format!("failed to re-inspect the runtime socket {path:?} after chmod")
            })?
            .permissions()
            .mode()
            & 0o777;
        if mode != RUNTIME_SOCKET_MODE {
            bail!(
                "the runtime socket {path:?} has mode {mode:o} after chmod; expected owner-only {RUNTIME_SOCKET_MODE:o}"
            );
        }
    }
    Ok(())
}

/// Bind a Unix filesystem local socket listener with owner-only permissions.
/// The socket mode is set through interprocess (`fchmod` before `bind`, immune
/// to umask races) when the platform supports it; otherwise the listener is
/// bound first and secured by path afterwards — safe because the socket is
/// created inside the 0700 owner-only runtime directory, which no other user
/// can traverse or modify. The resulting on-disk mode is always verified.
#[cfg(unix)]
pub(crate) fn bind_listener_at(path: &Path) -> io::Result<Listener> {
    use interprocess::os::unix::local_socket::ListenerOptionsExt;
    let name = path.to_fs_name::<GenericFilePath>()?;
    let secure = |listener: Listener| -> io::Result<Listener> {
        verify_runtime_socket(path).map_err(|error| io::Error::other(format!("{error:#}")))?;
        Ok(listener)
    };
    match ListenerOptions::new()
        .name(name.clone())
        .mode(RUNTIME_SOCKET_MODE as libc::mode_t)
        .create_sync()
    {
        Ok(listener) => secure(listener),
        Err(error) if error.kind() == ErrorKind::Unsupported => {
            // The platform cannot fchmod sockets (e.g. macOS): bind first, then
            // apply the mode by path and verify.
            let listener = ListenerOptions::new().name(name).create_sync()?;
            secure(listener)
        }
        Err(error) => Err(error),
    }
}

fn next_request_id() -> String {
    let sequence = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "req-{:x}-{:x}-{sequence:x}",
        std::process::id(),
        now_millis()
    )
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

/// A unique throwaway temp directory, removed on drop. Tests must never leave
/// files behind on failure, and must never collide with the real runtime path.
#[cfg(all(unix, test))]
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[cfg(all(unix, test))]
struct TempDir(PathBuf);

#[cfg(all(unix, test))]
impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "grok-bridge-ipc-test-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(all(unix, test))]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A unique throwaway temp directory under the canonical short `/tmp`, removed
/// on drop (including on assertion failure). Unix socket paths created inside
/// it stay short enough for `sun_path` on every platform — `TempDir` lives
/// under a deep macOS `TMPDIR`, which would overflow the socket address.
#[cfg(all(unix, test))]
static NEXT_SHORT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[cfg(all(unix, test))]
pub(crate) struct ShortTempDir(PathBuf);

#[cfg(all(unix, test))]
impl ShortTempDir {
    pub(crate) fn new(label: &str) -> Self {
        let path = std::fs::canonicalize("/tmp")
            .expect("canonical /tmp must resolve")
            .join(format!(
                "gbt-{label}-{}-{}",
                std::process::id(),
                NEXT_SHORT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(all(unix, test))]
impl Drop for ShortTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::WaitCondition;
    use std::io::{Cursor, Read};

    #[test]
    fn reads_exactly_one_frame() {
        let mut reader = BufReader::new(Cursor::new(b"one\ntwo\n"));
        assert_eq!(read_frame(&mut reader, None).unwrap(), b"one\n");
        assert_eq!(read_frame(&mut reader, None).unwrap(), b"two\n");
    }

    #[test]
    fn rejects_oversized_frames_before_unbounded_growth() {
        let input = vec![b'x'; MAX_FRAME_BYTES + 1];
        let mut reader = BufReader::new(Cursor::new(input));
        assert!(read_frame(&mut reader, None).is_err());
    }

    #[test]
    fn read_frame_with_a_fresh_deadline_accepts_prompt_frames() {
        let mut reader = BufReader::new(Cursor::new(b"one\n"));
        let deadline = Instant::now() + Duration::from_secs(60);
        assert_eq!(read_frame(&mut reader, Some(deadline)).unwrap(), b"one\n");
    }

    #[test]
    fn read_frame_with_an_expired_deadline_fails_before_reading() {
        let mut reader = BufReader::new(Cursor::new(b"one\n"));
        let expired = Instant::now() - Duration::from_secs(1);
        let error = read_frame(&mut reader, Some(expired)).unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
    }

    /// Simulates a PIPE_NOWAIT named pipe: first read is a successful
    /// zero-byte completion (no data yet), later reads return the frame.
    struct EmptyThenFrame {
        sent_empty: bool,
        rest: Cursor<Vec<u8>>,
    }

    impl Read for EmptyThenFrame {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.sent_empty {
                self.sent_empty = true;
                return Ok(0);
            }
            self.rest.read(buf)
        }
    }

    #[test]
    fn nowait_empty_read_without_remap_looks_like_peer_closed() {
        // Documents the actual failure path: BufReader latches Ok(0) as EOF
        // and read_frame reports peer-closed even though a frame follows.
        let inner = EmptyThenFrame {
            sent_empty: false,
            rest: Cursor::new(b"pong\n".to_vec()),
        };
        let mut reader = BufReader::new(inner);
        let deadline = Instant::now() + Duration::from_secs(2);
        let error = read_frame(&mut reader, Some(deadline)).unwrap_err();
        assert!(error.to_string().contains("peer closed"), "{error:#}");
    }

    #[test]
    fn nowait_empty_read_is_not_eof_when_data_follows() {
        let inner = EmptyThenFrame {
            sent_empty: false,
            rest: Cursor::new(b"pong\n".to_vec()),
        };
        let mut reader = BufReader::new(RemapNowaitEmptyRead(inner));
        let deadline = Instant::now() + Duration::from_secs(2);
        assert_eq!(read_frame(&mut reader, Some(deadline)).unwrap(), b"pong\n");
    }

    #[test]
    fn response_deadline_accommodates_documented_long_waits() {
        // The server clamps wait timeouts to 7_200_000 ms and read waits to
        // 300 s; the client deadline must never undercut either.
        let wait = Request::Wait {
            session: "s".to_owned(),
            for_condition: WaitCondition::TuiIdle,
            timeout_ms: Some(7_200_000),
        };
        assert!(
            response_deadline_for(&wait) >= Duration::from_millis(7_200_000) + IPC_RESPONSE_SLACK
        );

        let read = Request::Read {
            session: "s".to_owned(),
            cursor: Some(0),
            limit: Some(1024),
            wait_ms: Some(300_000),
        };
        assert!(
            response_deadline_for(&read) >= Duration::from_millis(300_000) + IPC_RESPONSE_SLACK
        );

        // Ordinary requests still get a bounded default rather than forever.
        let status = Request::ServerStatus;
        assert_eq!(response_deadline_for(&status), IPC_RESPONSE_SLACK);
        assert!(response_deadline_for(&status) < Duration::from_secs(120));
    }

    #[test]
    fn write_response_round_trips_an_envelope() {
        let response = ResponseEnvelope::failure("r1", "boom", "kaput");
        let mut sink = Cursor::new(Vec::new());
        write_response(&mut sink, &response).unwrap();
        let frame = sink.into_inner();
        assert_eq!(decode_response(&frame).unwrap(), response);
    }

    #[test]
    fn read_frame_with_deadline_times_out_for_a_silent_peer() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let _client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        // The peer never sends; the non-blocking read + deadline must give up
        // instead of pinning the thread forever.
        server.set_nonblocking(true).unwrap();
        let mut reader = BufReader::new(server);
        let started = Instant::now();
        let error = read_frame(
            &mut reader,
            Some(Instant::now() + Duration::from_millis(100)),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn write_frame_all_times_out_for_a_peer_that_stops_draining() {
        // Exercise the Runtime's real IPC channel type (AF_UNIX socket on Unix,
        // named pipe on Windows) on a throwaway name — never the user's live
        // runtime path. Its pipe buffers are small and fixed on every platform,
        // so a peer that stops reading genuinely fills the channel — unlike a
        // TCP pair, where delayed ACKs and autotuned buffers keep freeing
        // headroom and can let a small frame slip through, hiding the write
        // deadline.
        #[cfg(unix)]
        let (name, _dir) = {
            let dir = ShortTempDir::new("write-deadline");
            let name = dir
                .path()
                .join("test.sock")
                .to_fs_name::<GenericFilePath>()
                .unwrap();
            (name, dir)
        };
        #[cfg(windows)]
        let name = {
            // A throwaway one-shot pipe name, never the live runtime name:
            // parallel tests must not collide with each other or with a real
            // server, and the pipe is bound only inside this test.
            static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
            format!(
                "grok-bridge-write-deadline-test-{}-{id}",
                std::process::id()
            )
            .to_ns_name::<GenericNamespaced>()
            .unwrap()
        };
        let listener = ListenerOptions::new()
            .name(name.clone())
            .create_sync()
            .unwrap();
        let _client = Stream::connect(name).unwrap();
        let mut server = listener.accept().unwrap();
        // The peer never reads. Fill the pipe until a non-blocking write
        // signals backpressure: a WouldBlock/TimedOut error on either
        // platform, or Ok(0) on Windows PIPE_NOWAIT pipes, which report a
        // successful zero-byte write when the buffer is full. Unix pipes
        // instead accept bytes until the buffer is genuinely full and only
        // then start returning WouldBlock, so `filled` may stay 0 on Windows
        // but is expected to be large on Unix.
        server.set_nonblocking(true).unwrap();
        let filler = vec![0x55u8; 64 * 1024];
        let mut filled = 0usize;
        let pipe_full = loop {
            match server.write(&filler) {
                // Windows named pipes in nonblocking (PIPE_NOWAIT) mode return
                // Ok(0) when the write could not make progress (the pipe
                // buffer is full), with no implication that the peer closed;
                // WriteFileEx reports a successful zero-byte completion. That
                // is the platform's backpressure signal. On Unix, Ok(0) still
                // means the peer half-closed the connection and must not be
                // mistaken for a full buffer.
                #[cfg(windows)]
                Ok(0) => break true,
                #[cfg(not(windows))]
                Ok(0) => panic!("IPC peer closed while filling the send buffer"),
                Ok(written) => filled += written,
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    break true;
                }
                Err(error) => panic!("unexpected fill error: {error}"),
            }
            if filled > 16 * 1024 * 1024 {
                panic!("send buffer never blocked; cannot exercise the write deadline");
            }
        };
        assert!(
            pipe_full,
            "fill loop ended without a backpressure signal (WouldBlock/TimedOut, or Ok(0) on Windows) after {filled} bytes"
        );

        // A full-size frame can never fit in the small filled pipe, so the
        // stalled peer must surface as a bounded timeout instead of a
        // completed write.
        let started = Instant::now();
        let data = vec![0x66u8; MAX_FRAME_BYTES];
        let error = write_frame_all(&mut server, &data, Duration::from_millis(100)).unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn read_frame_waits_for_a_delayed_ipc_response() {
        // Issue #24: a client that polls before the server writes must not
        // treat an empty nowait read as EOF. The server sleeps, then writes
        // a frame; the non-blocking client must still receive it. Throwaway
        // pipe/socket name, never the live runtime endpoint.
        #[cfg(unix)]
        let (name, _dir) = {
            let dir = ShortTempDir::new("delayed-read");
            let name = dir
                .path()
                .join("test.sock")
                .to_fs_name::<GenericFilePath>()
                .unwrap();
            (name, dir)
        };
        #[cfg(windows)]
        let name = {
            static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
            format!("grok-bridge-delayed-read-test-{}-{id}", std::process::id())
                .to_ns_name::<GenericNamespaced>()
                .unwrap()
        };
        let listener = ListenerOptions::new()
            .name(name.clone())
            .create_sync()
            .unwrap();
        let client = Stream::connect(name).unwrap();
        let mut server = listener.accept().unwrap();
        let server_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            server.write_all(b"pong\n").unwrap();
        });
        client.set_nonblocking(true).unwrap();
        #[cfg(windows)]
        let mut reader = BufReader::new(RemapNowaitEmptyRead(client));
        #[cfg(not(windows))]
        let mut reader = BufReader::new(client);
        let deadline = Instant::now() + Duration::from_secs(2);
        assert_eq!(read_frame(&mut reader, Some(deadline)).unwrap(), b"pong\n");
        server_thread.join().unwrap();
    }

    #[test]
    fn read_frame_waits_for_a_delayed_ipc_request() {
        // Issue #27: the server accepts, sets PIPE_NOWAIT, and reads before the
        // client has written. An empty nowait read must not be treated as EOF.
        // Throwaway pipe/socket name, never the live runtime endpoint.
        #[cfg(unix)]
        let (name, _dir) = {
            let dir = ShortTempDir::new("delayed-req");
            let name = dir
                .path()
                .join("test.sock")
                .to_fs_name::<GenericFilePath>()
                .unwrap();
            (name, dir)
        };
        #[cfg(windows)]
        let name = {
            static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
            format!("grok-bridge-delayed-req-test-{}-{id}", std::process::id())
                .to_ns_name::<GenericNamespaced>()
                .unwrap()
        };
        let listener = ListenerOptions::new()
            .name(name.clone())
            .create_sync()
            .unwrap();
        let mut client = Stream::connect(name).unwrap();
        let server = listener.accept().unwrap();
        let client_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            client.write_all(b"ping\n").unwrap();
        });
        server.set_nonblocking(true).unwrap();
        // Production wraps only on Windows. Tests wrap too so Linux CI
        // exercises RemapNowaitEmptyRead on the server-as-reader path.
        let mut reader = BufReader::new(RemapNowaitEmptyRead(server));
        let deadline = Instant::now() + Duration::from_secs(2);
        assert_eq!(read_frame(&mut reader, Some(deadline)).unwrap(), b"ping\n");
        client_thread.join().unwrap();
    }

    #[test]
    fn read_frame_waits_for_a_delayed_long_ipc_request() {
        // Reporter's 600-char `show --session` repro: a delayed long frame
        // must still be read in full. Throwaway name, never the live endpoint.
        #[cfg(unix)]
        let (name, _dir) = {
            let dir = ShortTempDir::new("delayed-long");
            let name = dir
                .path()
                .join("test.sock")
                .to_fs_name::<GenericFilePath>()
                .unwrap();
            (name, dir)
        };
        #[cfg(windows)]
        let name = {
            static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
            format!("grok-bridge-delayed-long-test-{}-{id}", std::process::id())
                .to_ns_name::<GenericNamespaced>()
                .unwrap()
        };
        let listener = ListenerOptions::new()
            .name(name.clone())
            .create_sync()
            .unwrap();
        let mut client = Stream::connect(name).unwrap();
        let server = listener.accept().unwrap();
        let mut payload = vec![b'a'; 600];
        payload.push(b'\n');
        let expected = payload.clone();
        let client_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            client.write_all(&payload).unwrap();
        });
        server.set_nonblocking(true).unwrap();
        let mut reader = BufReader::new(RemapNowaitEmptyRead(server));
        let deadline = Instant::now() + Duration::from_secs(2);
        assert_eq!(read_frame(&mut reader, Some(deadline)).unwrap(), expected);
        client_thread.join().unwrap();
    }

    #[test]
    fn read_frame_fails_fast_when_ipc_peer_disconnects() {
        // Real disconnect must still fail fast (peer closed / broken pipe /
        // EOF), not hang the full 30s request deadline. Unix is not wrapped:
        // Unix Ok(0) is real EOF. Windows keeps the production wrap; disconnect
        // surfaces as a pipe-broken error, not Ok(0).
        #[cfg(unix)]
        let (name, _dir) = {
            let dir = ShortTempDir::new("peer-drop");
            let name = dir
                .path()
                .join("test.sock")
                .to_fs_name::<GenericFilePath>()
                .unwrap();
            (name, dir)
        };
        #[cfg(windows)]
        let name = {
            static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
            format!("grok-bridge-peer-drop-test-{}-{id}", std::process::id())
                .to_ns_name::<GenericNamespaced>()
                .unwrap()
        };
        let listener = ListenerOptions::new()
            .name(name.clone())
            .create_sync()
            .unwrap();
        let client = Stream::connect(name).unwrap();
        let server = listener.accept().unwrap();
        drop(client);
        server.set_nonblocking(true).unwrap();
        #[cfg(windows)]
        let mut reader = BufReader::new(RemapNowaitEmptyRead(server));
        #[cfg(not(windows))]
        let mut reader = BufReader::new(server);
        let started = Instant::now();
        let error =
            read_frame(&mut reader, Some(Instant::now() + Duration::from_secs(2))).unwrap_err();
        let message = error.to_string().to_ascii_lowercase();
        assert!(
            message.contains("peer closed")
                || message.contains("broken pipe")
                || message.contains("not connected")
                || message.contains("eof")
                || message.contains("reset"),
            "{error:#}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "disconnect must fail fast, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn runtime_identity_is_namespaced_and_stable() {
        let first = runtime_identity();
        let second = runtime_identity();
        assert_eq!(first, second);
        assert!(
            first
                .to_string_lossy()
                .starts_with("grok-bridge-runtime-v1-")
        );
    }

    #[test]
    fn codex_thread_identity_precedes_the_legacy_session_identity() {
        assert_eq!(
            client_session_id_from(Some("thread-42".to_owned()), Some("session-7".to_owned()))
                .unwrap()
                .as_deref(),
            Some("thread-42")
        );
        assert_eq!(
            client_session_id_from(None, Some("session-7".to_owned()))
                .unwrap()
                .as_deref(),
            Some("session-7")
        );
        assert_eq!(client_session_id_from(None, None).unwrap(), None);
        assert!(client_session_id_from(Some("bad\nidentity".to_owned()), None).is_err());
    }

    #[test]
    fn connect_first_with_no_candidates_is_a_diagnosable_error_not_a_panic() {
        // Regression: `connect()` filters out candidates whose names fail to
        // construct, so an empty list must never reach `unwrap()` on the last
        // connection error.
        let empty: [Name<'_>; 0] = [];
        let error = connect_first(&empty).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no runtime IPC endpoint candidates"),
            "{error:#}"
        );
    }
}

/// Unix-only tests for owner-only Runtime IPC permissions and safe stale-socket
/// cleanup. Every test works on an explicit throwaway temp directory, never on
/// the user's real runtime path.
#[cfg(all(unix, test))]
mod unix_ipc_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    /// Binds a real Unix domain socket at `path`, leaving the socket file in
    /// place when the listener is dropped.
    fn bind_test_socket(path: &Path) -> std::os::unix::net::UnixListener {
        std::os::unix::net::UnixListener::bind(path).unwrap()
    }

    #[test]
    fn fresh_dir_is_created_owner_only() {
        let temp = TempDir::new("dir-fresh");
        let dir = temp.path().join("runtime");
        ensure_private_dir(&dir, current_uid()).unwrap();
        let meta = std::fs::symlink_metadata(&dir).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.uid(), current_uid());
        assert_eq!(meta.permissions().mode() & 0o777, RUNTIME_DIR_MODE);
    }

    #[test]
    fn existing_owner_dir_is_tightened_to_owner_only() {
        let temp = TempDir::new("dir-tighten");
        let dir = temp.path().join("runtime");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_private_dir(&dir, current_uid()).unwrap();
        let mode = std::fs::symlink_metadata(&dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, RUNTIME_DIR_MODE);
    }

    #[test]
    fn symlink_dir_is_rejected_without_being_followed() {
        let temp = TempDir::new("dir-link");
        let real = temp.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = temp.path().join("runtime");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let error = ensure_private_dir(&link, current_uid()).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
        // The link and its target must be untouched.
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(real.is_dir());
    }

    #[test]
    fn regular_file_dir_is_rejected_without_deletion() {
        let temp = TempDir::new("dir-file");
        let file = temp.path().join("runtime");
        std::fs::write(&file, b"not a dir").unwrap();
        let error = ensure_private_dir(&file, current_uid()).unwrap_err();
        assert!(error.to_string().contains("not a directory"), "{error:#}");
        assert_eq!(std::fs::read(&file).unwrap(), b"not a dir");
    }

    #[test]
    fn foreign_owner_dir_is_rejected() {
        let temp = TempDir::new("dir-owner");
        let dir = temp.path().join("runtime");
        std::fs::create_dir_all(&dir).unwrap();
        // Exercise the ownership check with a uid that is not ours; the real
        // path stays untouched and unrejected.
        let error = ensure_private_dir(&dir, current_uid() + 1).unwrap_err();
        assert!(error.to_string().contains("uid"), "{error:#}");
        assert!(dir.is_dir());
    }

    #[test]
    fn runtime_dir_uses_only_absolute_owner_only_xdg_bases() {
        let temp = TempDir::new("xdg-base");
        let trusted = temp.path().join("trusted");
        std::fs::create_dir_all(&trusted).unwrap();
        std::fs::set_permissions(&trusted, std::fs::Permissions::from_mode(0o700)).unwrap();
        let fallback = temp.path().join("fallback");
        std::fs::create_dir_all(&fallback).unwrap();
        std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o700)).unwrap();
        let trusted_canonical = std::fs::canonicalize(&trusted).unwrap();
        let fallback_canonical = std::fs::canonicalize(&fallback).unwrap();

        assert!(trusted_xdg_runtime_base(&trusted, current_uid()));
        // Trust semantics only: an unbounded length budget keeps the socket
        // path length out of this test (length rules have their own tests).
        assert_eq!(
            runtime_dir_from_with_limit(
                Some(trusted.clone().into_os_string()),
                fallback.clone(),
                current_uid(),
                usize::MAX,
            )
            .unwrap(),
            trusted_canonical.join(runtime_identity())
        );

        assert_eq!(
            runtime_dir_from_with_limit(
                Some(OsString::from("relative/runtime")),
                fallback.clone(),
                current_uid(),
                usize::MAX,
            )
            .unwrap(),
            fallback_canonical.join(runtime_identity())
        );
        assert!(!trusted_xdg_runtime_base(&trusted, current_uid() + 1));

        std::fs::set_permissions(&trusted, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!trusted_xdg_runtime_base(&trusted, current_uid()));
        assert_eq!(
            runtime_dir_from_with_limit(
                Some(trusted.into_os_string()),
                fallback.clone(),
                current_uid(),
                usize::MAX,
            )
            .unwrap(),
            fallback_canonical.join(runtime_identity())
        );
    }

    #[test]
    fn runtime_base_rejects_replaceable_parent_and_accepts_sticky_parent() {
        let temp = TempDir::new("runtime-parent");
        let fallback = temp.path().join("fallback");
        std::fs::create_dir_all(&fallback).unwrap();
        std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o700)).unwrap();

        let shared = temp.path().join("shared");
        let candidate = shared.join("candidate");
        std::fs::create_dir_all(&candidate).unwrap();
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(!trusted_xdg_runtime_base(&candidate, current_uid()));
        assert_eq!(
            runtime_dir_from_with_limit(
                Some(candidate.clone().into_os_string()),
                fallback.clone(),
                current_uid(),
                usize::MAX,
            )
            .unwrap(),
            std::fs::canonicalize(&fallback)
                .unwrap()
                .join(runtime_identity())
        );

        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(trusted_xdg_runtime_base(&candidate, current_uid()));
        assert_eq!(
            runtime_dir_from_with_limit(
                Some(candidate.clone().into_os_string()),
                fallback,
                current_uid(),
                usize::MAX,
            )
            .unwrap(),
            std::fs::canonicalize(candidate)
                .unwrap()
                .join(runtime_identity())
        );
    }

    #[test]
    fn runtime_dir_prefers_a_short_trusted_xdg_base() {
        // 正常路径：可信且足够短的 XDG 基目录直接被采用，socket 路径保持
        // 在保守上限内。基目录建在 `/tmp` 的真实路径上（macOS 上 `/tmp`
        // 是指向 `/private/tmp` 的符号链接，canonicalize 后即真实目录），
        // 这样跨平台都得到一个短的可信基目录。`ShortTempDir` 在 drop 时
        // 清理目录，断言失败也一样。
        let xdg = ShortTempDir::new("xdg-fit");
        std::fs::set_permissions(xdg.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = runtime_dir_from(
            Some(xdg.path().as_os_str().to_os_string()),
            env::temp_dir(),
            current_uid(),
        )
        .unwrap();
        assert_eq!(
            result,
            std::fs::canonicalize(xdg.path())
                .unwrap()
                .join(runtime_identity())
        );
        assert!(
            socket_path_bytes(&result.join("runtime.sock")) <= UNIX_SOCKET_PATH_MAX_BYTES,
            "socket path must stay within the conservative limit"
        );
    }

    #[test]
    fn runtime_dir_rejects_an_overlong_xdg_base_and_falls_back() {
        // 长路径：XDG 基目录可信但让 socket 路径超过保守上限时，它必须被
        // 跳过，回退到下一个可信基目录（temp，必要时 `/tmp`），最终 socket
        // 路径仍在上限内。确定性 fallback 不引入新目录、不扩大 allowed
        // roots：结果始终是某个已通过信任检查的基目录。
        let temp = TempDir::new("xdg-overlong");
        let long_name = "a".repeat(150);
        let xdg = temp.path().join(&long_name);
        std::fs::create_dir_all(&xdg).unwrap();
        std::fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = runtime_dir_from(
            Some(xdg.clone().into_os_string()),
            temp.path().to_path_buf(),
            current_uid(),
        )
        .unwrap();
        assert!(
            socket_path_bytes(&result.join("runtime.sock")) <= UNIX_SOCKET_PATH_MAX_BYTES,
            "fallback socket path must stay within the conservative limit"
        );
        // 拒绝的是 XDG 本身，而不是它的某个子路径。
        assert!(!result.starts_with(std::fs::canonicalize(&xdg).unwrap()));
    }

    #[test]
    fn runtime_dir_selection_is_deterministic_for_identical_inputs() {
        // 稳定性：同一环境两次计算得到完全相同的结果，包括超限回退路径。
        let temp = TempDir::new("xdg-determinism");
        let xdg = temp.path().join("short");
        std::fs::create_dir_all(&xdg).unwrap();
        std::fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o700)).unwrap();
        let first = runtime_dir_from(
            Some(xdg.clone().into_os_string()),
            temp.path().to_path_buf(),
            current_uid(),
        )
        .unwrap();
        let second = runtime_dir_from(
            Some(xdg.into_os_string()),
            temp.path().to_path_buf(),
            current_uid(),
        )
        .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn runtime_startup_lock_serializes_concurrent_starters() {
        let temp = TempDir::new("startup-lock");
        let dir = temp.path().join("runtime");
        ensure_private_dir(&dir, current_uid()).unwrap();
        let first = acquire_runtime_startup_lock_at(&dir, Duration::from_secs(1)).unwrap();
        let acquired = Arc::new(AtomicBool::new(false));
        let acquired_in_thread = Arc::clone(&acquired);
        let thread_dir = dir.clone();
        let waiter = std::thread::spawn(move || {
            let second =
                acquire_runtime_startup_lock_at(&thread_dir, Duration::from_secs(2)).unwrap();
            acquired_in_thread.store(true, Ordering::Release);
            drop(second);
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(!acquired.load(Ordering::Acquire));
        drop(first);
        waiter.join().unwrap();
        assert!(acquired.load(Ordering::Acquire));
    }

    #[test]
    fn stale_socket_symlink_is_refused_without_being_followed() {
        let temp = TempDir::new("sock-link");
        let target = temp.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = temp.path().join("runtime.sock");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = stale_socket_safe_to_remove(&link, current_uid()).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
        // The symlink itself is never followed or removed.
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"x");
    }

    #[test]
    fn stale_socket_wrong_file_type_is_refused_without_deletion() {
        let temp = TempDir::new("sock-file");
        let file = temp.path().join("runtime.sock");
        std::fs::write(&file, b"keep me").unwrap();
        let error = stale_socket_safe_to_remove(&file, current_uid()).unwrap_err();
        assert!(
            error.to_string().contains("not a Unix domain socket"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"keep me");
    }

    #[test]
    fn stale_socket_foreign_owner_is_refused() {
        let temp = TempDir::new("sock-owner");
        let socket = temp.path().join("runtime.sock");
        let listener = bind_test_socket(&socket);
        drop(listener);
        let error = stale_socket_safe_to_remove(&socket, current_uid() + 1).unwrap_err();
        assert!(error.to_string().contains("uid"), "{error:#}");
        assert!(
            std::fs::symlink_metadata(&socket)
                .unwrap()
                .file_type()
                .is_socket()
        );
    }

    #[test]
    fn owned_stale_socket_is_removed() {
        let temp = TempDir::new("sock-remove");
        let socket = temp.path().join("runtime.sock");
        let listener = bind_test_socket(&socket);
        drop(listener);
        assert!(socket.exists());
        assert!(remove_stale_socket_at(&socket).unwrap());
        assert!(!socket.exists());
    }

    #[test]
    fn missing_stale_socket_is_not_an_error() {
        let temp = TempDir::new("sock-missing");
        let socket = temp.path().join("runtime.sock");
        assert!(!remove_stale_socket_at(&socket).unwrap());
    }

    #[test]
    fn bound_socket_is_owner_only_socket_for_current_user() {
        let temp = TempDir::new("sock-mode");
        let path = temp.path().join("runtime.sock");
        let _listener = bind_listener_at(&path).unwrap();
        let meta = std::fs::symlink_metadata(&path).unwrap();
        assert!(meta.file_type().is_socket());
        assert_eq!(meta.uid(), current_uid());
        assert_eq!(meta.permissions().mode() & 0o777, RUNTIME_SOCKET_MODE);
    }

    #[test]
    fn occupied_socket_bind_fails_with_addr_in_use() {
        let temp = TempDir::new("sock-occupied");
        let path = temp.path().join("runtime.sock");
        let _first = bind_listener_at(&path).unwrap();
        let error = bind_listener_at(&path).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AddrInUse);
    }

    #[test]
    fn runtime_socket_path_lives_inside_the_runtime_dir() {
        assert_eq!(
            runtime_socket_path().unwrap(),
            runtime_dir().unwrap().join("runtime.sock")
        );
    }

    /// A throwaway namespaced IPC name, unique per test, never the live
    /// runtime name: parallel tests must not collide with each other or with a
    /// real server.
    fn unique_namespaced_name(label: &str) -> Name<'static> {
        static NEXT_NAMESPACED_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_NAMESPACED_ID.fetch_add(1, Ordering::Relaxed);
        format!(
            "grok-bridge-legacy-test-{label}-{}-{id}",
            std::process::id()
        )
        .to_ns_name::<GenericNamespaced>()
        .unwrap()
    }

    #[test]
    fn legacy_runtime_name_matches_the_old_binaries_namespaced_identity() {
        // Upgrade compatibility: the legacy endpoint must be exactly what an
        // older binary bound — the same per-user identity through the same
        // GenericNamespaced conversion. If either changes, clients can no
        // longer find a Runtime started by the previous version.
        let legacy = legacy_runtime_name().unwrap();
        let expected = runtime_identity()
            .to_ns_name::<GenericNamespaced>()
            .unwrap();
        assert_eq!(legacy, expected);
    }

    #[test]
    fn legacy_namespaced_runtime_is_selected_when_the_filesystem_endpoint_is_absent() {
        // Upgrade compatibility: a Runtime still running from an older binary
        // answers on the legacy GenericNamespaced name. The client must use it
        // instead of auto-starting a second Runtime (which would strand old
        // sessions and contend for the WebUI port). The filesystem candidate is
        // absent; the namespaced one is live, so the first live connection must
        // come from the legacy endpoint.
        let legacy = unique_namespaced_name("legacy-selected");
        let listener = ListenerOptions::new()
            .name(legacy.clone())
            .create_sync()
            .unwrap();
        let missing_fs = unique_short_socket_path("legacy-fs-absent")
            .to_fs_name::<GenericFilePath>()
            .unwrap();
        let stream = connect_first(&[missing_fs, legacy]).unwrap();
        drop(listener);
        drop(stream);
    }

    /// A unique throwaway filesystem socket path short enough for `sun_path`
    /// (TempDir lives under a deep macOS `TMPDIR`, which would overflow the
    /// socket address). Built on the canonical `/tmp` so it is a real, short
    /// directory on every Unix.
    fn unique_short_socket_path(label: &str) -> PathBuf {
        static NEXT_SHORT_SOCKET_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_SHORT_SOCKET_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::fs::canonicalize("/tmp").unwrap();
        base.join(format!("gbt-{label}-{}-{id}.sock", std::process::id()))
    }

    #[test]
    fn legacy_namespaced_runtime_is_not_used_when_the_filesystem_endpoint_is_live() {
        // The current endpoint wins: a live filesystem socket is preferred over
        // the legacy name, so after a clean migration the client never talks to
        // a stale namespaced listener.
        let legacy = unique_namespaced_name("legacy-second");
        let _legacy_listener = ListenerOptions::new()
            .name(legacy.clone())
            .create_sync()
            .unwrap();
        let fs_path = unique_short_socket_path("legacy-fs-live");
        let fs_listener = bind_listener_at(&fs_path).unwrap();
        let fs_name = fs_path.to_fs_name::<GenericFilePath>().unwrap();
        let stream = connect_first(&[fs_name, legacy]).unwrap();
        drop(fs_listener);
        drop(stream);
    }

    #[test]
    fn connect_first_fails_only_when_every_endpoint_is_unavailable() {
        // Auto-start must be the last resort: only when both the filesystem
        // endpoint and the legacy name are dead does the client consider
        // launching a new Runtime.
        let missing_fs = unique_short_socket_path("legacy-none")
            .to_fs_name::<GenericFilePath>()
            .unwrap();
        let missing_legacy = unique_namespaced_name("legacy-none");
        let error = connect_first(&[missing_fs, missing_legacy]).unwrap_err();
        assert!(
            error.to_string().contains("runtime server is not running"),
            "{error:#}"
        );
    }
}
