use std::{
    collections::{HashMap, VecDeque},
    env,
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::protocol::{
    HookActivity, HookEvent, HookEventKind, MAX_WRITE_BYTES, ReadResult, SessionPhase,
    SessionState, WaitCondition, WaitResult, validate_owner, validate_terminal_size,
};

const INITIAL_COLS: u16 = 120;
const INITIAL_ROWS: u16 = 36;
const SCROLLBACK_ROWS: usize = 5_000;
const MAX_TRANSCRIPT_BYTES: usize = 512 * 1024;
const MAX_READ_BYTES: usize = 64 * 1024;
const WRITER_QUEUE_CAPACITY: usize = 64;
const QUIET_IDLE_MILLISECONDS: u64 = 3_000;
const PROCESS_TERMINATE_TIMEOUT_MS: u32 = 5_000;
const PROVIDER_SESSION_UUID_BYTES: usize = 16;

pub(crate) struct SessionHost {
    registry: Mutex<SessionRegistry>,
    next_id: AtomicU64,
}

pub(crate) struct CloseOwnerResult {
    pub(crate) matched: usize,
    pub(crate) closed: usize,
    pub(crate) failures: Vec<String>,
}

struct SessionRegistry {
    accepting: bool,
    sessions: HashMap<String, Arc<Session>>,
    provider_sessions: HashMap<String, String>,
}

impl SessionRegistry {
    fn remove_session(&mut self, handle: &str, session: &Arc<Session>) -> bool {
        if !self
            .sessions
            .get(handle)
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            return false;
        }
        self.sessions.remove(handle);
        self.provider_sessions
            .retain(|_, mapped_handle| mapped_handle != handle);
        true
    }
}

impl SessionHost {
    pub(crate) fn new() -> Self {
        Self {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::new(),
                provider_sessions: HashMap::new(),
            }),
            next_id: AtomicU64::new(1),
        }
    }

    pub(crate) fn create(
        &self,
        cwd: &str,
        prompt: Option<String>,
        model: Option<String>,
        owner: Option<String>,
        always_approve: bool,
    ) -> Result<SessionState> {
        let cwd = canonical_directory(Path::new(cwd))?;
        ensure_allowed_root(&cwd)?;
        validate_prompt(prompt.as_deref())?;
        validate_model(model.as_deref())?;

        let mut registry = self
            .registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
        if !registry.accepting {
            bail!("runtime server is stopping and no longer accepts new sessions");
        }
        let handle = self.next_handle();
        let provider_session_id = generate_provider_session_id()?;
        if registry
            .provider_sessions
            .contains_key(&provider_session_id)
        {
            bail!("generated a duplicate Grok provider session ID");
        }
        let session = Session::spawn(
            handle.clone(),
            &provider_session_id,
            LaunchConfig {
                grok_bin: env::var_os("GROK_BIN").unwrap_or_else(default_grok_bin),
                cwd,
                prompt,
                model,
                owner,
                always_approve,
            },
        )?;
        let state = session.state()?;
        registry.sessions.insert(handle.clone(), session);
        registry
            .provider_sessions
            .insert(provider_session_id, handle);
        Ok(state)
    }

    pub(crate) fn list(&self) -> Result<Vec<SessionState>> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
        let mut states = registry
            .sessions
            .values()
            .map(|session| session.state())
            .collect::<Result<Vec<_>>>()?;
        states.sort_by_key(|state| state.created_at_ms);
        Ok(states)
    }

    pub(crate) fn list_web(&self) -> Result<Vec<SessionState>> {
        self.list()
    }

    pub(crate) fn show(&self, handle: &str) -> Result<SessionState> {
        self.get(handle)?.state()
    }

    pub(crate) fn read(
        &self,
        handle: &str,
        cursor: u64,
        limit: usize,
        wait_ms: u64,
    ) -> Result<ReadResult> {
        self.get(handle)?.read(cursor, limit, wait_ms)
    }

    pub(crate) fn send(&self, handle: &str, input: String) -> Result<SessionState> {
        let session = self.get(handle)?;
        session.send(input)?;
        session.state()
    }

    pub(crate) fn write_raw(&self, handle: &str, data: Vec<u8>) -> Result<SessionState> {
        let session = self.get(handle)?;
        session.write_raw(data)?;
        session.state()
    }

    pub(crate) fn resize(&self, handle: &str, cols: u16, rows: u16) -> Result<SessionState> {
        let session = self.get(handle)?;
        session.resize(cols, rows)?;
        session.state()
    }

    pub(crate) fn wait(
        &self,
        handle: &str,
        condition: WaitCondition,
        timeout_ms: u64,
    ) -> Result<WaitResult> {
        self.get(handle)?.wait(condition, timeout_ms)
    }

    pub(crate) fn apply_hook_event(
        &self,
        provider_session_id: &str,
        event: HookEvent,
    ) -> Result<bool> {
        let session = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            let Some(handle) = registry.provider_sessions.get(provider_session_id) else {
                return Ok(false);
            };
            let Some(session) = registry.sessions.get(handle) else {
                return Ok(false);
            };
            Arc::clone(session)
        };
        session.apply_hook_event(event)?;
        Ok(true)
    }

    pub(crate) fn close(&self, handle: &str) -> Result<bool> {
        let session = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            registry.sessions.get(handle).cloned()
        };
        let Some(session) = session else {
            bail!("session not found: {handle}");
        };
        session.shutdown()?;
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
        registry.remove_session(handle, &session);
        Ok(true)
    }

    pub(crate) fn close_owner(&self, owner: &str) -> Result<CloseOwnerResult> {
        validate_owner(owner)?;
        let sessions = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            let mut sessions = Vec::new();
            for (handle, session) in &registry.sessions {
                if session.has_owner(owner)? {
                    sessions.push((handle.clone(), Arc::clone(session)));
                }
            }
            sessions
        };

        let matched = sessions.len();
        let mut closed = 0;
        let mut failures = Vec::new();
        for (handle, session) in sessions {
            match session.shutdown() {
                Ok(()) => {
                    closed += 1;
                    let mut registry = self
                        .registry
                        .lock()
                        .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
                    registry.remove_session(&handle, &session);
                }
                Err(error) => failures.push(format!("{handle}: {error:#}")),
            }
        }
        Ok(CloseOwnerResult {
            matched,
            closed,
            failures,
        })
    }

    pub(crate) fn shutdown_all(&self) -> Result<()> {
        let sessions = {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            registry.accepting = false;
            registry
                .sessions
                .iter()
                .map(|(handle, session)| (handle.clone(), Arc::clone(session)))
                .collect::<Vec<_>>()
        };

        let mut errors = Vec::new();
        for (handle, session) in sessions {
            match session.shutdown() {
                Ok(()) => {
                    let mut registry = self
                        .registry
                        .lock()
                        .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
                    registry.remove_session(&handle, &session);
                }
                Err(error) => errors.push(format!("{handle}: {error:#}")),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            bail!("failed to stop one or more sessions: {}", errors.join("; "))
        }
    }

    pub(crate) fn active_count(&self) -> u32 {
        self.list()
            .map(|states| {
                states
                    .iter()
                    .filter(|state| phase_is_active(state.phase))
                    .count() as u32
            })
            .unwrap_or(0)
    }

    fn get(&self, handle: &str) -> Result<Arc<Session>> {
        self.registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?
            .sessions
            .get(handle)
            .cloned()
            .with_context(|| format!("session not found: {handle}"))
    }

    fn next_handle(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("gbt-{:x}-{:x}-{id:x}", std::process::id(), now_millis())
    }
}

struct LaunchConfig {
    grok_bin: OsString,
    cwd: PathBuf,
    prompt: Option<String>,
    model: Option<String>,
    owner: Option<String>,
    always_approve: bool,
}

struct Session {
    inner: Mutex<SessionInner>,
    changed: Condvar,
    writer_tx: Mutex<Option<SyncSender<Vec<u8>>>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>,
    shutdown: AtomicBool,
    terminating: AtomicBool,
}

struct SessionInner {
    session: String,
    owner: Option<String>,
    phase: SessionPhase,
    cwd: String,
    model: Option<String>,
    always_approve: bool,
    process_id: Option<u32>,
    created_at_ms: u64,
    updated_at_ms: u64,
    exit_code: Option<u32>,
    error: Option<String>,
    title: Option<String>,
    parser: vt100::Parser<TitleCallbacks>,
    chunks: VecDeque<OutputChunk>,
    transcript_bytes: usize,
    next_cursor: u64,
    last_output_at_ms: Option<u64>,
    process_done: bool,
    reader_done: bool,
    hook: HookState,
}

struct OutputChunk {
    start: u64,
    data: Vec<u8>,
}

#[derive(Default)]
struct HookState {
    activity: HookActivity,
    last_event: Option<HookEventKind>,
    last_event_at_ms: Option<u64>,
    tool_name: Option<String>,
    waiting_reason: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
enum HookEffect {
    Reset,
    Working {
        tool_name: Option<String>,
    },
    Waiting {
        tool_name: Option<String>,
        reason: String,
    },
    Done,
    RecordOnly,
}

#[derive(Default)]
struct TitleCallbacks {
    title: Option<String>,
    title_updated: bool,
    responses: Vec<Vec<u8>>,
}

impl vt100::Callbacks for TitleCallbacks {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = Some(String::from_utf8_lossy(title).into_owned());
        self.title_updated = true;
    }

    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        first_intermediate: Option<u8>,
        second_intermediate: Option<u8>,
        params: &[&[u16]],
        final_character: char,
    ) {
        if first_intermediate.is_some() || second_intermediate.is_some() {
            return;
        }
        let first_param = params.first().and_then(|value| value.first()).copied();
        match (final_character, first_param) {
            ('n', Some(5)) => self.responses.push(b"\x1b[0n".to_vec()),
            ('n', Some(6)) => {
                let (row, column) = screen.cursor_position();
                self.responses
                    .push(format!("\x1b[{};{}R", row + 1, column + 1).into_bytes());
            }
            ('c', None | Some(0)) => self.responses.push(b"\x1b[?1;2c".to_vec()),
            _ => {}
        }
    }
}

impl Session {
    fn has_owner(&self, owner: &str) -> Result<bool> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?
            .owner
            .as_deref()
            == Some(owner))
    }

    fn spawn(handle: String, provider_session_id: &str, config: LaunchConfig) -> Result<Arc<Self>> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                cols: INITIAL_COLS,
                rows: INITIAL_ROWS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY")?;
        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone the PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take the PTY writer")?;
        let command = build_grok_command(&config, provider_session_id);
        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to start interactive Grok Build")?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let process_id = child
            .process_id()
            .context("Grok did not report a process ID")?;
        let (writer_tx, writer_rx) = sync_channel(WRITER_QUEUE_CAPACITY);
        let now = now_millis();
        let session = Arc::new(Self {
            inner: Mutex::new(SessionInner {
                session: handle,
                owner: config.owner,
                phase: SessionPhase::Starting,
                cwd: config.cwd.to_string_lossy().into_owned(),
                model: config.model,
                always_approve: config.always_approve,
                process_id: Some(process_id),
                created_at_ms: now,
                updated_at_ms: now,
                exit_code: None,
                error: None,
                title: None,
                parser: vt100::Parser::new_with_callbacks(
                    INITIAL_ROWS,
                    INITIAL_COLS,
                    SCROLLBACK_ROWS,
                    TitleCallbacks::default(),
                ),
                chunks: VecDeque::new(),
                transcript_bytes: 0,
                next_cursor: 0,
                last_output_at_ms: None,
                process_done: false,
                reader_done: false,
                hook: HookState::default(),
            }),
            changed: Condvar::new(),
            writer_tx: Mutex::new(Some(writer_tx)),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(Some(killer)),
            shutdown: AtomicBool::new(false),
            terminating: AtomicBool::new(false),
        });

        spawn_reader(Arc::clone(&session), reader);
        spawn_writer(Arc::clone(&session), writer, writer_rx);
        spawn_waiter(Arc::clone(&session), child);
        Ok(session)
    }

    fn state(&self) -> Result<SessionState> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        Ok(inner.to_state())
    }

    fn apply_hook_event(&self, event: HookEvent) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if phase_is_terminal(inner.phase) {
            return Ok(());
        }

        if let Some(cwd) = event.cwd.as_deref() {
            let hook_cwd = canonical_directory(Path::new(cwd))?;
            let session_cwd = normalize_platform_path(PathBuf::from(&inner.cwd));
            if hook_cwd != session_cwd {
                bail!("hook working directory does not match the session");
            }
        }

        match hook_effect(&event) {
            HookEffect::Reset => {
                inner.hook.activity = HookActivity::Unknown;
                inner.hook.tool_name = None;
                inner.hook.waiting_reason = None;
            }
            HookEffect::Working { tool_name } => {
                inner.hook.activity = HookActivity::Working;
                inner.hook.tool_name = tool_name;
                inner.hook.waiting_reason = None;
                inner.phase = SessionPhase::Running;
            }
            HookEffect::Waiting { tool_name, reason } => {
                inner.hook.activity = HookActivity::Waiting;
                inner.hook.tool_name = tool_name;
                inner.hook.waiting_reason = Some(reason);
                inner.phase = SessionPhase::Running;
            }
            HookEffect::Done => {
                inner.hook.activity = HookActivity::Done;
                inner.hook.tool_name = None;
                inner.hook.waiting_reason = None;
                inner.phase = SessionPhase::Idle;
            }
            HookEffect::RecordOnly => {}
        }

        let now = now_millis();
        inner.hook.last_event = Some(event.kind);
        inner.hook.last_event_at_ms = Some(now);
        inner.updated_at_ms = now;
        drop(inner);
        self.changed.notify_all();
        Ok(())
    }

    fn read(&self, cursor: u64, limit: usize, wait_ms: u64) -> Result<ReadResult> {
        let limit = limit.clamp(1, MAX_READ_BYTES);
        let deadline = Instant::now() + Duration::from_millis(wait_ms.min(300_000));
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if cursor > inner.next_cursor {
            bail!(
                "cursor {cursor} is beyond the latest cursor {}",
                inner.next_cursor
            );
        }
        while cursor == inner.next_cursor && phase_is_active(inner.phase) && wait_ms > 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let waited = self
                .changed
                .wait_timeout(inner, remaining)
                .map_err(|_| anyhow::anyhow!("session wait lock was poisoned"))?;
            inner = waited.0;
            if waited.1.timed_out() {
                break;
            }
        }

        let oldest_cursor = inner
            .chunks
            .front()
            .map(|chunk| chunk.start)
            .unwrap_or(inner.next_cursor);
        let actual_cursor = cursor.max(oldest_cursor);
        let mut output = Vec::with_capacity(limit);
        for chunk in &inner.chunks {
            let end = chunk.start + chunk.data.len() as u64;
            if end <= actual_cursor {
                continue;
            }
            let offset = actual_cursor.saturating_sub(chunk.start) as usize;
            let available = &chunk.data[offset.min(chunk.data.len())..];
            let take = available.len().min(limit - output.len());
            output.extend_from_slice(&available[..take]);
            if output.len() == limit {
                break;
            }
        }
        let next_cursor = actual_cursor + output.len() as u64;
        Ok(ReadResult {
            session: inner.session.clone(),
            cursor: actual_cursor,
            next_cursor,
            data_base64: BASE64.encode(&output),
            plain_text: None,
            screen: Some(inner.parser.screen().contents()),
            truncated: cursor < oldest_cursor,
            eof: phase_is_terminal(inner.phase),
        })
    }

    fn send(&self, input: String) -> Result<()> {
        if input.is_empty() {
            bail!("input must not be empty");
        }
        let data = if input.len() == 1 && input.as_bytes()[0].is_ascii_control() {
            input.into_bytes()
        } else {
            let mut data = Vec::with_capacity(input.len() + 13);
            data.extend_from_slice(b"\x1b[200~");
            data.extend_from_slice(input.as_bytes());
            data.extend_from_slice(b"\x1b[201~\r");
            data
        };
        self.enqueue_input(data, true)
    }

    fn write_raw(&self, data: Vec<u8>) -> Result<()> {
        if data.is_empty() {
            bail!("terminal data must not be empty");
        }
        if data.len() > MAX_WRITE_BYTES {
            bail!("terminal data exceeds the 64 KiB limit");
        }
        let starts_turn = raw_input_starts_turn(&data);
        self.enqueue_input(data, starts_turn)
    }

    fn enqueue_input(&self, data: Vec<u8>, starts_turn: bool) -> Result<()> {
        if self.shutdown.load(Ordering::Acquire) {
            bail!("session has already stopped");
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if inner.process_done || phase_is_terminal(inner.phase) || inner.error.is_some() {
            bail!("session is not writable");
        }
        let writer_guard = self
            .writer_tx
            .lock()
            .map_err(|_| anyhow::anyhow!("session input lock was poisoned"))?;
        let Some(writer) = writer_guard.as_ref() else {
            bail!("session input channel is closed");
        };
        match writer.try_send(data) {
            Ok(()) => {
                if starts_turn {
                    inner.phase = SessionPhase::Running;
                    inner.hook.activity = HookActivity::Working;
                    inner.hook.tool_name = None;
                    inner.hook.waiting_reason = None;
                }
                inner.updated_at_ms = now_millis();
                drop(writer_guard);
                drop(inner);
                self.changed.notify_all();
                Ok(())
            }
            Err(TrySendError::Full(_)) => bail!("session input queue is full"),
            Err(TrySendError::Disconnected(_)) => bail!("session input channel is closed"),
        }
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        validate_terminal_size(cols, rows)?;
        if self.shutdown.load(Ordering::Acquire) {
            bail!("session has already stopped");
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if inner.process_done || phase_is_terminal(inner.phase) || inner.error.is_some() {
            bail!("session is not resizable");
        }
        let master_guard = self
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("PTY master lock was poisoned"))?;
        let Some(master) = master_guard.as_ref() else {
            bail!("PTY master is closed");
        };
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to resize PTY")?;
        inner.parser.screen_mut().set_size(rows, cols);
        inner.updated_at_ms = now_millis();
        drop(master_guard);
        drop(inner);
        self.changed.notify_all();
        Ok(())
    }

    fn wait(&self, condition: WaitCondition, timeout_ms: u64) -> Result<WaitResult> {
        let timeout_ms = timeout_ms.clamp(1, 7_200_000);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        loop {
            if condition == WaitCondition::TuiIdle {
                let screen = inner.parser.screen().contents();
                if let Some(reason) = blocked_reason(&screen) {
                    return Ok(inner.wait_result(condition, false, false, Some(reason)));
                }
                if inner.hook.activity == HookActivity::Waiting {
                    let reason = inner
                        .hook
                        .waiting_reason
                        .as_deref()
                        .unwrap_or("grok-hook-waiting");
                    return Ok(inner.wait_result(condition, false, false, Some(reason)));
                }
            }
            if wait_satisfied(&mut inner, condition) {
                return Ok(inner.wait_result(condition, true, false, None));
            }
            if condition == WaitCondition::TuiIdle && phase_is_terminal(inner.phase) {
                return Ok(inner.wait_result(condition, false, false, None));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(inner.wait_result(condition, false, true, None));
            }
            let poll = remaining.min(Duration::from_millis(250));
            let waited = self
                .changed
                .wait_timeout(inner, poll)
                .map_err(|_| anyhow::anyhow!("session wait lock was poisoned"))?;
            inner = waited.0;
        }
    }

    fn shutdown(&self) -> Result<()> {
        self.shutdown.store(true, Ordering::Release);
        self.request_termination()
            .context("failed to terminate Grok")?;
        self.close_writer();
        self.release_master();

        let deadline = Instant::now() + Duration::from_millis(PROCESS_TERMINATE_TIMEOUT_MS.into());
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        while !phase_is_terminal(inner.phase) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("Grok stopped but its PTY output did not close within five seconds");
            }
            let waited = self
                .changed
                .wait_timeout(inner, remaining)
                .map_err(|_| anyhow::anyhow!("session wait lock was poisoned"))?;
            inner = waited.0;
            if waited.1.timed_out() && !phase_is_terminal(inner.phase) {
                bail!("Grok stopped but its PTY output did not close within five seconds");
            }
        }
        Ok(())
    }

    fn append_output(&self, data: Vec<u8>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let now = now_millis();
        let start = inner.next_cursor;
        inner.next_cursor = inner.next_cursor.saturating_add(data.len() as u64);
        inner.transcript_bytes = inner.transcript_bytes.saturating_add(data.len());
        inner.parser.process(&data);
        inner.title = inner.parser.callbacks().title.clone();
        let callbacks = inner.parser.callbacks_mut();
        let title_updated = std::mem::take(&mut callbacks.title_updated);
        let responses = std::mem::take(&mut callbacks.responses);
        inner.phase = phase_after_output(
            inner.phase,
            inner.title.as_deref(),
            title_updated,
            inner.hook.activity,
            inner.process_done,
            inner.error.is_some(),
            self.shutdown.load(Ordering::Acquire),
        );
        inner.last_output_at_ms = Some(now);
        inner.updated_at_ms = now;
        inner.chunks.push_back(OutputChunk { start, data });
        while inner.transcript_bytes > MAX_TRANSCRIPT_BYTES {
            let Some(removed) = inner.chunks.pop_front() else {
                break;
            };
            inner.transcript_bytes = inner.transcript_bytes.saturating_sub(removed.data.len());
        }
        drop(inner);
        for response in responses {
            self.queue_terminal_response(response);
        }
        self.changed.notify_all();
    }

    fn queue_terminal_response(&self, response: Vec<u8>) {
        let result = self
            .writer_tx
            .lock()
            .ok()
            .and_then(|writer| writer.as_ref().map(|writer| writer.try_send(response)));
        match result {
            Some(Ok(())) => {}
            Some(Err(TrySendError::Full(_))) => {
                self.mark_writer_error("terminal response queue is full".to_owned());
            }
            Some(Err(TrySendError::Disconnected(_))) | None => {
                if !self.shutdown.load(Ordering::Acquire) {
                    self.mark_writer_error("terminal response channel is closed".to_owned());
                }
            }
        }
    }

    fn mark_reader_done(&self) {
        let finalized = if let Ok(mut inner) = self.inner.lock() {
            inner.reader_done = true;
            inner.updated_at_ms = now_millis();
            finalize_session(&mut inner, self.shutdown.load(Ordering::Acquire))
        } else {
            false
        };
        self.finish_transition(finalized);
    }

    fn mark_reader_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.reader_done = true;
            record_error(&mut inner, message);
        }
        self.changed.notify_all();
        if let Err(error) = self.request_termination() {
            self.record_secondary_error(format!(
                "failed to terminate Grok after reader error: {error}"
            ));
        }
    }

    fn mark_writer_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            record_error(&mut inner, message);
        }
        self.changed.notify_all();
        if let Err(error) = self.request_termination() {
            self.record_secondary_error(format!(
                "failed to terminate Grok after writer error: {error}"
            ));
        }
    }

    fn mark_wait_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            record_error(&mut inner, message);
        }
        self.changed.notify_all();
        if let Err(error) = self.request_termination() {
            self.record_secondary_error(format!(
                "failed to terminate Grok after wait error: {error}"
            ));
        }
    }

    fn mark_exit(&self, exit_code: u32) {
        let finalized = if let Ok(mut inner) = self.inner.lock() {
            if !inner.process_done {
                inner.process_done = true;
                inner.exit_code = Some(exit_code);
                inner.process_id = None;
            }
            inner.updated_at_ms = now_millis();
            finalize_session(&mut inner, self.shutdown.load(Ordering::Acquire))
        } else {
            false
        };
        self.finish_transition(finalized);
    }

    fn request_termination(&self) -> Result<()> {
        if self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?
            .process_done
        {
            return Ok(());
        }
        if self
            .terminating
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        let result = self
            .killer
            .lock()
            .map_err(|_| anyhow::anyhow!("Grok process killer lock was poisoned"))?
            .as_mut()
            .context("Grok process killer is unavailable")?
            .kill();
        #[cfg(windows)]
        {
            // portable-pty 0.9 inverts the TerminateProcess result in its
            // cloned Windows killer: success is returned as a stale OS error,
            // while failure is returned as Ok. The waiter remains the source
            // of truth for the actual process exit.
            let _ = result;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            match result {
                Ok(()) => Ok(()),
                Err(error) => {
                    self.terminating.store(false, Ordering::Release);
                    Err(error).context("failed to terminate Grok")
                }
            }
        }
    }

    fn record_secondary_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            record_error(&mut inner, message);
        }
        self.changed.notify_all();
    }

    fn close_writer(&self) {
        if let Ok(mut writer) = self.writer_tx.lock() {
            writer.take();
        }
    }

    fn release_master(&self) {
        if let Ok(mut master) = self.master.lock() {
            master.take();
        }
    }

    fn finish_transition(&self, finalized: bool) {
        if finalized {
            self.close_writer();
            self.release_master();
        }
        self.changed.notify_all();
    }
}

impl SessionInner {
    fn to_state(&self) -> SessionState {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        SessionState {
            session: self.session.clone(),
            owner: self.owner.clone(),
            phase: self.phase,
            cwd: self.cwd.clone(),
            model: self.model.clone(),
            always_approve: self.always_approve,
            process_id: self.process_id,
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            exit_code: self.exit_code,
            error: self.error.clone(),
            title: self.title.clone(),
            screen: Some(screen.contents()),
            rows,
            cols,
            screen_ansi_base64: BASE64.encode(screen.contents_formatted()),
            last_cursor: self.next_cursor,
            last_output_at_ms: self.last_output_at_ms,
            activity: self.hook.activity,
            hook_event: self.hook.last_event,
            hook_at_ms: self.hook.last_event_at_ms,
            tool_name: self.hook.tool_name.clone(),
            waiting_reason: self.hook.waiting_reason.clone(),
        }
    }

    fn wait_result(
        &self,
        condition: WaitCondition,
        satisfied: bool,
        timed_out: bool,
        blocked_reason: Option<&str>,
    ) -> WaitResult {
        WaitResult {
            session: self.session.clone(),
            condition,
            satisfied,
            timed_out,
            phase: self.phase,
            exit_code: self.exit_code,
            blocked_reason: blocked_reason.map(str::to_owned),
        }
    }
}

fn generate_provider_session_id() -> Result<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; PROVIDER_SESSION_UUID_BYTES];
    getrandom::fill(&mut bytes).context("failed to generate the Grok provider session ID")?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut session_id = String::with_capacity(36);
    for (index, byte) in bytes.into_iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            session_id.push('-');
        }
        session_id.push(HEX[usize::from(byte >> 4)] as char);
        session_id.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    Ok(session_id)
}

fn hook_effect(event: &HookEvent) -> HookEffect {
    match event.kind {
        HookEventKind::SessionStart => HookEffect::Reset,
        HookEventKind::UserPromptSubmit
        | HookEventKind::PostToolUse
        | HookEventKind::PostToolUseFailure => HookEffect::Working {
            tool_name: event.tool_name.clone(),
        },
        HookEventKind::PreToolUse if is_ask_user_question(event.tool_name.as_deref()) => {
            HookEffect::Waiting {
                tool_name: event.tool_name.clone(),
                reason: event
                    .message
                    .clone()
                    .unwrap_or_else(|| "ask_user_question".to_owned()),
            }
        }
        HookEventKind::PreToolUse => HookEffect::Working {
            tool_name: event.tool_name.clone(),
        },
        HookEventKind::Stop | HookEventKind::StopFailure | HookEventKind::SessionEnd => {
            HookEffect::Done
        }
        HookEventKind::Notification => notification_effect(event),
    }
}

fn is_ask_user_question(tool_name: Option<&str>) -> bool {
    tool_name.is_some_and(|name| name.eq_ignore_ascii_case("ask_user_question"))
}

fn notification_effect(event: &HookEvent) -> HookEffect {
    let notification_type = event
        .notification_type
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if notification_type == "permission_prompt" {
        return HookEffect::RecordOnly;
    }

    let level = event
        .level
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let message = event.message.as_deref().unwrap_or_default();
    let lower_message = message.to_ascii_lowercase();
    let waiting = [
        "permission",
        "question",
        "ask_user_question",
        "elicitation",
        "elicitation_dialog",
    ]
    .iter()
    .any(|value| notification_type == *value || level == *value)
        || ["permission", "approval", "approve", "question"]
            .iter()
            .any(|value| lower_message.contains(value))
        || ["权限", "授权", "批准", "问题", "确认"]
            .iter()
            .any(|value| message.contains(value));
    if waiting {
        let reason = event
            .message
            .clone()
            .or_else(|| event.notification_type.clone())
            .unwrap_or_else(|| "grok-hook-waiting".to_owned());
        return HookEffect::Waiting {
            tool_name: event.tool_name.clone(),
            reason,
        };
    }

    let done = [
        "idle_prompt",
        "input_prompt",
        "input_required",
        "user_input",
        "waiting_for_input",
    ]
    .iter()
    .any(|value| notification_type == *value || level == *value)
        || ["waiting for input", "waiting for your input"]
            .iter()
            .any(|value| lower_message.contains(value))
        || ["请输入", "等待输入", "需要输入"]
            .iter()
            .any(|value| message.contains(value));
    if done {
        HookEffect::Done
    } else {
        HookEffect::RecordOnly
    }
}

fn build_grok_command(config: &LaunchConfig, provider_session_id: &str) -> CommandBuilder {
    let mut command = CommandBuilder::new(&config.grok_bin);
    command.cwd(config.cwd.as_os_str());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.arg("--session-id");
    command.arg(provider_session_id);
    if config.always_approve {
        command.arg("--always-approve");
    }
    if let Some(model) = config.model.as_deref() {
        command.arg("--model");
        command.arg(model);
    }
    if let Some(prompt) = config.prompt.as_deref() {
        command.arg(prompt);
    }
    command
}

fn spawn_reader(session: Arc<Session>, mut reader: Box<dyn Read + Send>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    session.mark_reader_done();
                    return;
                }
                Ok(read) => session.append_output(buffer[..read].to_vec()),
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::BrokenPipe | ErrorKind::UnexpectedEof
                    ) =>
                {
                    session.mark_reader_done();
                    return;
                }
                Err(error) => {
                    session.mark_reader_error(format!("failed to read Grok output: {error}"));
                    return;
                }
            }
        }
    });
}

fn spawn_writer(
    session: Arc<Session>,
    mut writer: Box<dyn Write + Send>,
    writer_rx: std::sync::mpsc::Receiver<Vec<u8>>,
) {
    thread::spawn(move || {
        while let Ok(data) = writer_rx.recv() {
            if let Err(error) = writer.write_all(&data).and_then(|()| writer.flush()) {
                session.mark_writer_error(format!("failed to write Grok input: {error}"));
                return;
            }
        }
    });
}

fn spawn_waiter(session: Arc<Session>, mut child: Box<dyn portable_pty::Child + Send + Sync>) {
    thread::spawn(move || match child.wait() {
        Ok(status) => session.mark_exit(status.exit_code()),
        Err(error) => session.mark_wait_error(format!("failed while waiting for Grok: {error}")),
    });
}

fn finalize_session(inner: &mut SessionInner, shutdown: bool) -> bool {
    let Some(phase) = completed_phase(
        inner.phase,
        inner.process_done,
        inner.reader_done,
        shutdown,
        inner.error.is_some(),
        inner.exit_code,
    ) else {
        return false;
    };
    inner.phase = phase;
    inner.process_id = None;
    inner.updated_at_ms = now_millis();
    true
}

fn completed_phase(
    current: SessionPhase,
    process_done: bool,
    reader_done: bool,
    shutdown: bool,
    failed: bool,
    exit_code: Option<u32>,
) -> Option<SessionPhase> {
    if phase_is_terminal(current) || !process_done || !reader_done {
        return None;
    }
    Some(if shutdown {
        SessionPhase::Stopped
    } else if failed || exit_code != Some(0) {
        SessionPhase::Failed
    } else {
        SessionPhase::Exited
    })
}

fn phase_after_output(
    current: SessionPhase,
    title: Option<&str>,
    title_updated: bool,
    hook_activity: HookActivity,
    process_done: bool,
    failed: bool,
    shutdown: bool,
) -> SessionPhase {
    if phase_is_terminal(current) || process_done || failed || shutdown {
        current
    } else if title_updated && let Some(phase) = phase_from_title(title) {
        phase
    } else if hook_activity == HookActivity::Done {
        SessionPhase::Idle
    } else if hook_activity == HookActivity::Waiting || current == SessionPhase::Starting {
        SessionPhase::Running
    } else {
        current
    }
}

fn record_error(inner: &mut SessionInner, message: String) {
    match &mut inner.error {
        Some(existing) => {
            existing.push_str("; ");
            existing.push_str(&message);
        }
        None => inner.error = Some(message),
    }
    inner.updated_at_ms = now_millis();
}

fn wait_satisfied(inner: &mut SessionInner, condition: WaitCondition) -> bool {
    match condition {
        WaitCondition::Exit => phase_is_terminal(inner.phase),
        WaitCondition::TuiIdle => {
            if inner.error.is_some() {
                return false;
            }
            if inner.phase == SessionPhase::Idle {
                return true;
            }
            let quiet = now_millis().saturating_sub(
                inner
                    .last_output_at_ms
                    .unwrap_or(inner.updated_at_ms)
                    .max(inner.updated_at_ms),
            ) >= QUIET_IDLE_MILLISECONDS;
            if inner.phase == SessionPhase::Running
                && inner.title.is_none()
                && matches!(
                    inner.hook.activity,
                    HookActivity::Unknown | HookActivity::Working
                )
                && quiet
            {
                inner.phase = SessionPhase::Idle;
                inner.updated_at_ms = now_millis();
                return true;
            }
            false
        }
    }
}

fn blocked_reason(screen: &str) -> Option<&'static str> {
    if screen.contains("Run Grok Build in a project directory?") {
        Some("grok-project-directory")
    } else if screen.contains("Type your answer here") || screen.contains("Enter:submit") {
        Some("grok-interactive-prompt")
    } else {
        None
    }
}

fn phase_from_title(title: Option<&str>) -> Option<SessionPhase> {
    let title = title?.trim();
    let lower = title.to_ascii_lowercase();
    if title_has_braille_spinner(title) && (lower.ends_with("grok") || lower.contains(" - grok")) {
        return Some(SessionPhase::Running);
    }
    if lower == "grok" || lower.ends_with(" - grok") {
        return Some(SessionPhase::Idle);
    }
    None
}

fn title_has_braille_spinner(title: &str) -> bool {
    title
        .chars()
        .next()
        .is_some_and(|character| ('\u{2800}'..='\u{28ff}').contains(&character))
}

fn phase_is_active(phase: SessionPhase) -> bool {
    matches!(
        phase,
        SessionPhase::Starting | SessionPhase::Running | SessionPhase::Idle
    )
}

fn phase_is_terminal(phase: SessionPhase) -> bool {
    matches!(
        phase,
        SessionPhase::Exited | SessionPhase::Failed | SessionPhase::Stopped
    )
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let canonical = normalize_platform_path(
        path.canonicalize()
            .with_context(|| format!("failed to resolve working directory: {}", path.display()))?,
    );
    if !canonical.is_dir() {
        bail!(
            "working directory is not a directory: {}",
            canonical.display()
        );
    }
    Ok(canonical)
}

fn ensure_allowed_root(cwd: &Path) -> Result<()> {
    let Some(value) = env::var_os("GROK_BRIDGE_ALLOWED_ROOTS") else {
        return Ok(());
    };
    let mut roots = Vec::new();
    for root in env::split_paths(&value) {
        roots.push(normalize_platform_path(root.canonicalize().with_context(
            || format!("failed to resolve allowed root: {}", root.display()),
        )?));
    }
    if roots.iter().any(|root| cwd.starts_with(root)) {
        Ok(())
    } else {
        bail!(
            "working directory is outside GROK_BRIDGE_ALLOWED_ROOTS: {}",
            cwd.display()
        )
    }
}

#[cfg(windows)]
fn normalize_platform_path(path: PathBuf) -> PathBuf {
    let display = path.to_string_lossy();
    if let Some(rest) = display.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = display.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

#[cfg(not(windows))]
fn normalize_platform_path(path: PathBuf) -> PathBuf {
    path
}

pub(crate) fn default_grok_bin() -> OsString {
    if cfg!(windows) {
        OsString::from("grok.exe")
    } else {
        OsString::from("grok")
    }
}

fn validate_prompt(prompt: Option<&str>) -> Result<()> {
    if let Some(prompt) = prompt {
        if prompt.trim().is_empty() {
            bail!("prompt must not be empty");
        }
        if prompt.len() > 128 * 1024 {
            bail!("prompt exceeds the 128 KiB limit");
        }
    }
    Ok(())
}

fn validate_model(model: Option<&str>) -> Result<()> {
    if let Some(model) = model {
        if model.is_empty() || model.len() > 256 {
            bail!("model must contain between 1 and 256 bytes");
        }
        if !model
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:/".contains(character))
        {
            bail!("model contains unsupported characters");
        }
    }
    Ok(())
}

fn raw_input_starts_turn(data: &[u8]) -> bool {
    data.iter()
        .any(|byte| matches!(*byte, b'\r' | b'\n' | 0x03))
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

    const TEST_PROVIDER_SESSION_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

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

    fn test_session(phase: SessionPhase) -> Session {
        let cwd = canonical_directory(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (writer_tx, _writer_rx) = sync_channel(1);
        let terminal = phase_is_terminal(phase);
        Session {
            inner: Mutex::new(SessionInner {
                session: "gbt-test".to_owned(),
                owner: Some("test-owner".to_owned()),
                phase,
                cwd: cwd.to_string_lossy().into_owned(),
                model: Some("grok-test".to_owned()),
                always_approve: false,
                process_id: (!terminal).then_some(42),
                created_at_ms: 1,
                updated_at_ms: 1,
                exit_code: None,
                error: None,
                title: None,
                parser: vt100::Parser::new_with_callbacks(
                    INITIAL_ROWS,
                    INITIAL_COLS,
                    SCROLLBACK_ROWS,
                    TitleCallbacks::default(),
                ),
                chunks: VecDeque::new(),
                transcript_bytes: 0,
                next_cursor: 0,
                last_output_at_ms: None,
                process_done: terminal,
                reader_done: terminal,
                hook: HookState::default(),
            }),
            changed: Condvar::new(),
            writer_tx: Mutex::new(Some(writer_tx)),
            master: Mutex::new(None),
            killer: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            terminating: AtomicBool::new(false),
        }
    }

    fn test_host(provider_session_id: &str, phase: SessionPhase) -> SessionHost {
        let session = Arc::new(test_session(phase));
        let handle = session.state().unwrap().session;
        SessionHost {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::from([(handle.clone(), session)]),
                provider_sessions: HashMap::from([(provider_session_id.to_owned(), handle)]),
            }),
            next_id: AtomicU64::new(1),
        }
    }

    #[test]
    fn detects_grok_working_and_idle_titles() {
        assert_eq!(
            phase_from_title(Some("⠋ - Waiting for response… - grok")),
            Some(SessionPhase::Running)
        );
        assert_eq!(
            phase_from_title(Some("Fix the auth bug - grok")),
            Some(SessionPhase::Idle)
        );
        assert_eq!(phase_from_title(Some("grok")), Some(SessionPhase::Idle));
        assert_eq!(phase_from_title(Some("PowerShell")), None);
    }

    #[test]
    fn builds_only_interactive_grok_arguments() {
        let config = LaunchConfig {
            grok_bin: OsString::from("grok.exe"),
            cwd: PathBuf::from(r"C:\repo"),
            prompt: Some("修复中文".to_owned()),
            model: Some("grok-4".to_owned()),
            owner: None,
            always_approve: true,
        };
        let command = build_grok_command(&config, TEST_PROVIDER_SESSION_ID);
        assert_eq!(command.get_env("GROK_BRIDGE_SESSION"), None);
        assert_eq!(command.get_env("GROK_BRIDGE_HOOK_TOKEN"), None);
        let argv = command
            .get_argv()
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            argv,
            [
                "grok.exe",
                "--session-id",
                TEST_PROVIDER_SESSION_ID,
                "--always-approve",
                "--model",
                "grok-4",
                "修复中文"
            ]
        );
        assert!(!argv.iter().any(|value| value == "-p"));
        assert!(!argv.iter().any(|value| value == "--output-format"));
    }

    #[test]
    fn publishes_terminal_phase_only_after_process_and_reader_finish() {
        assert_eq!(
            completed_phase(SessionPhase::Running, true, false, false, false, Some(0)),
            None
        );
        assert_eq!(
            completed_phase(SessionPhase::Running, true, true, false, false, Some(0)),
            Some(SessionPhase::Exited)
        );
        assert_eq!(
            completed_phase(SessionPhase::Running, true, true, true, false, Some(1)),
            Some(SessionPhase::Stopped)
        );
    }

    #[test]
    fn generates_random_uuid_v4_provider_session_ids() {
        let first = generate_provider_session_id().unwrap();
        let second = generate_provider_session_id().unwrap();
        assert_eq!(first.len(), 36);
        assert_eq!(&first[8..9], "-");
        assert_eq!(&first[13..14], "-");
        assert_eq!(&first[18..19], "-");
        assert_eq!(&first[23..24], "-");
        assert_eq!(&first[14..15], "4");
        assert!(matches!(&first[19..20], "8" | "9" | "a" | "b"));
        assert!(
            first
                .bytes()
                .all(|byte| { byte.is_ascii_digit() || matches!(byte, b'a'..=b'f' | b'-') })
        );
        assert_ne!(first, second);
    }

    #[test]
    fn maps_hook_lifecycle_and_tool_events_to_activity() {
        let mut event = hook_event(HookEventKind::PreToolUse);
        event.tool_name = Some("read_file".to_owned());
        assert_eq!(
            hook_effect(&event),
            HookEffect::Working {
                tool_name: Some("read_file".to_owned())
            }
        );

        event.tool_name = Some("ASK_USER_QUESTION".to_owned());
        event.message = Some("请选择目标".to_owned());
        assert_eq!(
            hook_effect(&event),
            HookEffect::Waiting {
                tool_name: Some("ASK_USER_QUESTION".to_owned()),
                reason: "请选择目标".to_owned(),
            }
        );

        for kind in [
            HookEventKind::UserPromptSubmit,
            HookEventKind::PostToolUse,
            HookEventKind::PostToolUseFailure,
        ] {
            assert!(matches!(
                hook_effect(&hook_event(kind)),
                HookEffect::Working { .. }
            ));
        }
        for kind in [
            HookEventKind::Stop,
            HookEventKind::StopFailure,
            HookEventKind::SessionEnd,
        ] {
            assert_eq!(hook_effect(&hook_event(kind)), HookEffect::Done);
        }
        assert_eq!(
            hook_effect(&hook_event(HookEventKind::SessionStart)),
            HookEffect::Reset
        );
    }

    #[test]
    fn classifies_notification_events_without_treating_permission_prompt_as_blocked() {
        let mut event = hook_event(HookEventKind::Notification);
        event.notification_type = Some("permission_prompt".to_owned());
        event.message = Some("Approval required".to_owned());
        assert_eq!(hook_effect(&event), HookEffect::RecordOnly);

        event.notification_type = Some("question".to_owned());
        event.message = Some("请选择".to_owned());
        assert_eq!(
            hook_effect(&event),
            HookEffect::Waiting {
                tool_name: None,
                reason: "请选择".to_owned(),
            }
        );

        event.notification_type = Some("input_required".to_owned());
        event.message = None;
        assert_eq!(hook_effect(&event), HookEffect::Done);

        event.notification_type = Some("status".to_owned());
        event.level = Some("info".to_owned());
        assert_eq!(hook_effect(&event), HookEffect::RecordOnly);
    }

    #[test]
    fn applies_hook_state_without_advancing_the_read_cursor() {
        let session = test_session(SessionPhase::Running);
        let cwd = session.state().unwrap().cwd;
        let mut event = hook_event(HookEventKind::PreToolUse);
        event.cwd = Some(cwd);
        event.tool_name = Some("ask_user_question".to_owned());
        event.message = Some("需要选择".to_owned());
        session.apply_hook_event(event).unwrap();

        let web = session.state().unwrap();
        assert_eq!(web.phase, SessionPhase::Running);
        assert_eq!(web.activity, HookActivity::Waiting);
        assert_eq!(web.tool_name.as_deref(), Some("ask_user_question"));
        assert_eq!(web.waiting_reason.as_deref(), Some("需要选择"));
        assert_eq!(web.last_cursor, 0);
        let read = session.read(0, 1, 0).unwrap();
        assert_eq!(read.cursor, 0);
        assert_eq!(read.next_cursor, 0);
        let wait = session.wait(WaitCondition::TuiIdle, 0).unwrap();
        assert!(!wait.satisfied);
        assert!(!wait.timed_out);
        assert_eq!(wait.blocked_reason.as_deref(), Some("需要选择"));

        let serialized = serde_json::to_value(web).unwrap();
        assert_eq!(serialized["session"], "gbt-test");
        assert_eq!(serialized["activity"], "waiting");
        assert!(serialized.get("hook_token").is_none());
        assert!(serialized.get("provider_session_id").is_none());
    }

    #[test]
    fn ignores_late_terminal_hook_events() {
        let terminal = test_session(SessionPhase::Exited);
        let mut late = hook_event(HookEventKind::PreToolUse);
        late.cwd = Some("path-that-does-not-exist".to_owned());
        late.tool_name = Some("ask_user_question".to_owned());
        terminal.apply_hook_event(late).unwrap();
        let web = terminal.state().unwrap();
        assert_eq!(web.phase, SessionPhase::Exited);
        assert_eq!(web.activity, HookActivity::Unknown);
        assert_eq!(web.hook_event, None);
    }

    #[test]
    fn routes_hook_events_by_provider_session_id() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let cwd = host.show("gbt-test").unwrap().cwd;
        let mut event = hook_event(HookEventKind::PreToolUse);
        event.cwd = Some(cwd);
        event.tool_name = Some("ask_user_question".to_owned());
        event.message = Some("需要选择".to_owned());

        assert!(
            host.apply_hook_event(TEST_PROVIDER_SESSION_ID, event)
                .unwrap()
        );
        let web = host.list_web().unwrap().pop().unwrap();
        assert_eq!(web.activity, HookActivity::Waiting);
        assert_eq!(web.tool_name.as_deref(), Some("ask_user_question"));
        assert_eq!(web.waiting_reason.as_deref(), Some("需要选择"));
    }

    #[test]
    fn returns_false_for_unknown_provider_sessions() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        assert!(
            !host
                .apply_hook_event(
                    "00000000-0000-4000-8000-000000000000",
                    hook_event(HookEventKind::Stop)
                )
                .unwrap()
        );
        assert_eq!(host.show("gbt-test").unwrap().phase, SessionPhase::Running);
    }

    #[test]
    fn close_removes_the_provider_session_index() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Exited);
        assert!(host.close("gbt-test").unwrap());
        assert!(
            !host
                .apply_hook_event(TEST_PROVIDER_SESSION_ID, hook_event(HookEventKind::Stop))
                .unwrap()
        );
        let registry = host.registry.lock().unwrap();
        assert!(registry.sessions.is_empty());
        assert!(registry.provider_sessions.is_empty());
    }

    #[test]
    fn hook_done_and_waiting_survive_output_without_an_explicit_grok_title() {
        assert_eq!(
            phase_after_output(
                SessionPhase::Running,
                None,
                false,
                HookActivity::Done,
                false,
                false,
                false,
            ),
            SessionPhase::Idle
        );
        assert_eq!(
            phase_after_output(
                SessionPhase::Running,
                None,
                false,
                HookActivity::Waiting,
                false,
                false,
                false,
            ),
            SessionPhase::Running
        );
        assert_eq!(
            phase_after_output(
                SessionPhase::Idle,
                Some("⠋ - Waiting for response… - grok"),
                true,
                HookActivity::Done,
                false,
                false,
                false,
            ),
            SessionPhase::Running
        );
        assert_eq!(
            phase_after_output(
                SessionPhase::Running,
                Some("grok"),
                true,
                HookActivity::Waiting,
                false,
                false,
                false,
            ),
            SessionPhase::Idle
        );
    }

    #[test]
    fn quiet_fallback_recovers_from_a_missing_completion_hook() {
        let session = test_session(SessionPhase::Running);
        let mut inner = session.inner.lock().unwrap();
        inner.updated_at_ms = now_millis().saturating_sub(QUIET_IDLE_MILLISECONDS + 1);
        inner.hook.activity = HookActivity::Working;
        assert!(wait_satisfied(&mut inner, WaitCondition::TuiIdle));
        assert_eq!(inner.phase, SessionPhase::Idle);

        inner.phase = SessionPhase::Running;
        inner.updated_at_ms = now_millis().saturating_sub(QUIET_IDLE_MILLISECONDS + 1);
        inner.hook.activity = HookActivity::Waiting;
        assert!(!wait_satisfied(&mut inner, WaitCondition::TuiIdle));
        assert_eq!(inner.phase, SessionPhase::Running);
    }

    #[test]
    fn late_output_does_not_revive_a_finished_process() {
        assert_eq!(
            phase_after_output(
                SessionPhase::Exited,
                Some("grok"),
                true,
                HookActivity::Done,
                true,
                false,
                false,
            ),
            SessionPhase::Exited
        );
        assert_eq!(
            phase_after_output(
                SessionPhase::Running,
                Some("grok"),
                false,
                HookActivity::Unknown,
                false,
                false,
                false
            ),
            SessionPhase::Running
        );
        assert_eq!(
            phase_after_output(
                SessionPhase::Running,
                Some("grok"),
                true,
                HookActivity::Unknown,
                false,
                false,
                false
            ),
            SessionPhase::Idle
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalizes_windows_verbatim_paths_for_child_processes() {
        assert_eq!(
            normalize_platform_path(PathBuf::from(r"\\?\D:\repo\project")),
            PathBuf::from(r"D:\repo\project")
        );
        assert_eq!(
            normalize_platform_path(PathBuf::from(r"\\?\UNC\server\share\repo")),
            PathBuf::from(r"\\server\share\repo")
        );
    }

    #[test]
    fn detects_interactive_grok_prompts_as_blocked() {
        assert_eq!(
            blocked_reason("Run Grok Build in a project directory?"),
            Some("grok-project-directory")
        );
        assert_eq!(
            blocked_reason("Type your answer here  Enter:submit"),
            Some("grok-interactive-prompt")
        );
        assert_eq!(blocked_reason("中文通讯正常"), None);
    }

    #[test]
    fn raw_navigation_does_not_mark_a_turn_running() {
        assert!(!raw_input_starts_turn(b"hello"));
        assert!(!raw_input_starts_turn(b"\x1b[A"));
        assert!(raw_input_starts_turn(b"hello\r"));
        assert!(raw_input_starts_turn(&[0x03]));
    }
}
