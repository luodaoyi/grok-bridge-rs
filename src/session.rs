use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
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
    ClientLeaseState, CloseGroupResult, HookActivity, HookEvent, HookEventKind, MAX_WRITE_BYTES,
    ReadResult, SessionPhase, SessionState, TerminalStreamEntry, WaitCondition, WaitResult,
    WebEventsMessage, validate_client_session_id, validate_owner, validate_terminal_size,
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
const DEFAULT_CODEX_LEASE_SECONDS: u64 = 120;
const DEFAULT_ORPHAN_GRACE_SECONDS: u64 = 600;
const MIN_CODEX_LEASE_SECONDS: u64 = 30;
const MAX_CODEX_LEASE_SECONDS: u64 = 24 * 60 * 60;
const MIN_ORPHAN_GRACE_SECONDS: u64 = 60;
const MAX_ORPHAN_GRACE_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy)]
pub(crate) struct OrphanPolicy {
    pub(crate) lease_ms: u64,
    pub(crate) grace_ms: u64,
}

impl OrphanPolicy {
    pub(crate) fn from_env() -> Result<Self> {
        Ok(Self {
            lease_ms: parse_duration_env(
                "GROK_BRIDGE_CODEX_LEASE_SECONDS",
                DEFAULT_CODEX_LEASE_SECONDS,
                MIN_CODEX_LEASE_SECONDS,
                MAX_CODEX_LEASE_SECONDS,
            )?
            .saturating_mul(1_000),
            grace_ms: parse_duration_env(
                "GROK_BRIDGE_ORPHAN_GRACE_SECONDS",
                DEFAULT_ORPHAN_GRACE_SECONDS,
                MIN_ORPHAN_GRACE_SECONDS,
                MAX_ORPHAN_GRACE_SECONDS,
            )?
            .saturating_mul(1_000),
        })
    }
}

/// Global host revision used by the read-only WebUI `/api/events` stream.
/// Every session metadata or terminal change bumps the revision and wakes waiters.
pub(crate) struct HostRevision {
    state: Mutex<u64>,
    changed: Condvar,
}

impl HostRevision {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(0),
            changed: Condvar::new(),
        }
    }

    pub(crate) fn current(&self) -> u64 {
        self.state.lock().map(|guard| *guard).unwrap_or(0)
    }

    pub(crate) fn bump(&self) {
        let Ok(mut revision) = self.state.lock() else {
            return;
        };
        *revision = revision.wrapping_add(1);
        self.changed.notify_all();
    }

    pub(crate) fn wait_for_change(&self, seen: u64, timeout: Duration) -> u64 {
        let Ok(revision) = self.state.lock() else {
            return seen;
        };
        if *revision != seen {
            return *revision;
        }
        let Ok(result) = self.changed.wait_timeout(revision, timeout) else {
            return seen;
        };
        *result.0
    }
}

/// One encoded WebUI events frame plus cursor commits that become durable only
/// after the frame is successfully sent.
#[derive(Debug)]
pub(crate) struct WebEventsFramePlan {
    pub(crate) message: WebEventsMessage,
    /// Exclusive byte cursors to store after this frame is sent.
    pub(crate) cursor_commits: HashMap<String, u64>,
    /// Cursor map keys to drop after this frame is sent (closed sessions).
    pub(crate) cursor_drops: Vec<String>,
}

pub(crate) struct SessionHost {
    registry: Mutex<SessionRegistry>,
    next_id: AtomicU64,
    orphan_policy: OrphanPolicy,
    revision: Arc<HostRevision>,
}

struct SessionRegistry {
    accepting: bool,
    sessions: HashMap<String, Arc<Session>>,
    provider_sessions: HashMap<String, String>,
    clients: HashMap<String, Arc<AtomicU64>>,
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
        let client_session_id = session.client_session_id().ok().flatten();
        self.sessions.remove(handle);
        self.provider_sessions
            .retain(|_, mapped_handle| mapped_handle != handle);
        if let Some(client_session_id) = client_session_id
            && !self
                .sessions
                .values()
                .any(|remaining| remaining.has_client(&client_session_id).unwrap_or(false))
        {
            self.clients.remove(&client_session_id);
        }
        true
    }
}

impl SessionHost {
    pub(crate) fn new(orphan_policy: OrphanPolicy) -> Self {
        Self {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::new(),
                provider_sessions: HashMap::new(),
                clients: HashMap::new(),
            }),
            next_id: AtomicU64::new(1),
            orphan_policy,
            revision: Arc::new(HostRevision::new()),
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision.current()
    }

    pub(crate) fn notify_revision(&self) {
        self.revision.bump();
    }

    pub(crate) fn wait_revision(&self, seen: u64, timeout: Duration) -> u64 {
        self.revision.wait_for_change(seen, timeout)
    }

    /// Earliest *future* wall-clock deadline at which any session's client lease
    /// state would change without another host revision (Connected →
    /// Disconnected/Orphaned). Returns only future deadlines so waiters do not
    /// spin after the transition is already reflected in live list/show state.
    pub(crate) fn next_client_lifecycle_deadline_ms(&self) -> Result<Option<u64>> {
        let now = now_millis();
        let registry = self
            .registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
        let mut next: Option<u64> = None;
        for session in registry.sessions.values() {
            if let Some(deadline) = session.next_lifecycle_deadline_ms(now)? {
                next = Some(match next {
                    Some(current) => current.min(deadline),
                    None => deadline,
                });
            }
        }
        Ok(next)
    }

    pub(crate) fn touch_client(&self, client_session_id: &str) -> Result<()> {
        self.touch_client_at(client_session_id, now_millis())
    }

    fn touch_client_at(&self, client_session_id: &str, now: u64) -> Result<()> {
        validate_client_session_id(client_session_id)?;
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
        registry
            .clients
            .entry(client_session_id.to_owned())
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .store(now, Ordering::Release);
        for session in registry.sessions.values() {
            session.cancel_uncommitted_cleanup_for_client(client_session_id)?;
        }
        drop(registry);
        self.notify_revision();
        Ok(())
    }

    /// Keep every managed session represented by the WebUI's global event
    /// stream leased while at least one WebSocket client remains attached.
    pub(crate) fn touch_web_clients(&self) -> Result<usize> {
        self.touch_web_clients_at(now_millis())
    }

    fn touch_web_clients_at(&self, now: u64) -> Result<usize> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
        let refreshed = registry.clients.len();
        for lease in registry.clients.values() {
            lease.store(now, Ordering::Release);
        }
        for session in registry.sessions.values() {
            session.cancel_uncommitted_orphan_cleanup()?;
        }
        drop(registry);
        if refreshed > 0 {
            self.notify_revision();
        }
        Ok(refreshed)
    }

    pub(crate) fn create(
        &self,
        cwd: &str,
        prompt: Option<String>,
        model: Option<String>,
        owner: Option<String>,
        always_approve: bool,
        client_session_id: Option<String>,
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
        let client_lease = if let Some(client_session_id) = client_session_id.as_deref() {
            validate_client_session_id(client_session_id)?;
            let lease = registry
                .clients
                .entry(client_session_id.to_owned())
                .or_insert_with(|| Arc::new(AtomicU64::new(now_millis())))
                .clone();
            lease.store(now_millis(), Ordering::Release);
            Some(lease)
        } else {
            None
        };
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
                client_session_id,
                client_lease,
                orphan_policy: self.orphan_policy,
            },
            Arc::clone(&self.revision),
        )?;
        let state = session.state()?;
        registry.sessions.insert(handle.clone(), session);
        registry
            .provider_sessions
            .insert(provider_session_id, handle);
        drop(registry);
        self.notify_revision();
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

    /// Plan ordered WebUI events frames without mutating the connection cursor map.
    ///
    /// - `force_reset`: one `reset=true` ANSI snapshot per session (upgrade / resync).
    /// - Otherwise drain raw PTY bytes only up to each session's **frozen**
    ///   `last_cursor` from this batch so a live producer cannot make the plan chase
    ///   forever.
    /// - Terminal entries may span multiple frames under `max_message_bytes`.
    /// - Cursor commits/drops are returned per frame and must be applied only after
    ///   that frame is successfully sent.
    /// - Session metadata in the JSON omits heavy `screen` / `screen_ansi_base64`;
    ///   reset terminal entries remain the authoritative ANSI snapshot.
    pub(crate) fn plan_web_events(
        &self,
        cursors: &HashMap<String, u64>,
        force_reset: bool,
        max_message_bytes: usize,
    ) -> Result<Vec<WebEventsFramePlan>> {
        let sessions = self.list_web()?;
        let active: HashSet<&str> = sessions
            .iter()
            .map(|state| state.session.as_str())
            .collect();
        let cursor_drops: Vec<String> = cursors
            .keys()
            .filter(|session| !active.contains(session.as_str()))
            .cloned()
            .collect();

        let mut terminal_entries: Vec<(TerminalStreamEntry, Option<(String, u64)>)> = Vec::new();
        for state in &sessions {
            if force_reset || !cursors.contains_key(&state.session) {
                terminal_entries.push((
                    TerminalStreamEntry::reset_snapshot(state),
                    Some((state.session.clone(), state.last_cursor)),
                ));
                continue;
            }

            let mut cursor = cursors
                .get(&state.session)
                .copied()
                .unwrap_or(state.last_cursor);
            // Freeze the exclusive end for this batch so continuous output cannot
            // unbounded-chase the live cursor inside one plan call.
            let freeze_end = state.last_cursor;
            if cursor > freeze_end {
                terminal_entries.push((
                    TerminalStreamEntry::reset_snapshot(state),
                    Some((state.session.clone(), freeze_end)),
                ));
                continue;
            }
            if cursor == freeze_end {
                continue;
            }

            while cursor < freeze_end {
                let limit = usize::try_from(freeze_end - cursor)
                    .unwrap_or(MAX_READ_BYTES)
                    .clamp(1, MAX_READ_BYTES);
                let read = match self.read(&state.session, cursor, limit, 0) {
                    Ok(read) => read,
                    Err(_) => {
                        terminal_entries.push((
                            TerminalStreamEntry::reset_snapshot(state),
                            Some((state.session.clone(), freeze_end)),
                        ));
                        break;
                    }
                };
                if read.truncated {
                    terminal_entries.push((
                        TerminalStreamEntry::reset_snapshot(state),
                        Some((state.session.clone(), freeze_end)),
                    ));
                    break;
                }
                if read.next_cursor == read.cursor {
                    break;
                }
                // Never emit past the freeze point even if the live stream advanced.
                let capped_next = read.next_cursor.min(freeze_end);
                if capped_next <= cursor {
                    break;
                }
                let mut entry = TerminalStreamEntry::delta(&read);
                if capped_next != read.next_cursor {
                    // Re-encode a prefix when the live read overshot the freeze.
                    let raw = BASE64.decode(&read.data_base64).unwrap_or_default();
                    let take = (capped_next - read.cursor) as usize;
                    let take = take.min(raw.len());
                    entry.data_base64 = BASE64.encode(&raw[..take]);
                    entry.next_cursor = read.cursor + take as u64;
                }
                cursor = entry.next_cursor;
                terminal_entries.push((entry, Some((state.session.clone(), cursor))));
            }
        }

        let sessions_view: Vec<SessionState> =
            sessions.into_iter().map(web_events_session_view).collect();
        pack_web_events_frames(
            sessions_view,
            terminal_entries,
            cursor_drops,
            max_message_bytes,
        )
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
        drop(registry);
        self.notify_revision();
        Ok(true)
    }

    pub(crate) fn close_owner(&self, owner: &str) -> Result<CloseGroupResult> {
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
        if closed > 0 {
            self.notify_revision();
        }
        Ok(CloseGroupResult {
            matched,
            closed,
            failures,
        })
    }

    pub(crate) fn close_client(&self, client_session_id: &str) -> Result<CloseGroupResult> {
        validate_client_session_id(client_session_id)?;
        let sessions = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            let mut sessions = Vec::new();
            for (handle, session) in &registry.sessions {
                if session.has_client(client_session_id)? {
                    sessions.push((handle.clone(), Arc::clone(session)));
                }
            }
            sessions
        };
        let result = self.close_sessions(sessions)?;
        if result.failures.is_empty() {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            registry.clients.remove(client_session_id);
        }
        // close_sessions already notifies on successful closes; also wake when the
        // client lease map changes even if no sessions matched.
        if result.matched == 0 {
            self.notify_revision();
        }
        Ok(result)
    }

    pub(crate) fn reap_orphans(&self) -> Result<CloseGroupResult> {
        let now = now_millis();
        let candidates = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            let mut sessions = Vec::new();
            for (handle, session) in &registry.sessions {
                if session.claim_orphan_cleanup(now)? {
                    sessions.push((handle.clone(), Arc::clone(session)));
                }
            }
            sessions
        };

        let mut sessions = Vec::new();
        for (handle, session) in candidates {
            let committed = {
                let registry = self
                    .registry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
                let still_registered = registry
                    .sessions
                    .get(&handle)
                    .is_some_and(|current| Arc::ptr_eq(current, &session));
                still_registered && session.commit_orphan_cleanup(now_millis())?
            };
            if committed {
                sessions.push((handle, session));
            }
        }
        if !sessions.is_empty() {
            // Surface ClientLeaseState::Closing only after the final lease and
            // phase recheck commits cleanup.
            self.notify_revision();
        }
        self.close_sessions(sessions)
    }

    fn close_sessions(&self, sessions: Vec<(String, Arc<Session>)>) -> Result<CloseGroupResult> {
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
                Err(error) => {
                    session.reset_orphan_cleanup();
                    failures.push(format!("{handle}: {error:#}"));
                }
            }
        }
        if closed > 0 || matched > 0 {
            // Removal and Closing (cleanup_claimed) transitions must wake WebUI.
            self.notify_revision();
        }
        Ok(CloseGroupResult {
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
        self.notify_revision();
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
    client_session_id: Option<String>,
    client_lease: Option<Arc<AtomicU64>>,
    orphan_policy: OrphanPolicy,
}

struct Session {
    inner: Mutex<SessionInner>,
    changed: Condvar,
    host_revision: Arc<HostRevision>,
    writer_tx: Mutex<Option<SyncSender<Vec<u8>>>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>,
    shutdown: AtomicBool,
    terminating: AtomicBool,
    cleanup_claimed: AtomicBool,
    cleanup_committed: AtomicBool,
}

struct SessionInner {
    session: String,
    owner: Option<String>,
    client_session_id: Option<String>,
    client_lease: Option<Arc<AtomicU64>>,
    orphan_policy: OrphanPolicy,
    phase: SessionPhase,
    phase_changed_at_ms: u64,
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
    turn_done: bool,
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

    fn has_client(&self, client_session_id: &str) -> Result<bool> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?
            .client_session_id
            .as_deref()
            == Some(client_session_id))
    }

    fn client_session_id(&self) -> Result<Option<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?
            .client_session_id
            .clone())
    }

    fn spawn(
        handle: String,
        provider_session_id: &str,
        config: LaunchConfig,
        host_revision: Arc<HostRevision>,
    ) -> Result<Arc<Self>> {
        ensure_grok_state_dir_writable(provider_session_id)?;
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
                client_session_id: config.client_session_id,
                client_lease: config.client_lease,
                orphan_policy: config.orphan_policy,
                phase: SessionPhase::Starting,
                phase_changed_at_ms: now,
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
            host_revision,
            writer_tx: Mutex::new(Some(writer_tx)),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(Some(killer)),
            shutdown: AtomicBool::new(false),
            terminating: AtomicBool::new(false),
            cleanup_claimed: AtomicBool::new(false),
            cleanup_committed: AtomicBool::new(false),
        });

        spawn_reader(Arc::clone(&session), reader);
        spawn_writer(Arc::clone(&session), writer, writer_rx);
        spawn_waiter(Arc::clone(&session), child);
        Ok(session)
    }

    fn signal_changed(&self) {
        self.changed.notify_all();
        self.host_revision.bump();
    }

    fn state(&self) -> Result<SessionState> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        Ok(inner.to_state(now_millis(), self.cleanup_claimed.load(Ordering::Acquire)))
    }

    /// Next pure time-based client lease transition for this session, if any.
    /// Returns only a future deadline so waiters do not spin after the transition
    /// has already been observed via a subsequent list/show.
    fn next_lifecycle_deadline_ms(&self, now: u64) -> Result<Option<u64>> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        Ok(inner.next_lifecycle_deadline_ms(now, self.cleanup_claimed.load(Ordering::Acquire)))
    }

    fn claim_orphan_cleanup(&self, now: u64) -> Result<bool> {
        if self.cleanup_claimed.load(Ordering::Acquire)
            || self.cleanup_committed.load(Ordering::Acquire)
        {
            return Ok(false);
        }
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if !inner.orphan_cleanup_due(now) {
            return Ok(false);
        }
        // Claim while the session lock is still held. Input and phase changes
        // take the same lock, so an idle session cannot become Running between
        // the eligibility check and the claim.
        Ok(self
            .cleanup_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok())
    }

    /// Recheck the lease and phase immediately before cleanup becomes
    /// irreversible. The caller holds the host registry lock, serializing this
    /// commit with every Codex/WebUI lease refresh.
    fn commit_orphan_cleanup(&self, now: u64) -> Result<bool> {
        if !self.cleanup_claimed.load(Ordering::Acquire)
            || self.cleanup_committed.load(Ordering::Acquire)
        {
            return Ok(false);
        }
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if !self.cleanup_claimed.load(Ordering::Acquire) {
            return Ok(false);
        }
        if !inner.orphan_cleanup_due(now) {
            self.cleanup_claimed.store(false, Ordering::Release);
            return Ok(false);
        }
        Ok(self
            .cleanup_committed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok())
    }

    fn cancel_uncommitted_cleanup_for_client(&self, client_session_id: &str) -> Result<bool> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        if inner.client_session_id.as_deref() != Some(client_session_id) {
            return Ok(false);
        }
        Ok(self.cancel_uncommitted_cleanup_locked())
    }

    fn cancel_uncommitted_orphan_cleanup(&self) -> Result<bool> {
        let _inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?;
        Ok(self.cancel_uncommitted_cleanup_locked())
    }

    fn cancel_uncommitted_cleanup_locked(&self) -> bool {
        !self.cleanup_committed.load(Ordering::Acquire)
            && self.cleanup_claimed.swap(false, Ordering::AcqRel)
    }

    fn reset_orphan_cleanup(&self) {
        self.cleanup_committed.store(false, Ordering::Release);
        self.cleanup_claimed.store(false, Ordering::Release);
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

        let now = now_millis();
        let effect = if inner.hook.turn_done
            && !matches!(
                event.kind,
                HookEventKind::SessionStart
                    | HookEventKind::UserPromptSubmit
                    | HookEventKind::Stop
                    | HookEventKind::StopFailure
                    | HookEventKind::SessionEnd
            ) {
            HookEffect::RecordOnly
        } else {
            hook_effect(&event)
        };
        match effect {
            HookEffect::Reset => {
                inner.hook.activity = HookActivity::Unknown;
                inner.hook.tool_name = None;
                inner.hook.waiting_reason = None;
                inner.hook.turn_done = false;
            }
            HookEffect::Working { tool_name } => {
                inner.hook.activity = HookActivity::Working;
                inner.hook.tool_name = tool_name;
                inner.hook.waiting_reason = None;
                if event.kind == HookEventKind::UserPromptSubmit {
                    inner.hook.turn_done = false;
                }
                set_phase(&mut inner, SessionPhase::Running, now);
            }
            HookEffect::Waiting { tool_name, reason } => {
                inner.hook.activity = HookActivity::Waiting;
                inner.hook.tool_name = tool_name;
                inner.hook.waiting_reason = Some(reason);
                set_phase(&mut inner, SessionPhase::Running, now);
            }
            HookEffect::Done => {
                inner.hook.activity = HookActivity::Done;
                inner.hook.tool_name = None;
                inner.hook.waiting_reason = None;
                inner.hook.turn_done = true;
                set_phase(&mut inner, SessionPhase::Idle, now);
            }
            HookEffect::RecordOnly => {}
        }

        inner.hook.last_event = Some(event.kind);
        inner.hook.last_event_at_ms = Some(now);
        inner.updated_at_ms = now;
        drop(inner);
        self.signal_changed();
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
        if self.cleanup_claimed.load(Ordering::Acquire)
            || self.cleanup_committed.load(Ordering::Acquire)
        {
            bail!("session cleanup has started");
        }
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
                let now = now_millis();
                if starts_turn {
                    set_phase(&mut inner, SessionPhase::Running, now);
                    inner.hook.activity = HookActivity::Working;
                    inner.hook.tool_name = None;
                    inner.hook.waiting_reason = None;
                    inner.hook.turn_done = false;
                }
                inner.updated_at_ms = now;
                drop(writer_guard);
                drop(inner);
                self.signal_changed();
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
        if self.cleanup_claimed.load(Ordering::Acquire)
            || self.cleanup_committed.load(Ordering::Acquire)
        {
            bail!("session cleanup has started");
        }
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
        self.signal_changed();
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
        let phase = phase_after_output(
            inner.phase,
            inner.title.as_deref(),
            title_updated,
            inner.hook.activity,
            inner.process_done,
            inner.error.is_some(),
            self.shutdown.load(Ordering::Acquire),
        );
        set_phase(&mut inner, phase, now);
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
        self.signal_changed();
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
        self.signal_changed();
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
        self.signal_changed();
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
        self.signal_changed();
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
        self.signal_changed();
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
        self.signal_changed();
    }
}

impl SessionInner {
    fn to_state(&self, now: u64, cleanup_claimed: bool) -> SessionState {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let (client_state, client_last_seen_at_ms, orphaned_at_ms, auto_close_at_ms) =
            self.client_lifecycle(now, cleanup_claimed);
        SessionState {
            session: self.session.clone(),
            owner: self.owner.clone(),
            client_session_id: self.client_session_id.clone(),
            client_state,
            client_lease_ms: self
                .client_lease
                .as_ref()
                .map(|_| self.orphan_policy.lease_ms),
            orphan_grace_ms: self
                .client_lease
                .as_ref()
                .map(|_| self.orphan_policy.grace_ms),
            client_last_seen_at_ms,
            orphaned_at_ms,
            auto_close_at_ms,
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

    fn next_lifecycle_deadline_ms(&self, now: u64, cleanup_claimed: bool) -> Option<u64> {
        if cleanup_claimed {
            return None;
        }
        let lease = self.client_lease.as_ref()?;
        let last_seen = lease.load(Ordering::Acquire);
        let lease_expires_at = last_seen.saturating_add(self.orphan_policy.lease_ms);
        // Connected while now < lease_expires_at; at equality the state has already
        // flipped. Schedule wake at lease_expires_at so the due send observes the
        // new Disconnected/Orphaned state (not a second Connected snapshot).
        if now < lease_expires_at {
            Some(lease_expires_at)
        } else {
            None
        }
    }

    fn client_lifecycle(
        &self,
        now: u64,
        cleanup_claimed: bool,
    ) -> (ClientLeaseState, Option<u64>, Option<u64>, Option<u64>) {
        let Some(lease) = self.client_lease.as_ref() else {
            return (ClientLeaseState::Unmanaged, None, None, None);
        };
        let last_seen = lease.load(Ordering::Acquire);
        let lease_expires_at = last_seen.saturating_add(self.orphan_policy.lease_ms);
        if cleanup_claimed {
            return (ClientLeaseState::Closing, Some(last_seen), None, None);
        }
        // Inclusive expiry: Connected only strictly before lease_expires_at.
        if now < lease_expires_at {
            return (ClientLeaseState::Connected, Some(last_seen), None, None);
        }
        if !phase_is_safe_for_orphan_cleanup(self.phase) {
            return (ClientLeaseState::Disconnected, Some(last_seen), None, None);
        }
        let orphaned_at = lease_expires_at.max(self.phase_changed_at_ms);
        let auto_close_at = orphaned_at.saturating_add(self.orphan_policy.grace_ms);
        (
            ClientLeaseState::Orphaned,
            Some(last_seen),
            Some(orphaned_at),
            Some(auto_close_at),
        )
    }

    fn orphan_cleanup_due(&self, now: u64) -> bool {
        let (_, _, _, auto_close_at) = self.client_lifecycle(now, false);
        auto_close_at.is_some_and(|deadline| now >= deadline)
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

fn set_phase(inner: &mut SessionInner, phase: SessionPhase, now: u64) {
    if inner.phase != phase {
        inner.phase = phase;
        inner.phase_changed_at_ms = now;
    }
}

/// Strip heavy terminal snapshots from session metadata for `/api/events`.
/// Reset terminal entries carry the authoritative ANSI snapshot.
fn web_events_session_view(mut state: SessionState) -> SessionState {
    state.screen = None;
    state.screen_ansi_base64 = String::new();
    state
}

fn message_json_len(message: &WebEventsMessage) -> usize {
    serde_json::to_vec(message)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

/// One planned terminal payload plus an optional durable cursor commit for its
/// final piece (`session` → exclusive PTY cursor).
type PlannedTerminal = (TerminalStreamEntry, Option<(String, u64)>);

fn pack_web_events_frames(
    sessions_view: Vec<SessionState>,
    terminal_entries: Vec<PlannedTerminal>,
    cursor_drops: Vec<String>,
    max_message_bytes: usize,
) -> Result<Vec<WebEventsFramePlan>> {
    let max_message_bytes = max_message_bytes.max(1);
    let sessions_only = WebEventsMessage::sessions(sessions_view.clone(), Vec::new());
    let sessions_only_len = message_json_len(&sessions_only);
    if sessions_only_len > max_message_bytes {
        bail!(
            "web events sessions metadata exceeds max_message_bytes ({sessions_only_len} > {max_message_bytes})"
        );
    }

    // Expand every terminal entry into pieces that each serialize under the bound
    // when paired alone with sessions metadata.
    let mut expanded: Vec<PlannedTerminal> = Vec::new();
    for (entry, commit) in terminal_entries {
        expanded.extend(split_terminal_entry_to_fit(
            entry,
            commit,
            &sessions_view,
            max_message_bytes,
        )?);
    }

    let mut frames: Vec<WebEventsFramePlan> = Vec::new();
    let mut terminals: Vec<TerminalStreamEntry> = Vec::new();
    let mut commits: HashMap<String, u64> = HashMap::new();
    let mut drops_for_first = cursor_drops;

    let flush = |terminals: &mut Vec<TerminalStreamEntry>,
                 commits: &mut HashMap<String, u64>,
                 drops: &mut Vec<String>,
                 sessions_view: &Vec<SessionState>,
                 frames: &mut Vec<WebEventsFramePlan>| {
        if terminals.is_empty() && commits.is_empty() && drops.is_empty() && !frames.is_empty() {
            return;
        }
        let message = WebEventsMessage::sessions(sessions_view.clone(), std::mem::take(terminals));
        debug_assert!(message_json_len(&message) <= max_message_bytes);
        frames.push(WebEventsFramePlan {
            message,
            cursor_commits: std::mem::take(commits),
            cursor_drops: std::mem::take(drops),
        });
    };

    if expanded.is_empty() {
        frames.push(WebEventsFramePlan {
            message: sessions_only,
            cursor_commits: HashMap::new(),
            cursor_drops: drops_for_first,
        });
        return Ok(frames);
    }

    for (entry, commit) in expanded {
        let mut probe_terminals = terminals.clone();
        probe_terminals.push(entry.clone());
        let probe = WebEventsMessage::sessions(sessions_view.clone(), probe_terminals);
        if !terminals.is_empty() && message_json_len(&probe) > max_message_bytes {
            flush(
                &mut terminals,
                &mut commits,
                &mut drops_for_first,
                &sessions_view,
                &mut frames,
            );
        }
        // After split_terminal_entry_to_fit, a single entry must fit alone.
        let alone = WebEventsMessage::sessions(sessions_view.clone(), vec![entry.clone()]);
        if message_json_len(&alone) > max_message_bytes {
            bail!("web events terminal chunk still exceeds max_message_bytes after split");
        }
        terminals.push(entry);
        if let Some((session, cursor)) = commit {
            commits.insert(session, cursor);
        }
    }

    if !terminals.is_empty()
        || !commits.is_empty()
        || !drops_for_first.is_empty()
        || frames.is_empty()
    {
        flush(
            &mut terminals,
            &mut commits,
            &mut drops_for_first,
            &sessions_view,
            &mut frames,
        );
    }

    if frames.is_empty() {
        frames.push(WebEventsFramePlan {
            message: WebEventsMessage::sessions(sessions_view, Vec::new()),
            cursor_commits: HashMap::new(),
            cursor_drops: Vec::new(),
        });
    }
    for frame in &frames {
        let len = message_json_len(&frame.message);
        if len > max_message_bytes {
            bail!("web events frame exceeds max_message_bytes ({len} > {max_message_bytes})");
        }
    }
    Ok(frames)
}

fn terminal_entry_message_len(
    sessions_view: &[SessionState],
    entry: &TerminalStreamEntry,
) -> usize {
    message_json_len(&WebEventsMessage::sessions(
        sessions_view.to_vec(),
        vec![entry.clone()],
    ))
}

/// Split one terminal entry into ordered pieces that each serialize to
/// `<= max_message_bytes` with the sessions metadata. Reset snapshots: first
/// piece `reset=true`, continuations `reset=false`; PTY cursor commit only on
/// the final piece. Raw deltas preserve byte cursor progression.
fn split_terminal_entry_to_fit(
    entry: TerminalStreamEntry,
    commit: Option<(String, u64)>,
    sessions_view: &[SessionState],
    max_message_bytes: usize,
) -> Result<Vec<PlannedTerminal>> {
    if terminal_entry_message_len(sessions_view, &entry) <= max_message_bytes {
        return Ok(vec![(entry, commit)]);
    }

    let raw = BASE64
        .decode(&entry.data_base64)
        .context("terminal data_base64 is invalid")?;
    if raw.is_empty() {
        bail!("web events terminal entry exceeds max_message_bytes with empty payload");
    }

    let mut pieces: Vec<PlannedTerminal> = Vec::new();
    let mut offset = 0_usize;
    let mut stream_cursor = entry.cursor;
    let original_reset = entry.reset;
    let pty_commit_cursor = entry.next_cursor;

    while offset < raw.len() {
        let remaining = raw.len() - offset;
        // Binary-search the largest raw prefix that still fits in one frame.
        let mut lo = 1_usize;
        let mut hi = remaining;
        let mut best = 0_usize;
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let candidate = TerminalStreamEntry {
                session: entry.session.clone(),
                reset: original_reset && offset == 0,
                cursor: stream_cursor,
                next_cursor: stream_cursor.saturating_add(mid as u64),
                data_base64: BASE64.encode(&raw[offset..offset + mid]),
            };
            if terminal_entry_message_len(sessions_view, &candidate) <= max_message_bytes {
                best = mid;
                lo = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                hi = mid - 1;
            }
        }
        if best == 0 {
            bail!(
                "web events cannot fit any terminal payload bytes within max_message_bytes ({max_message_bytes})"
            );
        }

        let is_last = offset + best >= raw.len();
        let next_cursor = if is_last {
            // Final piece reports the original exclusive end (PTY or snapshot end).
            pty_commit_cursor
        } else {
            stream_cursor.saturating_add(best as u64)
        };
        let piece = TerminalStreamEntry {
            session: entry.session.clone(),
            reset: original_reset && offset == 0,
            cursor: stream_cursor,
            next_cursor,
            data_base64: BASE64.encode(&raw[offset..offset + best]),
        };
        // Durable PTY cursor advances only after the final chunk is sent.
        let piece_commit = if is_last { commit.clone() } else { None };
        pieces.push((piece, piece_commit));
        stream_cursor = next_cursor;
        offset += best;
    }

    Ok(pieces)
}

fn phase_is_safe_for_orphan_cleanup(phase: SessionPhase) -> bool {
    phase == SessionPhase::Idle || phase_is_terminal(phase)
}

fn parse_duration_env(name: &str, default: u64, min: u64, max: u64) -> Result<u64> {
    let Some(value) = env::var_os(name) else {
        return Ok(default);
    };
    let value = value
        .to_str()
        .with_context(|| format!("{name} must be valid Unicode"))?;
    let seconds = value
        .parse::<u64>()
        .with_context(|| format!("{name} must be an integer number of seconds"))?;
    if !(min..=max).contains(&seconds) {
        bail!("{name} must be between {min} and {max} seconds");
    }
    Ok(seconds)
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
        HookEventKind::UserPromptSubmit => HookEffect::Working {
            tool_name: event.tool_name.clone(),
        },
        HookEventKind::PostToolUse
        | HookEventKind::PostToolUseFailure
        | HookEventKind::PermissionDenied
        | HookEventKind::PreCompact
        | HookEventKind::PostCompact => HookEffect::Working { tool_name: None },
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
        HookEventKind::SubagentStart | HookEventKind::SubagentStop => HookEffect::RecordOnly,
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
    let now = now_millis();
    set_phase(inner, phase, now);
    inner.process_id = None;
    inner.updated_at_ms = now;
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
                let now = now_millis();
                set_phase(inner, SessionPhase::Idle, now);
                inner.updated_at_ms = now;
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

fn ensure_grok_state_dir_writable(provider_session_id: &str) -> Result<()> {
    let Some(state_dir) = grok_state_dir() else {
        return Ok(());
    };
    ensure_grok_state_dir_writable_at(&state_dir, provider_session_id)
}

fn grok_state_dir() -> Option<PathBuf> {
    grok_state_dir_from(
        env::var_os("GROK_HOME"),
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
        cfg!(windows),
    )
}

fn grok_state_dir_from(
    grok_home: Option<OsString>,
    home: Option<OsString>,
    user_profile: Option<OsString>,
    windows: bool,
) -> Option<PathBuf> {
    non_empty_path(grok_home).or_else(|| {
        let home = if windows {
            non_empty_path(user_profile).or_else(|| non_empty_path(home))
        } else {
            non_empty_path(home).or_else(|| non_empty_path(user_profile))
        }?;
        Some(home.join(".grok"))
    })
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn ensure_grok_state_dir_writable_at(state_dir: &Path, provider_session_id: &str) -> Result<()> {
    let context = format!(
        "Grok state directory is not writable: {}. The Runtime may have inherited a filesystem sandbox; start grok-bridge server outside that sandbox and retry",
        state_dir.display()
    );
    fs::create_dir_all(state_dir).with_context(|| context.clone())?;
    let probe_path = state_dir.join(format!(".grok-bridge-write-probe-{provider_session_id}"));
    let mut created = false;
    let probe_result = (|| -> std::io::Result<()> {
        let mut probe = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe_path)?;
        created = true;
        probe.write_all(b"grok-bridge")?;
        probe.flush()
    })();
    let cleanup_result = if created {
        match fs::remove_file(&probe_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    } else {
        Ok(())
    };
    probe_result.with_context(|| context.clone())?;
    cleanup_result.with_context(|| {
        format!(
            "failed to remove Grok state probe: {}",
            probe_path.display()
        )
    })?;
    Ok(())
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

    fn temporary_test_directory(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "grok-bridge-{label}-{}-{}",
            std::process::id(),
            generate_provider_session_id().unwrap()
        ))
    }

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
        test_session_with_revision(phase, Arc::new(HostRevision::new()))
    }

    fn test_session_with_revision(
        phase: SessionPhase,
        host_revision: Arc<HostRevision>,
    ) -> Session {
        let cwd = canonical_directory(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (writer_tx, _writer_rx) = sync_channel(1);
        let terminal = phase_is_terminal(phase);
        Session {
            inner: Mutex::new(SessionInner {
                session: "gbt-test".to_owned(),
                owner: Some("test-owner".to_owned()),
                client_session_id: None,
                client_lease: None,
                orphan_policy: OrphanPolicy {
                    lease_ms: 120_000,
                    grace_ms: 600_000,
                },
                phase,
                phase_changed_at_ms: 1,
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
            host_revision,
            writer_tx: Mutex::new(Some(writer_tx)),
            master: Mutex::new(None),
            killer: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            terminating: AtomicBool::new(false),
            cleanup_claimed: AtomicBool::new(false),
            cleanup_committed: AtomicBool::new(false),
        }
    }

    fn test_host(provider_session_id: &str, phase: SessionPhase) -> SessionHost {
        let revision = Arc::new(HostRevision::new());
        let session = Arc::new(test_session_with_revision(phase, Arc::clone(&revision)));
        let handle = session.state().unwrap().session;
        SessionHost {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::from([(handle.clone(), session)]),
                provider_sessions: HashMap::from([(provider_session_id.to_owned(), handle)]),
                clients: HashMap::new(),
            }),
            next_id: AtomicU64::new(1),
            orphan_policy: OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            },
            revision,
        }
    }

    #[test]
    fn resolves_grok_state_directory_with_platform_precedence() {
        assert_eq!(
            grok_state_dir_from(
                Some(OsString::from("/custom/grok")),
                Some(OsString::from("/home/test")),
                Some(OsString::from(r"C:\Users\test")),
                false,
            ),
            Some(PathBuf::from("/custom/grok"))
        );
        assert_eq!(
            grok_state_dir_from(
                None,
                Some(OsString::from("/home/test")),
                Some(OsString::from(r"C:\Users\test")),
                false,
            ),
            Some(PathBuf::from("/home/test").join(".grok"))
        );
        assert_eq!(
            grok_state_dir_from(
                None,
                Some(OsString::from("/home/test")),
                Some(OsString::from(r"C:\Users\test")),
                true,
            ),
            Some(PathBuf::from(r"C:\Users\test").join(".grok"))
        );
        assert_eq!(
            grok_state_dir_from(Some(OsString::new()), None, None, false),
            None
        );
    }

    #[test]
    fn probes_writable_grok_state_directory_and_removes_probe() {
        let root = temporary_test_directory("writable-state");
        let state_dir = root.join("state");
        ensure_grok_state_dir_writable_at(&state_dir, TEST_PROVIDER_SESSION_ID).unwrap();
        assert!(state_dir.is_dir());
        assert!(
            !state_dir
                .join(format!(
                    ".grok-bridge-write-probe-{TEST_PROVIDER_SESSION_ID}"
                ))
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_unwritable_grok_state_directory_with_sandbox_guidance() {
        let root = temporary_test_directory("blocked-state");
        fs::create_dir_all(&root).unwrap();
        let state_dir = root.join("state-file");
        fs::write(&state_dir, b"not a directory").unwrap();
        let error = ensure_grok_state_dir_writable_at(&state_dir, TEST_PROVIDER_SESSION_ID)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Grok state directory is not writable"));
        assert!(error.contains("filesystem sandbox"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn probe_collision_preserves_existing_file() {
        let root = temporary_test_directory("probe-collision");
        fs::create_dir_all(&root).unwrap();
        let probe_path = root.join(format!(
            ".grok-bridge-write-probe-{TEST_PROVIDER_SESSION_ID}"
        ));
        fs::write(&probe_path, b"existing").unwrap();
        assert!(ensure_grok_state_dir_writable_at(&root, TEST_PROVIDER_SESSION_ID).is_err());
        assert_eq!(fs::read(&probe_path).unwrap(), b"existing");
        fs::remove_dir_all(root).unwrap();
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
            client_session_id: None,
            client_lease: None,
            orphan_policy: OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            },
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
            HookEventKind::PermissionDenied,
            HookEventKind::PreCompact,
            HookEventKind::PostCompact,
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
        for kind in [HookEventKind::SubagentStart, HookEventKind::SubagentStop] {
            assert_eq!(hook_effect(&hook_event(kind)), HookEffect::RecordOnly);
        }
    }

    #[test]
    fn completed_turn_ignores_late_tool_events_until_the_next_prompt() {
        let session = test_session(SessionPhase::Running);
        session
            .apply_hook_event(hook_event(HookEventKind::Stop))
            .unwrap();
        let stopped = session.state().unwrap();
        assert_eq!(stopped.phase, SessionPhase::Idle);
        assert_eq!(stopped.activity, HookActivity::Done);

        let mut late = hook_event(HookEventKind::PostToolUse);
        late.tool_name = Some("edit_file".to_owned());
        session.apply_hook_event(late).unwrap();
        let guarded = session.state().unwrap();
        assert_eq!(guarded.phase, SessionPhase::Idle);
        assert_eq!(guarded.activity, HookActivity::Done);
        assert_eq!(guarded.tool_name, None);

        session
            .apply_hook_event(hook_event(HookEventKind::UserPromptSubmit))
            .unwrap();
        let resumed = session.state().unwrap();
        assert_eq!(resumed.phase, SessionPhase::Running);
        assert_eq!(resumed.activity, HookActivity::Working);
    }

    #[test]
    fn lease_cleanup_only_targets_idle_or_terminal_sessions_after_grace() {
        let session = test_session(SessionPhase::Idle);
        let lease = Arc::new(AtomicU64::new(1_000));
        {
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-thread".to_owned());
            inner.client_lease = Some(lease);
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 100,
                grace_ms: 200,
            };
            inner.phase_changed_at_ms = 900;
            assert_eq!(
                inner.client_lifecycle(1_050, false).0,
                ClientLeaseState::Connected
            );
            let lifecycle = inner.client_lifecycle(1_101, false);
            assert_eq!(lifecycle.0, ClientLeaseState::Orphaned);
            assert_eq!(lifecycle.2, Some(1_100));
            assert_eq!(lifecycle.3, Some(1_300));
            assert!(!inner.orphan_cleanup_due(1_299));
            assert!(inner.orphan_cleanup_due(1_300));

            set_phase(&mut inner, SessionPhase::Running, 1_200);
            let running = inner.client_lifecycle(2_000, false);
            assert_eq!(running.0, ClientLeaseState::Disconnected);
            assert_eq!(running.3, None);
            assert!(!inner.orphan_cleanup_due(10_000));
        }
    }

    #[test]
    fn web_keepalive_refreshes_leases_and_cancels_pending_cleanup() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Idle);
        let session = host.get("gbt-test").unwrap();
        let lease = Arc::new(AtomicU64::new(1_000));
        {
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-web".to_owned());
            inner.client_lease = Some(Arc::clone(&lease));
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 100,
                grace_ms: 200,
            };
            inner.phase_changed_at_ms = 900;
        }
        host.registry
            .lock()
            .unwrap()
            .clients
            .insert("codex-web".to_owned(), Arc::clone(&lease));

        assert!(session.claim_orphan_cleanup(1_300).unwrap());
        assert!(session.cleanup_claimed.load(Ordering::Acquire));
        let before = host.revision();
        assert_eq!(host.touch_web_clients_at(1_350).unwrap(), 1);
        assert_eq!(lease.load(Ordering::Acquire), 1_350);
        assert!(!session.cleanup_claimed.load(Ordering::Acquire));
        assert!(!session.cleanup_committed.load(Ordering::Acquire));
        assert_ne!(host.revision(), before);
        assert_eq!(
            session
                .inner
                .lock()
                .unwrap()
                .client_lifecycle(1_400, false)
                .0,
            ClientLeaseState::Connected
        );
    }

    #[test]
    fn final_orphan_commit_rechecks_lease_and_blocks_late_input() {
        let session = test_session(SessionPhase::Idle);
        let lease = Arc::new(AtomicU64::new(1_000));
        {
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-race".to_owned());
            inner.client_lease = Some(Arc::clone(&lease));
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 100,
                grace_ms: 200,
            };
            inner.phase_changed_at_ms = 900;
        }

        assert!(session.claim_orphan_cleanup(1_300).unwrap());
        lease.store(1_300, Ordering::Release);
        assert!(!session.commit_orphan_cleanup(1_300).unwrap());
        assert!(!session.cleanup_claimed.load(Ordering::Acquire));

        assert!(session.claim_orphan_cleanup(1_600).unwrap());
        assert!(session.commit_orphan_cleanup(1_600).unwrap());
        let error = session.write_raw(b"new task\r".to_vec()).unwrap_err();
        assert!(format!("{error:#}").contains("session cleanup has started"));
    }

    #[test]
    fn client_heartbeat_cancels_claim_before_input_is_accepted() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Idle);
        let session = host.get("gbt-test").unwrap();
        let lease = Arc::new(AtomicU64::new(1_000));
        let (writer_tx, writer_rx) = sync_channel(1);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);
        {
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-resume".to_owned());
            inner.client_lease = Some(Arc::clone(&lease));
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 100,
                grace_ms: 200,
            };
            inner.phase_changed_at_ms = 900;
        }
        host.registry
            .lock()
            .unwrap()
            .clients
            .insert("codex-resume".to_owned(), lease);

        assert!(session.claim_orphan_cleanup(1_300).unwrap());
        host.touch_client_at("codex-resume", 1_300).unwrap();
        assert!(!session.cleanup_claimed.load(Ordering::Acquire));
        session.write_raw(b"resume\r".to_vec()).unwrap();
        assert_eq!(writer_rx.recv().unwrap(), b"resume\r");
        assert_eq!(session.state().unwrap().phase, SessionPhase::Running);
    }

    #[test]
    fn orphan_reaper_removes_expired_terminal_sessions_but_keeps_running_ones() {
        let expired = Arc::new(AtomicU64::new(1));
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Exited);
        {
            let session = host.get("gbt-test").unwrap();
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-expired".to_owned());
            inner.client_lease = Some(Arc::clone(&expired));
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 1,
                grace_ms: 1,
            };
            inner.phase_changed_at_ms = 1;
        }
        host.registry
            .lock()
            .unwrap()
            .clients
            .insert("codex-expired".to_owned(), expired);
        let result = host.reap_orphans().unwrap();
        assert_eq!(result.matched, 1);
        assert_eq!(result.closed, 1);
        assert!(result.failures.is_empty());
        assert!(host.list().unwrap().is_empty());

        let running = Arc::new(AtomicU64::new(1));
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        {
            let session = host.get("gbt-test").unwrap();
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-running".to_owned());
            inner.client_lease = Some(Arc::clone(&running));
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 1,
                grace_ms: 1,
            };
            inner.phase_changed_at_ms = 1;
        }
        host.registry
            .lock()
            .unwrap()
            .clients
            .insert("codex-running".to_owned(), running);
        let result = host.reap_orphans().unwrap();
        assert_eq!(result.matched, 0);
        assert_eq!(host.list().unwrap().len(), 1);
        assert_eq!(
            host.show("gbt-test").unwrap().client_state,
            ClientLeaseState::Disconnected
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

    #[test]
    fn host_revision_bumps_on_touch_client_and_waiters_observe_it() {
        let host = SessionHost::new(OrphanPolicy {
            lease_ms: 120_000,
            grace_ms: 600_000,
        });
        let seen = host.revision();
        host.touch_client("codex-thread-42").unwrap();
        assert_ne!(host.revision(), seen);
        let advanced = host.wait_revision(seen, Duration::from_millis(50));
        assert_ne!(advanced, seen);
    }

    #[test]
    fn host_revision_bumps_when_session_output_arrives() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let before = host.revision();
        let session = host.get("gbt-test").unwrap();
        session.append_output(b"hello".to_vec());
        assert_ne!(host.revision(), before);
    }

    fn apply_frame_commits(cursors: &mut HashMap<String, u64>, frames: &[WebEventsFramePlan]) {
        for frame in frames {
            for (session, cursor) in &frame.cursor_commits {
                cursors.insert(session.clone(), *cursor);
            }
            for session in &frame.cursor_drops {
                cursors.remove(session);
            }
        }
    }

    #[test]
    fn web_events_initial_reset_uses_ansi_snapshot_and_last_cursor() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let session = host.get("gbt-test").unwrap();
        session.append_output(b"abc".to_vec());
        let full_ansi = session.state().unwrap().screen_ansi_base64;
        let cursors = HashMap::new();
        let frames = host.plan_web_events(&cursors, true, 1024 * 1024).unwrap();
        assert_eq!(frames.len(), 1);
        let message = &frames[0].message;
        assert_eq!(message.message_type, "sessions");
        assert_eq!(message.sessions.len(), 1);
        assert!(message.sessions[0].screen.is_none());
        assert!(message.sessions[0].screen_ansi_base64.is_empty());
        assert_eq!(message.terminals.len(), 1);
        let entry = &message.terminals[0];
        assert!(entry.reset);
        assert_eq!(entry.cursor, 0);
        assert_eq!(entry.next_cursor, 3);
        assert_eq!(entry.data_base64, full_ansi);
        assert_eq!(frames[0].cursor_commits.get("gbt-test").copied(), Some(3));
    }

    #[test]
    fn web_events_drains_past_64kib_across_bounded_frames() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let session = host.get("gbt-test").unwrap();
        let payload = vec![b'x'; MAX_READ_BYTES + 1_024];
        session.append_output(payload.clone());

        let cursors = HashMap::from([("gbt-test".to_owned(), 0_u64)]);
        // Force multi-frame packing; every produced frame must stay in bound.
        let max_frame = 50_000;
        let frames = host.plan_web_events(&cursors, false, max_frame).unwrap();
        assert!(
            frames.len() >= 2,
            "expected multi-frame drain, got {}",
            frames.len()
        );
        for frame in &frames {
            let encoded = serde_json::to_vec(&frame.message).unwrap();
            assert!(
                encoded.len() <= max_frame,
                "frame len {} exceeds bound {}",
                encoded.len(),
                max_frame
            );
        }

        let mut decoded = Vec::new();
        for frame in &frames {
            for entry in &frame.message.terminals {
                assert!(!entry.reset);
                decoded.extend(
                    BASE64
                        .decode(&entry.data_base64)
                        .expect("terminal delta must be valid base64"),
                );
            }
        }
        assert_eq!(decoded, payload);

        let mut committed = cursors.clone();
        assert_eq!(committed.get("gbt-test").copied(), Some(0));
        apply_frame_commits(&mut committed, &frames);
        assert_eq!(
            committed.get("gbt-test").copied(),
            Some(payload.len() as u64)
        );
    }

    #[test]
    fn web_events_freeze_end_stops_live_cursor_chase() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let session = host.get("gbt-test").unwrap();
        session.append_output(vec![b'a'; 4_096]);
        let cursors = HashMap::from([("gbt-test".to_owned(), 0_u64)]);

        let producer = Arc::clone(&session);
        let running = Arc::new(AtomicBool::new(true));
        let running_flag = Arc::clone(&running);
        let hammer = thread::spawn(move || {
            while running_flag.load(Ordering::Acquire) {
                producer.append_output(vec![b'z'; 8_192]);
                thread::sleep(Duration::from_millis(1));
            }
        });
        // Let the producer run so list/read can observe a moving live cursor.
        thread::sleep(Duration::from_millis(30));
        let frames = host.plan_web_events(&cursors, false, 1024 * 1024).unwrap();
        running.store(false, Ordering::Release);
        hammer.join().unwrap();

        let mut decoded = 0_u64;
        let mut end = 0_u64;
        for frame in &frames {
            for entry in &frame.message.terminals {
                let raw = BASE64.decode(&entry.data_base64).unwrap();
                decoded += raw.len() as u64;
                end = end.max(entry.next_cursor);
            }
        }
        // Batch is finite: committed end equals total decoded and matches freeze commits.
        assert!(decoded > 0);
        assert_eq!(decoded, end);
        let committed = frames
            .iter()
            .filter_map(|frame| frame.cursor_commits.get("gbt-test").copied())
            .max()
            .unwrap();
        assert_eq!(committed, end);
        // Live stream may have advanced further after the frozen batch.
        assert!(session.state().unwrap().last_cursor >= end);
    }

    #[test]
    fn web_events_splits_large_reset_snapshot_with_final_commit_only() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let session = host.get("gbt-test").unwrap();
        // Large screen content so a reset ANSI snapshot exceeds a small bound.
        session.append_output(vec![b'R'; 12_000]);
        let full = session.state().unwrap();
        let full_ansi = BASE64
            .decode(&full.screen_ansi_base64)
            .expect("screen ansi");
        assert!(full_ansi.len() > 1_000);

        let cursors = HashMap::new();
        let max_frame = 2_500;
        let frames = host.plan_web_events(&cursors, true, max_frame).unwrap();
        assert!(
            frames.len() >= 2,
            "expected split reset, got {}",
            frames.len()
        );

        let mut reconstructed = Vec::new();
        let mut saw_reset = false;
        let mut commit_frames = 0_usize;
        for (index, frame) in frames.iter().enumerate() {
            let encoded = serde_json::to_vec(&frame.message).unwrap();
            assert!(
                encoded.len() <= max_frame,
                "frame {index} len {} exceeds bound {max_frame}",
                encoded.len()
            );
            if !frame.cursor_commits.is_empty() {
                commit_frames += 1;
            }
            for entry in &frame.message.terminals {
                if entry.reset {
                    assert!(!saw_reset, "reset must appear only on the first chunk");
                    assert_eq!(index, 0);
                    saw_reset = true;
                } else {
                    assert!(saw_reset, "continuation before reset");
                }
                reconstructed.extend(BASE64.decode(&entry.data_base64).unwrap());
            }
        }
        assert!(saw_reset);
        assert_eq!(reconstructed, full_ansi);
        assert_eq!(
            commit_frames, 1,
            "PTY cursor commits only on the final chunk"
        );
        assert_eq!(
            frames
                .last()
                .and_then(|frame| frame.cursor_commits.get("gbt-test").copied()),
            Some(full.last_cursor)
        );
        // Mid-send failure simulation: durable map stays uncommitted until apply.
        assert!(cursors.is_empty());
        let mut durable = cursors.clone();
        apply_frame_commits(&mut durable, &frames[..frames.len() - 1]);
        assert!(
            !durable.contains_key("gbt-test"),
            "partial send must not commit the PTY cursor"
        );
        apply_frame_commits(&mut durable, &frames[frames.len() - 1..]);
        assert_eq!(durable.get("gbt-test").copied(), Some(full.last_cursor));
    }

    #[test]
    fn web_events_sessions_only_oversize_is_a_planning_error() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let cursors = HashMap::new();
        // Bound smaller than any sessions metadata JSON.
        let err = host
            .plan_web_events(&cursors, false, 8)
            .expect_err("sessions-only oversize must fail planning");
        assert!(
            err.to_string().contains("sessions metadata exceeds"),
            "{err:#}"
        );
    }

    #[test]
    fn web_events_resets_when_client_cursor_is_truncated() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let session = host.get("gbt-test").unwrap();
        // Exceed the bounded transcript so cursor 0 becomes truncated.
        let big = vec![b'y'; MAX_TRANSCRIPT_BYTES + 4_096];
        session.append_output(big);
        let last_cursor = session.state().unwrap().last_cursor;

        let cursors = HashMap::from([("gbt-test".to_owned(), 0_u64)]);
        let frames = host.plan_web_events(&cursors, false, 1024 * 1024).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].message.terminals.len(), 1);
        assert!(frames[0].message.terminals[0].reset);
        assert_eq!(frames[0].message.terminals[0].next_cursor, last_cursor);
        assert_eq!(
            frames[0].cursor_commits.get("gbt-test").copied(),
            Some(last_cursor)
        );
        assert_eq!(cursors.get("gbt-test").copied(), Some(0));
    }

    #[test]
    fn web_events_resets_for_new_sessions_and_drops_closed_cursors() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let cursors = HashMap::from([("stale-session".to_owned(), 9_u64)]);
        let frames = host.plan_web_events(&cursors, false, 1024 * 1024).unwrap();
        assert_eq!(frames[0].message.terminals.len(), 1);
        assert!(frames[0].message.terminals[0].reset);
        assert!(
            frames[0]
                .cursor_drops
                .iter()
                .any(|session| session == "stale-session")
        );
        assert!(frames[0].cursor_commits.contains_key("gbt-test"));
        assert_eq!(cursors.get("stale-session").copied(), Some(9));
    }

    #[test]
    fn lease_deadline_and_client_state_transition_at_expiry() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Idle);
        let lease = Arc::new(AtomicU64::new(1_000));
        {
            let session = host.get("gbt-test").unwrap();
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-thread".to_owned());
            inner.client_lease = Some(Arc::clone(&lease));
            inner.orphan_policy = OrphanPolicy {
                lease_ms: 100,
                grace_ms: 200,
            };
            inner.phase_changed_at_ms = 900;
        }
        // last_seen=1000, lease_ms=100 => lease_expires_at=1100.
        // Connected while now < 1100; at 1100 state is already Orphaned (idle).
        // Never call next_lifecycle_deadline_ms while holding inner (non-reentrant Mutex).
        let session = host.get("gbt-test").unwrap();

        let connected_state = session.inner.lock().unwrap().to_state(1_050, false);
        assert_eq!(connected_state.client_lease_ms, Some(100));
        assert_eq!(connected_state.orphan_grace_ms, Some(200));
        assert_eq!(connected_state.client_state, ClientLeaseState::Connected);

        assert_eq!(
            session
                .inner
                .lock()
                .unwrap()
                .client_lifecycle(1_099, false)
                .0,
            ClientLeaseState::Connected
        );
        assert_eq!(
            session.next_lifecycle_deadline_ms(1_099).unwrap(),
            Some(1_100)
        );

        assert_eq!(
            session
                .inner
                .lock()
                .unwrap()
                .client_lifecycle(1_100, false)
                .0,
            ClientLeaseState::Orphaned
        );
        assert_eq!(session.next_lifecycle_deadline_ms(1_100).unwrap(), None);

        assert_eq!(
            session
                .inner
                .lock()
                .unwrap()
                .client_lifecycle(1_101, false)
                .0,
            ClientLeaseState::Orphaned
        );
        assert_eq!(session.next_lifecycle_deadline_ms(1_101).unwrap(), None);

        // Exactly one due wake at expiry observes the changed state: deadline was
        // scheduled while Connected, and the first moment now >= deadline lists
        // Orphaned with no further pure-time deadline.
        let scheduled = session.next_lifecycle_deadline_ms(1_050).unwrap();
        assert_eq!(scheduled, Some(1_100));
        let due_now = scheduled.unwrap();
        let due_state = session
            .inner
            .lock()
            .unwrap()
            .client_lifecycle(due_now, false)
            .0;
        assert_eq!(due_state, ClientLeaseState::Orphaned);
        assert_eq!(session.next_lifecycle_deadline_ms(due_now).unwrap(), None);
    }
}
