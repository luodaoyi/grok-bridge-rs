use std::{
    env,
    ffi::OsString,
    io::{BufRead, BufReader, Write},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use interprocess::local_socket::{GenericNamespaced, Name, Stream, prelude::*};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
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

fn connect() -> Result<Stream> {
    let name = runtime_name()?;
    Stream::connect(name).context("runtime server is not running")
}

fn call_over_stream(stream: Stream, envelope: &RequestEnvelope) -> Result<ResponseEnvelope> {
    let mut connection = BufReader::new(stream);
    connection
        .get_mut()
        .write_all(&encode_frame(envelope)?)
        .context("failed to write runtime request")?;
    connection
        .get_mut()
        .flush()
        .context("failed to flush runtime request")?;
    let frame = read_frame(&mut connection).context("failed to read runtime response")?;
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

pub(crate) fn runtime_name() -> Result<Name<'static>> {
    let identity = runtime_identity();
    identity
        .to_ns_name::<GenericNamespaced>()
        .context("failed to construct the runtime pipe name")
}

pub(crate) fn read_frame(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut frame = Vec::with_capacity(4096);
    loop {
        let buffer = reader
            .fill_buf()
            .context("failed to buffer protocol data")?;
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

pub(crate) fn write_response(stream: &mut impl Write, response: &ResponseEnvelope) -> Result<()> {
    stream
        .write_all(&encode_frame(response)?)
        .context("failed to write runtime response")?;
    stream.flush().context("failed to flush runtime response")
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
fn runtime_identity() -> OsString {
    let uid = unsafe { libc::getuid() };
    OsString::from(format!("grok-bridge-runtime-v1-u{uid}"))
}

#[cfg(not(any(windows, unix)))]
fn runtime_identity() -> OsString {
    OsString::from("grok-bridge-runtime-v1-default")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_exactly_one_frame() {
        let mut reader = BufReader::new(Cursor::new(b"one\ntwo\n"));
        assert_eq!(read_frame(&mut reader).unwrap(), b"one\n");
        assert_eq!(read_frame(&mut reader).unwrap(), b"two\n");
    }

    #[test]
    fn rejects_oversized_frames_before_unbounded_growth() {
        let input = vec![b'x'; MAX_FRAME_BYTES + 1];
        let mut reader = BufReader::new(Cursor::new(input));
        assert!(read_frame(&mut reader).is_err());
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
}
