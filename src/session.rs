use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
    ffi::OsString,
    fmt,
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{RecvTimeoutError, SyncSender, TrySendError, channel, sync_channel},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

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
/// Total pending writer bytes budget (≈ four maximum-size writes). Admission is
/// bounded by both entry count and bytes so a full queue always rejects the
/// whole input instead of accepting a partial write.
const WRITER_QUEUE_MAX_BYTES: usize = 4 * MAX_WRITE_BYTES;
const QUIET_IDLE_MILLISECONDS: u64 = 3_000;
const PROCESS_TERMINATE_TIMEOUT_MS: u32 = 5_000;
/// One absolute deadline shared by every session in a group close. The WebUI
/// request timeout (8 s) exceeds this by a small response margin.
const GROUP_CLOSE_TIMEOUT_MS: u64 = 6_000;
/// How long the group close collector may wait for a worker past the shared
/// deadline before declaring the remaining sessions timed out.
const GROUP_CLOSE_RESPONSE_MARGIN_MS: u64 = 1_000;
/// Fixed concurrency bound for group close, independent of group size.
const GROUP_CLOSE_WORKERS: usize = 4;
/// Minimum grace between HUP, TERM, and KILL inside one close deadline so a
/// graceful child is not force-killed prematurely.
const TERMINATION_GRACE_MIN_MS: u64 = 500;
/// Lead time before the close deadline during which the Linux liveness probe
/// runs its stable two-scan verification. Before that window a successful
/// kill(-pgid, 0) answers `Alive` immediately, so escalation does not walk
/// /proc (twice) on every round.
const STABLE_SCOPE_SCAN_LEAD_MS: u64 = 500;
/// Bounded escalation budget used by reader/writer/waiter error edges that
/// cannot block the close path.
const ERROR_ESCALATION_TIMEOUT_MS: u64 = 1_500;
#[cfg(windows)]
const WINDOWS_LAUNCH_HANDSHAKE_TIMEOUT_MS: u32 = 10_000;
/// Bytes of PTY output included in a failed Windows PID handshake error.
#[cfg(windows)]
const WINDOWS_PRE_HANDSHAKE_OUTPUT_MAX: usize = 2 * 1024;
/// How long a closed session handle stays in the idempotent re-close tombstone.
const CLOSED_TOMBSTONE_TTL_MS: u64 = 10 * 60 * 1_000;
const CLOSED_TOMBSTONE_CAP: usize = 1_024;
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
    /// Recently closed session handles (handle -> closed_at_ms) kept so a
    /// repeated single close returns an idempotent already-closed result
    /// instead of "session not found".
    closed: HashMap<String, u64>,
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

    fn remember_closed(&mut self, handle: &str) {
        let now = now_millis();
        if self.closed.len() >= CLOSED_TOMBSTONE_CAP {
            // Evict the OLDEST half when the tombstone grows past its cap so the
            // most recent (and most likely to be re-closed) handles survive.
            let mut entries = self
                .closed
                .drain()
                .map(|(handle, at)| (at, handle))
                .collect::<Vec<_>>();
            entries.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            entries.truncate(CLOSED_TOMBSTONE_CAP / 2);
            self.closed = entries
                .into_iter()
                .map(|(at, handle)| (handle, at))
                .collect();
        }
        self.closed.insert(handle.to_owned(), now);
    }

    fn was_closed(&self, handle: &str) -> bool {
        self.closed
            .get(handle)
            .is_some_and(|at| now_millis().saturating_sub(*at) <= CLOSED_TOMBSTONE_TTL_MS)
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
                closed: HashMap::new(),
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

    /// Refresh every managed session's client lease in response to an inbound
    /// WebUI command. The WebUI's global event stream itself never refreshes
    /// leases — only inbound terminal commands (and Codex keepalives via
    /// touch_client) prove the client is attached right now, so each inbound
    /// command cancels any provisional orphan cleanup before the PTY is
    /// touched.
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

    pub(crate) fn close(&self, handle: &str) -> std::result::Result<bool, CloseError> {
        let session = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| CloseError::Failed("session registry lock was poisoned".into()))?;
            registry.sessions.get(handle).cloned()
        };
        let Some(session) = session else {
            let registry = self
                .registry
                .lock()
                .map_err(|_| CloseError::Failed("session registry lock was poisoned".into()))?;
            if registry.was_closed(handle) {
                // Idempotent re-close after an earlier successful close.
                return Ok(true);
            }
            return Err(CloseError::NotFound(handle.to_owned()));
        };
        match session.close_attempt(default_close_deadline()) {
            CloseOutcome::Closed => {}
            CloseOutcome::Timeout => {
                // Ownership (killer, PTY master, writer) is retained so a later
                // close attempt can continue termination from observed state.
                return Err(CloseError::Timeout);
            }
            CloseOutcome::Failed(message) => return Err(CloseError::Failed(message)),
        }
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| CloseError::Failed("session registry lock was poisoned".into()))?;
        registry.remove_session(handle, &session);
        registry.remember_closed(handle);
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
        let outcomes =
            self.run_close_group(sessions, default_group_deadline(), GROUP_CLOSE_WORKERS)?;
        let (result, _not_closed) = self.apply_close_outcomes(outcomes)?;
        Ok(result)
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
        let outcomes =
            self.run_close_group(sessions, default_group_deadline(), GROUP_CLOSE_WORKERS)?;
        let (result, _not_closed) = self.apply_close_outcomes(outcomes)?;
        if result.failures.is_empty() && result.timed_out.is_empty() {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
            registry.clients.remove(client_session_id);
            drop(registry);
            // The client lease map changed after apply_close_outcomes already
            // notified; wake the WebUI again so it observes the removal.
            self.notify_revision();
            return Ok(result);
        }
        // apply_close_outcomes already notifies when sessions matched; wake
        // explicitly when nothing matched.
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
        let outcomes =
            self.run_close_group(sessions, default_group_deadline(), GROUP_CLOSE_WORKERS)?;
        let (result, not_closed) = self.apply_close_outcomes(outcomes)?;
        // Timed-out or failed orphans stay registered so the next reaper tick
        // can claim and retry them from observed state.
        for (_, session) in not_closed {
            session.reset_orphan_cleanup();
        }
        Ok(result)
    }

    /// Close a snapshot of sessions with a fixed worker bound and one absolute
    /// deadline shared by every session, independent of group size.
    fn run_close_group(
        &self,
        sessions: Vec<(String, Arc<Session>)>,
        deadline: Instant,
        workers: usize,
    ) -> Result<Vec<(String, Arc<Session>, CloseOutcome)>> {
        let matched = sessions.len();
        if matched == 0 {
            return Ok(Vec::new());
        }
        let workers = workers.clamp(1, matched);
        let (work_tx, work_rx) = channel::<(String, Arc<Session>)>();
        let (done_tx, done_rx) = channel::<(String, Arc<Session>, CloseOutcome)>();
        let queue = Arc::new(Mutex::new(work_rx));
        let mut pool = Vec::with_capacity(workers);
        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let done_tx = done_tx.clone();
            pool.push(thread::spawn(move || {
                loop {
                    let job = match queue.lock() {
                        Ok(queue) => queue.recv(),
                        Err(_) => break,
                    };
                    let Ok((handle, session)) = job else {
                        break;
                    };
                    let outcome = session.close_attempt(deadline);
                    if done_tx.send((handle, session, outcome)).is_err() {
                        break;
                    }
                }
            }));
        }
        // Track every session whose result is still outstanding so a stalled
        // worker can be reported as timed out instead of hanging the collector.
        let mut pending: HashMap<String, Arc<Session>> = sessions
            .iter()
            .map(|(handle, session)| (handle.clone(), Arc::clone(session)))
            .collect();
        for (handle, session) in sessions {
            let _ = work_tx.send((handle, session));
        }
        drop(work_tx);
        drop(done_tx);

        // Workers each honor the shared absolute deadline, but the collector
        // never waits past it plus a small response margin: close_attempt may
        // legitimately finish just after the deadline, and a hung worker must
        // not make the whole group wait forever.
        let recv_deadline = deadline + Duration::from_millis(GROUP_CLOSE_RESPONSE_MARGIN_MS);
        let mut outcomes = Vec::with_capacity(matched);
        while outcomes.len() < matched {
            let remaining = recv_deadline.saturating_duration_since(Instant::now());
            match done_rx.recv_timeout(remaining) {
                Ok((handle, session, outcome)) => {
                    pending.remove(&handle);
                    outcomes.push((handle, session, outcome));
                }
                Err(RecvTimeoutError::Timeout) => {
                    // Report every still-pending session as timed out; each
                    // keeps its process ownership so a later close can retry.
                    for (handle, session) in pending.drain() {
                        outcomes.push((handle, session, CloseOutcome::Timeout));
                    }
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("a group close worker exited without reporting its result");
                }
            }
        }
        // Workers exit as soon as their queue recv fails (work_tx was dropped
        // above). Plain JoinHandle::join is deliberately NOT used: a worker
        // still inside close_attempt at the response margin could block the
        // caller forever. close_attempt is strictly bounded by the shared
        // absolute deadline, so dropping the handles detaches workers that
        // finish on their own shortly after the collector returns.
        drop(pool);
        Ok(outcomes)
    }

    /// Remove closed sessions from the registry, tombstone their handles, and
    /// build the truthful per-session group result. `not_closed` carries the
    /// sessions that timed out or failed so callers can retry or reset state.
    fn apply_close_outcomes(
        &self,
        outcomes: Vec<(String, Arc<Session>, CloseOutcome)>,
    ) -> Result<(CloseGroupResult, CloseGroupNotClosed)> {
        let matched = outcomes.len();
        let mut closed = 0;
        let mut timed_out = Vec::new();
        let mut failures = Vec::new();
        let mut not_closed = Vec::new();
        for (handle, session, outcome) in outcomes {
            match outcome {
                CloseOutcome::Closed => {
                    closed += 1;
                    let mut registry = self
                        .registry
                        .lock()
                        .map_err(|_| anyhow::anyhow!("session registry lock was poisoned"))?;
                    registry.remove_session(&handle, &session);
                    registry.remember_closed(&handle);
                }
                CloseOutcome::Timeout => {
                    not_closed.push((handle.clone(), session));
                    timed_out.push(format!(
                        "{handle}: close timed out after the shared group deadline; \
                         retry close to continue termination"
                    ));
                }
                CloseOutcome::Failed(message) => {
                    not_closed.push((handle.clone(), session));
                    failures.push(format!("{handle}: {message}"));
                }
            }
        }
        if closed > 0 || matched > 0 {
            // Removal and Closing (cleanup_claimed) transitions must wake WebUI.
            self.notify_revision();
        }
        Ok((
            CloseGroupResult {
                matched,
                closed,
                timed_out,
                failures,
            },
            not_closed,
        ))
    }

    /// Stop every session and reap every owned process scope before the server
    /// exits. The bounded group close runs first; sessions it could not stop
    /// (Timeout or Failed) keep their process and PTY ownership, and this exit
    /// path then force-terminates and reaps each surviving scope within its own
    /// fresh bounded window. Server exit therefore never orphans a Unix process
    /// group or a Windows job scope, whether it was triggered by ServerStop or
    /// by the accept loop ending. A structural group-close failure (broken
    /// worker channel, poisoned registry) is not fatal to the cleanup: the
    /// snapshot then retains every session and the final forced pass covers all
    /// of them, with the structural error merged into the result. Only scopes
    /// that are *still* alive after their own forced window are reported.
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
        // Server exit is the last process owner: whichever sessions the bounded
        // group close could not verify closed (Timeout, Failed, or a structural
        // error that prevented per-session outcomes) must be force-terminated
        // here instead of letting the host drop with the scope still running.
        let (not_closed, close_error) = match self.run_close_group(
            sessions.clone(),
            default_group_deadline(),
            GROUP_CLOSE_WORKERS,
        ) {
            Ok(outcomes) => match self.apply_close_outcomes(outcomes) {
                Ok((_result, leftovers)) => (leftovers, None),
                Err(error) => {
                    // The registry lock is poisoned and no outcome was applied:
                    // every snapshot session still owns its scope.
                    (sessions, Some(error))
                }
            },
            Err(error) => {
                // The group close failed structurally without per-session
                // outcomes; the snapshot still owns every scope.
                (sessions, Some(error))
            }
        };
        // Each surviving session gets its own forced-termination window so a
        // slow first scope cannot consume a shared deadline and starve the
        // remaining ones of their Kill signal.
        let mut survivors = Vec::new();
        for (handle, session) in not_closed {
            let final_deadline =
                Instant::now() + Duration::from_millis(u64::from(PROCESS_TERMINATE_TIMEOUT_MS));
            match session.force_terminate_scope(final_deadline) {
                EscalationResult::Done => {
                    if session.finalize_now() != CloseOutcome::Closed {
                        survivors.push(format!(
                            "{handle}: close could not be finalized after final termination"
                        ));
                    }
                }
                EscalationResult::Timeout => survivors.push(format!(
                    "{handle}: process scope survived the final forced termination window"
                )),
                EscalationResult::Failed(message) => survivors.push(format!("{handle}: {message}")),
            }
        }
        self.notify_revision();
        if let Some(error) = close_error {
            survivors.push(format!("group close failed: {error:#}"));
        }
        if survivors.is_empty() {
            Ok(())
        } else {
            bail!(
                "failed to stop one or more sessions: {}",
                survivors.join("; ")
            )
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

/// One item handed to the writer thread. `budget` is the number of admission
/// bytes this item holds: input enqueues carry their exact payload length,
/// while terminal response frames (never admitted, sent directly) carry zero.
/// The writer releases exactly `budget` bytes after writing, so the byte
/// counter can never go negative for a response that was never counted.
#[derive(Debug, PartialEq)]
struct WriterItem {
    data: Vec<u8>,
    budget: usize,
}

struct Session {
    inner: Mutex<SessionInner>,
    changed: Condvar,
    host_revision: Arc<HostRevision>,
    writer_tx: Mutex<Option<SyncSender<WriterItem>>>,
    /// Raw bytes currently queued to the writer channel (admission budget).
    pending_writer_bytes: AtomicUsize,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    /// Immutable identifier of the owned process scope, captured at spawn: on
    /// Unix the process-group id (the session leader pid from portable-pty's
    /// setsid), on Windows the root child pid. Every close liveness probe and
    /// termination signal addresses the whole scope through this id. It is
    /// deliberately independent of the externally visible
    /// `SessionState::process_id`, which is cleared as soon as the root exits,
    /// so a naturally exited root can never mask a surviving descendant.
    scope_id: u32,
    /// Monotonic escalation level already sent to the owned process scope.
    /// Persists across close attempts so a retry continues from observed state
    /// instead of restarting the HUP/TERM/KILL ladder from the beginning.
    /// This state is Unix-only; Windows targets the Job Object instead.
    #[cfg(unix)]
    termination: Mutex<TerminationLevel>,
    /// Serializes termination escalation across explicit close, orphan reaping,
    /// and I/O-error cleanup for one session.
    close_lock: Mutex<()>,
    /// Job Object containing the gated launcher and every Grok descendant.
    /// The launcher is assigned before Grok is allowed to start, so membership
    /// does not depend on recovering a complete parent chain from snapshots.
    #[cfg(windows)]
    scope_job: Option<WindowsJob>,
    shutdown: AtomicBool,
    cleanup_claimed: AtomicBool,
    cleanup_committed: AtomicBool,
}

/// Outcome of one close attempt. `Timeout` and `Failed` retain process and PTY
/// ownership so a later attempt can continue termination.
#[derive(Debug, Eq, PartialEq)]
enum CloseOutcome {
    /// The owned process scope is verified terminated and the session is
    /// finalized (or was already complete).
    Closed,
    /// The attempt reached its deadline with the process scope still live.
    Timeout,
    /// Termination failed with a non-timeout error.
    Failed(String),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CloseError {
    NotFound(String),
    Timeout,
    Failed(String),
}

impl fmt::Display for CloseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(handle) => write!(formatter, "session not found: {handle}"),
            Self::Timeout => write!(
                formatter,
                "close timed out after {PROCESS_TERMINATE_TIMEOUT_MS} ms; \
                 the Grok process is still running; retry close to continue termination"
            ),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CloseError {}

type CloseGroupNotClosed = Vec<(String, Arc<Session>)>;

/// Highest Unix process-group signal already sent for this session.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
enum TerminationLevel {
    #[default]
    None,
    Hup,
    Term,
    Kill,
}

#[cfg(windows)]
#[derive(Clone)]
struct WindowsJob {
    handle: Arc<std::os::windows::io::OwnedHandle>,
}

#[cfg(windows)]
struct WindowsLaunchGate {
    event_name: OsString,
    event: std::os::windows::io::OwnedHandle,
    pid_report_name: OsString,
    pid_report: std::os::windows::io::OwnedHandle,
}

/// Result of a liveness probe of the owned process scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScopeAlive {
    /// The whole owned process scope is verifiably gone.
    Gone,
    /// The owned process scope is still alive.
    Alive,
    /// Liveness could not be verified (lock poisoned, or Windows could not
    /// open the process). Never treated as `Gone`.
    Unknown,
}

/// Session-level classification of the owned process scope for escalation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScopeState {
    /// Session terminal or the whole owned scope verifiably gone.
    Gone,
    /// The owned scope is still alive.
    Alive,
    /// Cannot verify (probe inconclusive or no retained pid).
    Unknown,
}

enum EscalationResult {
    /// The process is verified terminated (waiter observed its exit).
    Done,
    /// The absolute deadline passed with the process still live.
    Timeout,
    /// Sending the termination signal failed.
    Failed(String),
}

fn default_close_deadline() -> Instant {
    Instant::now() + Duration::from_millis(u64::from(PROCESS_TERMINATE_TIMEOUT_MS))
}

fn default_group_deadline() -> Instant {
    Instant::now() + Duration::from_millis(GROUP_CLOSE_TIMEOUT_MS)
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
        let grok_state_dir = ensure_grok_state_dir_writable(&config.cwd, provider_session_id)?;
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
        #[cfg(not(windows))]
        let command = build_grok_command(&config, provider_session_id, grok_state_dir.as_deref());
        #[cfg(windows)]
        let (command, launch_gate, scope_job) = build_windows_grok_launcher_command(
            &config,
            provider_session_id,
            grok_state_dir.as_deref(),
        )?;
        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to start interactive Grok Build")?;
        drop(pair.slave);
        let launcher_process_id = child
            .process_id()
            .context("the PTY child did not report a process ID")?;
        #[cfg(windows)]
        let initial_process_id = None;
        #[cfg(not(windows))]
        let initial_process_id = Some(launcher_process_id);
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
                process_id: initial_process_id,
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
            pending_writer_bytes: AtomicUsize::new(0),
            master: Mutex::new(Some(pair.master)),
            scope_id: launcher_process_id,
            #[cfg(unix)]
            termination: Mutex::new(TerminationLevel::default()),
            close_lock: Mutex::new(()),
            #[cfg(windows)]
            scope_job: Some(scope_job),
            shutdown: AtomicBool::new(false),
            cleanup_claimed: AtomicBool::new(false),
            cleanup_committed: AtomicBool::new(false),
        });

        // Writer first so ConPTY inherit-cursor CSI 6n replies are not lost if
        // the reader observes the query before the writer thread exists.
        spawn_writer(Arc::clone(&session), writer, writer_rx);
        spawn_reader(Arc::clone(&session), reader);
        #[cfg(windows)]
        let child = {
            let mut child = child;
            let launch = child
                .as_raw_handle()
                .context("portable-pty did not expose the Windows launcher handle")
                .and_then(|handle| {
                    session
                        .scope_job
                        .as_ref()
                        .context("Windows sessions own a Job Object")?
                        .assign_process(handle)
                        .context("failed to assign the Windows launcher to its Job Object")?;
                    launch_gate
                        .signal()
                        .context("failed to release the Windows Grok launch gate")?;
                    launch_gate
                        .wait_for_grok_pid(WINDOWS_LAUNCH_HANDSHAKE_TIMEOUT_MS, Some(handle))
                        .context("the Windows Grok launcher did not report its process ID")
                });
            match launch {
                Ok(process_id) => {
                    session.set_process_id(process_id)?;
                    child
                }
                Err(error) => {
                    let output = session.bounded_pre_handshake_output();
                    let _ = child.kill();
                    session.close_writer();
                    return Err(if output.is_empty() {
                        error
                    } else {
                        error.context(format!("pre-handshake PTY output: {output}"))
                    });
                }
            }
        };
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
    /// commit with every lease refresh — Codex keepalives and inbound WebUI
    /// terminal commands alike — so a refresh that lands first cancels the
    /// provisional claim.
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
        // A lone Ctrl-C interrupts the current turn; enqueue it without
        // starting a new one, keeping the bytes sent to the PTY unchanged.
        let starts_turn = !(input.len() == 1 && input.as_bytes()[0] == 0x03);
        let data = if input.len() == 1 && input.as_bytes()[0].is_ascii_control() {
            input.into_bytes()
        } else {
            let mut data = Vec::with_capacity(input.len() + 13);
            data.extend_from_slice(b"\x1b[200~");
            data.extend_from_slice(input.as_bytes());
            data.extend_from_slice(b"\x1b[201~\r");
            data
        };
        self.enqueue_input(data, starts_turn)
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
        // Byte budget is checked before any byte is queued; queue-full rejection
        // is always whole-input (never a partial write).
        let data_len = data.len();
        if self
            .pending_writer_bytes
            .load(Ordering::Acquire)
            .saturating_add(data_len)
            > WRITER_QUEUE_MAX_BYTES
        {
            bail!("session input queue is full");
        }
        // Count the bytes BEFORE the item becomes visible to the writer thread.
        // The writer only ever subtracts for items it received, and an item only
        // becomes receivable after a successful try_send that follows this
        // increment, so the counter can never underflow and the writer can never
        // subtract before the matching add. On Full/Disconnected the increment
        // is rolled back under the same lock. Ordering: Release on the add
        // pairs with the writer's Acquire release of the same item, but the
        // counter's ordering is not what makes admission correct — the channel
        // itself synchronizes the item — so the plain Acquire on the release
        // path is the minimal sufficient read side (Relaxed would also be
        // sound); no AcqRel is required for count correctness.
        self.pending_writer_bytes
            .fetch_add(data_len, Ordering::Release);
        match writer.try_send(WriterItem {
            data,
            budget: data_len,
        }) {
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
            Err(TrySendError::Full(_)) => {
                self.pending_writer_bytes
                    .fetch_sub(data_len, Ordering::Acquire);
                bail!("session input queue is full");
            }
            Err(TrySendError::Disconnected(_)) => {
                self.pending_writer_bytes
                    .fetch_sub(data_len, Ordering::Acquire);
                bail!("session input channel is closed");
            }
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

    /// One bounded close attempt. Marks the session as shutdown-requested,
    /// escalates termination of the owned process scope within `deadline`, and
    /// finalizes once termination is verified. On `Timeout`/`Failed` the
    /// process and PTY ownership is retained so a later attempt can continue
    /// from observed state (see `TerminationLevel`).
    fn close_attempt(&self, deadline: Instant) -> CloseOutcome {
        let _close_guard = match self.try_acquire_close_lock(deadline) {
            Ok(guard) => guard,
            Err(outcome) => return outcome,
        };
        self.shutdown.store(true, Ordering::Release);
        if self.process_is_done_or_terminal() {
            return self.finalize_now();
        }
        match self.escalate_until_done(deadline) {
            EscalationResult::Done => self.finalize_now(),
            EscalationResult::Timeout => CloseOutcome::Timeout,
            EscalationResult::Failed(message) => CloseOutcome::Failed(message),
        }
    }

    fn try_acquire_close_lock(
        &self,
        deadline: Instant,
    ) -> std::result::Result<MutexGuard<'_, ()>, CloseOutcome> {
        loop {
            match self.close_lock.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(CloseOutcome::Failed(
                        "session close lock was poisoned".to_owned(),
                    ));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(CloseOutcome::Timeout);
                    }
                    thread::sleep(remaining.min(Duration::from_millis(10)));
                }
            }
        }
    }

    /// True only when the owned process scope is *verified* gone. A terminal
    /// phase alone is not proof: on Unix the waiter observing the root exit
    /// does not prove that a descendant inside the same process group
    /// disappeared, and on Windows the root exit does not prove the descendant
    /// tree is gone. The immutable `scope_id` drives the liveness probe; a
    /// probe failure is never treated as done. Always asks for the stable
    /// verification because this decision finalizes the record.
    fn process_is_done_or_terminal(&self) -> bool {
        matches!(self.probe_scope(true), ScopeAlive::Gone)
    }

    /// Probe the owned process scope. Unix uses the immutable process-group id;
    /// Windows queries the Job Object that contains the gated launcher and all
    /// of its descendants. `stable_verification` is passed through to the Unix
    /// probe so the escalation loop can skip the expensive all-zombie scan
    /// until its final confirmation window.
    fn probe_scope(&self, stable_verification: bool) -> ScopeAlive {
        #[cfg(unix)]
        {
            process_scope_alive(self.scope_id, stable_verification)
        }
        #[cfg(windows)]
        {
            let _ = stable_verification;
            self.scope_job.as_ref().map_or_else(
                || process_alive(self.scope_id),
                WindowsJob::process_scope_alive,
            )
        }
    }

    /// Classify the owned process scope from the immutable `scope_id` and the
    /// platform liveness probe. `Gone` requires the probe to verify the whole
    /// scope is gone; the waiter observing the root exit alone is never
    /// treated as the whole scope being gone.
    fn scope_state(&self, stable_verification: bool) -> ScopeState {
        match self.probe_scope(stable_verification) {
            ScopeAlive::Gone => ScopeState::Gone,
            ScopeAlive::Alive => ScopeState::Alive,
            ScopeAlive::Unknown => ScopeState::Unknown,
        }
    }

    /// Record that the owned process scope has been verified gone. The waiter
    /// waits only on the root process, so the liveness probe is the authority
    /// for the whole group; the exit code stays unknown when the waiter never
    /// reported one.
    fn record_verified_process_done(&self) {
        if let Ok(mut inner) = self.inner.lock()
            && !inner.process_done
        {
            inner.process_done = true;
            inner.updated_at_ms = now_millis();
        }
    }

    /// Send HUP, then TERM, then KILL to the owned process scope within one
    /// monotonic absolute deadline, verifying the whole scope is gone (not
    /// just the root). Escalation milestones honor a minimum grace so a
    /// graceful child is not force-killed prematurely; the highest level
    /// already sent is remembered across attempts on Unix. Windows applies the
    /// same bounded schedule to the kernel-owned Job scope, where the HUP and
    /// TERM phases wait for natural exit and only KILL terminates the job.
    fn escalate_until_done(&self, deadline: Instant) -> EscalationResult {
        let started_at = Instant::now();
        let budget = deadline.saturating_duration_since(started_at);
        if budget.is_zero() {
            return EscalationResult::Timeout;
        }
        let term_after = escalation_milestone(budget, 1, 3);
        let kill_after = escalation_milestone(budget, 2, 3)
            .max(term_after + Duration::from_millis(TERMINATION_GRACE_MIN_MS));
        let term_at = started_at + term_after;
        let kill_at = started_at + kill_after;
        let stable_scan_at = deadline
            .checked_sub(Duration::from_millis(STABLE_SCOPE_SCAN_LEAD_MS))
            .unwrap_or(deadline);
        let mut last_target = TerminationLevel::None;
        loop {
            let now = Instant::now();
            // Outside the final confirmation window a successful kill(0) probe
            // answers Alive immediately; only near the deadline do we walk
            // /proc (twice) to distinguish a living member from an all-zombie
            // group, so each round costs one syscall instead of two scans.
            match self.scope_state(now >= stable_scan_at) {
                ScopeState::Gone => {
                    self.record_verified_process_done();
                    return EscalationResult::Done;
                }
                ScopeState::Alive | ScopeState::Unknown => {}
            }
            if now >= deadline {
                return EscalationResult::Timeout;
            }
            let target = if now < term_at {
                TerminationLevel::Hup
            } else if now < kill_at {
                TerminationLevel::Term
            } else {
                TerminationLevel::Kill
            };
            let repeat_final_signal = cfg!(unix) && target == TerminationLevel::Kill;
            if target != last_target || repeat_final_signal {
                if let Err(error) = self.escalate_to(target) {
                    return EscalationResult::Failed(format!(
                        "failed to terminate Grok process scope: {error}"
                    ));
                }
                last_target = target;
            }
            let next_milestone = match target {
                TerminationLevel::Hup => term_at,
                TerminationLevel::Term => kill_at,
                TerminationLevel::Kill | TerminationLevel::None => deadline,
            };
            let wait = next_milestone
                .min(deadline)
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50))
                .max(Duration::from_millis(1));
            // Wait on session changes (reader progress, waiter report) without
            // treating a terminal phase as proof the scope is gone: the liveness
            // probe at the top of the loop remains the authority.
            let inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => {
                    return EscalationResult::Failed("session state lock was poisoned".into());
                }
            };
            let _ = self
                .changed
                .wait_timeout(inner, wait)
                .map_err(|_| anyhow::anyhow!("session wait lock was poisoned"));
        }
    }

    /// Final forced termination used by `shutdown_all` for sessions the bounded
    /// group close could not stop. Skips the HUP/TERM ladder and sends KILL
    /// directly (Unix process-group SIGKILL / Windows `TerminateJobObject`),
    /// then reaps until the liveness probe verifies the whole scope is gone or
    /// `deadline` passes. The signal is repeated while the scope stays alive
    /// because a member can fork into the group concurrently with the first
    /// signal. Never reports `Done` without the probe certifying the scope
    /// gone; a transient send failure is remembered and retried.
    fn force_terminate_scope(&self, deadline: Instant) -> EscalationResult {
        let stable_scan_at = deadline
            .checked_sub(Duration::from_millis(STABLE_SCOPE_SCAN_LEAD_MS))
            .unwrap_or(deadline);
        let mut last_error: Option<String> = None;
        loop {
            let now = Instant::now();
            if let ScopeAlive::Gone = self.probe_scope(now >= stable_scan_at) {
                self.record_verified_process_done();
                return EscalationResult::Done;
            }
            if now >= deadline {
                return match last_error {
                    Some(message) => EscalationResult::Failed(format!(
                        "final forced termination could not be verified: {message}"
                    )),
                    None => EscalationResult::Timeout,
                };
            }
            if let Err(error) = self.escalate_to(TerminationLevel::Kill) {
                // Transient failures can clear on the next round; keep sending.
                last_error = Some(error.to_string());
            }
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50))
                .max(Duration::from_millis(1));
            let inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => {
                    return EscalationResult::Failed("session state lock was poisoned".into());
                }
            };
            let _ = self
                .changed
                .wait_timeout(inner, wait)
                .map_err(|_| anyhow::anyhow!("session wait lock was poisoned"));
        }
    }

    /// Send the next escalation level once; a level already sent is not sent
    /// again unless a later attempt asks for a stronger one. `SIGKILL` is the
    /// exception on Unix: repeat it while the group remains alive because a
    /// member can fork into the group concurrently with the first signal.
    /// Windows termination targets the Job Object, whose membership is fixed
    /// by the launch gate.
    fn escalate_to(&self, target: TerminationLevel) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            let mut state = match self.termination.lock() {
                Ok(state) => state,
                Err(_) => {
                    return Err(std::io::Error::other("termination state lock was poisoned"));
                }
            };
            if target < *state || (target == *state && target != TerminationLevel::Kill) {
                return Ok(());
            }
            send_termination_signal(self.scope_id, target)?;
            if target > *state {
                *state = target;
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            match self.scope_job.as_ref() {
                Some(job) => job.terminate(target),
                None => terminate_windows_process(self.scope_id, target),
            }
        }
    }

    /// Finalize a session whose process scope is *verified* terminated. The
    /// caller (`close_attempt`) reaches this only after the liveness probe
    /// reported the whole scope gone (or a previously verified close cleared
    /// the retained scope id). Idempotent and safe on already-complete
    /// sessions; always releases the PTY master and writer so a blocked reader
    /// edge unblocks. Returns `Failed` instead of claiming `Closed` when
    /// finalization could not be completed (e.g. the state lock is poisoned).
    fn finalize_now(&self) -> CloseOutcome {
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => {
                return CloseOutcome::Failed(
                    "session could not be finalized: state lock was poisoned".to_owned(),
                );
            }
        };
        let already_terminal = phase_is_terminal(inner.phase);
        if !inner.process_done && !already_terminal {
            // The liveness probe verified the whole scope is gone but the
            // waiter has not reported yet; the probe is the authority for the
            // entire process scope.
            inner.process_done = true;
            inner.updated_at_ms = now_millis();
        }
        let finalized = finalize_session(&mut inner, true) || already_terminal;
        if finalized {
            // The record is fully closed: never expose a pid for a finalized
            // session. finalize_session clears it on the transition; this also
            // covers the already-terminal path. The immutable scope_id remains
            // available on Session for any later verification.
            inner.process_id = None;
            inner.updated_at_ms = now_millis();
        }
        drop(inner);
        if finalized {
            self.close_writer();
            self.release_master();
            self.signal_changed();
            CloseOutcome::Closed
        } else {
            CloseOutcome::Failed(
                "session could not be finalized: the process scope was not verifiably terminated"
                    .to_owned(),
            )
        }
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
        // Terminal response frames bypass admission: they carry budget 0 so the
        // writer's release never underflows the byte counter for bytes that
        // were never counted.
        let result = self.writer_tx.lock().ok().and_then(|writer| {
            writer.as_ref().map(|writer| {
                writer.try_send(WriterItem {
                    data: response,
                    budget: 0,
                })
            })
        });
        match result {
            Some(Ok(())) => {}
            Some(Err(TrySendError::Full(_))) => {
                // Queue full is transient backpressure, not a fatal writer
                // failure: record the error for observers but never escalate
                // the owned process scope, matching enqueue_input semantics.
                self.mark_writer_error_with_escalation(
                    "terminal response queue is full".to_owned(),
                    false,
                );
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
        let finalized = if let Ok(mut inner) = self.inner.lock() {
            inner.reader_done = true;
            record_error(&mut inner, message);
            finalize_session(&mut inner, self.shutdown.load(Ordering::Acquire))
        } else {
            false
        };
        self.finish_transition(finalized);
        // The stream is gone; terminate the owned process scope within a bounded
        // escalation so the session converges instead of waiting forever.
        if !self.process_is_done_or_terminal() {
            self.run_error_edge_escalation();
        }
    }

    fn mark_writer_error(&self, message: String) {
        self.mark_writer_error_with_escalation(message, true);
    }

    /// Record a writer failure and optionally escalate the owned process scope.
    /// `escalate` must be false for recoverable backpressure (a full queue):
    /// the stream is still usable and the scope must survive; escalation is
    /// reserved for unrecoverable failures like a closed channel.
    fn mark_writer_error_with_escalation(&self, message: String, escalate: bool) {
        let finalized = if let Ok(mut inner) = self.inner.lock() {
            record_error(&mut inner, message);
            finalize_session(&mut inner, self.shutdown.load(Ordering::Acquire))
        } else {
            false
        };
        self.finish_transition(finalized);
        if escalate && !self.process_is_done_or_terminal() {
            self.run_error_edge_escalation();
        }
    }

    fn mark_wait_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            record_error(&mut inner, message);
        }
        self.signal_changed();
        // The waiter can no longer observe the root process, so verify the
        // whole owned scope directly (Unix process-group probe or Windows Job
        // Object). Never set process_done
        // without verification: an unverifiable failure keeps the real state
        // and ownership so a later close can retry termination.
        self.run_error_edge_escalation();
    }

    /// Bounded best-effort escalation used by reader/writer/waiter error edges.
    /// Converges finalization only when the scope is verified gone; timeout and
    /// failure keep ownership and state so a later close can retry.
    fn run_error_edge_escalation(&self) {
        let deadline = Instant::now() + Duration::from_millis(ERROR_ESCALATION_TIMEOUT_MS);
        let Ok(_close_guard) = self.try_acquire_close_lock(deadline) else {
            return;
        };
        match self.escalate_until_done(deadline) {
            EscalationResult::Done => {
                let shutdown = self.shutdown.load(Ordering::Acquire);
                let finalized = if let Ok(mut inner) = self.inner.lock() {
                    finalize_session(&mut inner, shutdown)
                } else {
                    false
                };
                self.finish_transition(finalized);
            }
            EscalationResult::Timeout => {
                // Scope still live or unverifiable: keep ownership and state.
            }
            EscalationResult::Failed(error) => {
                self.record_secondary_error(format!(
                    "failed to terminate Grok after an I/O edge failed: {error}"
                ));
            }
        }
    }

    fn mark_exit(&self, exit_code: u32) {
        let finalized = if let Ok(mut inner) = self.inner.lock() {
            if !inner.process_done {
                inner.process_done = true;
                inner.exit_code = Some(exit_code);
                // The root exited: the externally visible process id is stale
                // now (baseline semantics; a natural exit must never expose a
                // dead pid). The immutable Session::scope_id still identifies
                // the whole owned scope for close liveness and termination.
                inner.process_id = None;
            }
            inner.updated_at_ms = now_millis();
            finalize_session(&mut inner, self.shutdown.load(Ordering::Acquire))
        } else {
            false
        };
        self.finish_transition(finalized);
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

    #[cfg(windows)]
    fn set_process_id(&self, process_id: u32) -> Result<()> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("session state lock was poisoned"))?
            .process_id = Some(process_id);
        Ok(())
    }

    #[cfg(windows)]
    fn bounded_pre_handshake_output(&self) -> String {
        let Ok(inner) = self.inner.lock() else {
            return String::new();
        };
        let mut bytes = Vec::new();
        for chunk in &inner.chunks {
            bytes.extend_from_slice(&chunk.data);
            if bytes.len() >= WINDOWS_PRE_HANDSHAKE_OUTPUT_MAX {
                break;
            }
        }
        let end = bytes.len().min(WINDOWS_PRE_HANDSHAKE_OUTPUT_MAX);
        bytes[..end].escape_ascii().to_string()
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

    #[cfg(test)]
    fn test_hold_close_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.close_lock.lock().unwrap()
    }

    #[cfg(test)]
    fn test_poison_close_lock(&self) {
        let _guard = self.close_lock.lock().unwrap();
        panic!("test poisons the session close lock");
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
            activity: projected_activity(self.phase, self.hook.activity),
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

fn build_grok_command(
    config: &LaunchConfig,
    provider_session_id: &str,
    grok_state_dir: Option<&Path>,
) -> CommandBuilder {
    let mut command = CommandBuilder::new(&config.grok_bin);
    command.cwd(config.cwd.as_os_str());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    if let Some(grok_state_dir) = grok_state_dir {
        command.env("GROK_HOME", grok_state_dir.as_os_str());
    }
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

#[cfg(windows)]
fn build_windows_grok_launcher_command(
    config: &LaunchConfig,
    provider_session_id: &str,
    grok_state_dir: Option<&Path>,
) -> Result<(CommandBuilder, WindowsLaunchGate, WindowsJob)> {
    let grok = build_grok_command(config, provider_session_id, grok_state_dir);
    let gate = WindowsLaunchGate::create()?;
    let job = WindowsJob::create().context("failed to create the Windows Grok Job Object")?;
    let executable = env::current_exe().context("failed to locate the grok-bridge executable")?;
    let mut launcher = CommandBuilder::new(executable);
    launcher.cwd(config.cwd.as_os_str());
    launcher.env("TERM", "xterm-256color");
    launcher.env("COLORTERM", "truecolor");
    if let Some(grok_state_dir) = grok_state_dir {
        launcher.env("GROK_HOME", grok_state_dir.as_os_str());
    }
    launcher.arg("__windows-job-child");
    launcher.arg(&gate.event_name);
    launcher.arg(&gate.pid_report_name);
    launcher.args(grok.get_argv());
    Ok((launcher, gate, job))
}

#[cfg(windows)]
pub(crate) fn run_windows_job_child(mut arguments: Vec<OsString>) -> Result<i32> {
    if arguments.len() < 3 {
        bail!("internal Windows job child requires event, PID report, and program names");
    }
    let gate_name = arguments.remove(0);
    let pid_report_name = arguments.remove(0);
    let program = arguments.remove(0);
    wait_for_windows_launch_gate(&gate_name, WINDOWS_LAUNCH_HANDSHAKE_TIMEOUT_MS)?;
    let mut child = std::process::Command::new(&program)
        .args(arguments)
        .spawn()
        .with_context(|| format!("failed to start gated Grok process {program:?}"))?;
    report_windows_grok_pid(&pid_report_name, child.id())?;
    let status = child
        .wait()
        .with_context(|| format!("failed to wait for gated Grok process {program:?}"))?;
    Ok(status.code().unwrap_or(1))
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
    writer_rx: std::sync::mpsc::Receiver<WriterItem>,
) {
    thread::spawn(move || {
        while let Ok(item) = writer_rx.recv() {
            // Only the item's admitted budget is released: inputs release their
            // payload bytes, terminal responses release nothing, so the counter
            // tracks exactly the queued input bytes and can never underflow.
            if let Err(error) = writer.write_all(&item.data).and_then(|()| writer.flush()) {
                // The item left the queue, so release its budget too. Any items
                // still queued when the channel later closes are dropped without
                // release, but the session is terminating at that point and no
                // further admission happens.
                session
                    .pending_writer_bytes
                    .fetch_sub(item.budget, Ordering::Acquire);
                session.mark_writer_error(format!("failed to write Grok input: {error}"));
                return;
            }
            session
                .pending_writer_bytes
                .fetch_sub(item.budget, Ordering::Acquire);
        }
    });
}

fn spawn_waiter(session: Arc<Session>, mut child: Box<dyn portable_pty::Child + Send + Sync>) {
    thread::spawn(move || match child.wait() {
        Ok(status) => session.mark_exit(status.exit_code()),
        Err(error) => session.mark_wait_error(format!("failed while waiting for Grok: {error}")),
    });
}

/// Escalation milestone inside a close budget: `fraction` of the budget, but
/// never sooner than the minimum grace so a graceful child is not
/// force-killed prematurely.
fn escalation_milestone(
    budget: Duration,
    fraction_numerator: u64,
    fraction_denominator: u64,
) -> Duration {
    budget
        .mul_f64(fraction_numerator as f64 / fraction_denominator as f64)
        .max(Duration::from_millis(TERMINATION_GRACE_MIN_MS))
}

/// Send the requested termination to the process scope owned by the Session.
///
/// Unix: portable-pty spawns the PTY child as a session leader (`setsid`), so
/// the process group id equals the child's pid; signal the whole group so every
/// current member — including descendants that share the PTY — receives the
/// escalation. `ESRCH` means the group is already gone and is treated as
/// success; the liveness probe (see `process_scope_alive`) verifies the scope.
///
/// Windows: a gated launcher is assigned to a Job Object before Grok starts.
/// Every descendant therefore enters the same kernel-owned scope, even when an
/// intermediate process exits before close begins.
#[cfg(unix)]
fn send_termination_signal(pid: u32, level: TerminationLevel) -> std::io::Result<()> {
    let signal = match level {
        TerminationLevel::Hup => libc::SIGHUP,
        TerminationLevel::Term => libc::SIGTERM,
        TerminationLevel::Kill => libc::SIGKILL,
        TerminationLevel::None => return Ok(()),
    };
    // SAFETY: `kill` with a process group id derived from the session leader's
    // pid touches only that group; the child owns it because it called setsid.
    let result = unsafe { libc::kill(-(pid as libc::pid_t), signal) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        // The process group is already gone; the liveness probe observes it.
        Ok(())
    } else {
        Err(error)
    }
}

/// Probe whether the Unix process group owned by the Session is still alive.
/// The group id equals the session leader pid (`setsid` in portable-pty), and
/// the group persists as long as any member — including a descendant that
/// retained the PTY after the root exited — remains. Only `ESRCH` proves the
/// whole scope is gone; `EPERM` means it exists but is not signalable (still
/// alive), and any other error leaves the answer unknown.
///
/// `stable_verification` enables the all-zombie distinction on Linux: an
/// all-zombie group can still answer kill(-pgid, 0) with success (EPERM on
/// macOS), so a successful existence probe alone cannot certify `Gone`; the
/// stable two-scan view is required. Escalation asks for that only in its
/// final confirmation window (`STABLE_SCOPE_SCAN_LEAD_MS`); outside it a
/// successful existence probe answers `Alive` without walking /proc.
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn process_scope_alive(pid: u32, stable_verification: bool) -> ScopeAlive {
    // SAFETY: signal 0 only probes existence; `-pid` addresses the group owned
    // by the session leader that called setsid, so no unrelated group is hit.
    let result = unsafe { libc::kill(-(pid as libc::pid_t), 0) };
    if result == 0 {
        #[cfg(target_os = "macos")]
        return macos_process_group_alive(pid);
        #[cfg(target_os = "linux")]
        return linux_process_group_alive(pid, stable_verification);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return ScopeAlive::Alive;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => ScopeAlive::Gone,
        Some(libc::EPERM) => ScopeAlive::Alive,
        _ => ScopeAlive::Unknown,
    }
}

/// `kill(-pgid, 0)` also succeeds when a group contains only zombies. Require
/// two all-zombie views with identical membership before treating the group as
/// terminated: enumeration and per-PID inspection are not one atomic snapshot.
#[cfg(target_os = "macos")]
fn macos_process_group_alive(pgid: u32) -> ScopeAlive {
    stable_process_group_scan_result(
        macos_process_group_scan(pgid),
        macos_process_group_scan(pgid),
    )
}

#[cfg(target_os = "macos")]
fn macos_process_group_scan(pgid: u32) -> (ScopeAlive, Vec<u32>) {
    const MAX_GROUP_MEMBERS: usize = 4096;
    let mut pids = vec![0_i32; MAX_GROUP_MEMBERS];
    let buffer_bytes = pids.len() * std::mem::size_of::<libc::pid_t>();
    let returned = unsafe {
        libc::proc_listpgrppids(
            pgid as libc::pid_t,
            pids.as_mut_ptr().cast(),
            buffer_bytes as libc::c_int,
        )
    };
    if returned < 0 || returned as usize >= pids.len() {
        return (ScopeAlive::Unknown, Vec::new());
    }
    let count = returned as usize;
    let mut members = pids
        .into_iter()
        .take(count)
        .filter(|pid| *pid > 0)
        .map(|pid| pid as u32)
        .collect::<Vec<_>>();
    members.sort_unstable();
    members.dedup();
    let mut verdict = ScopeAlive::Gone;
    for &process_id in &members {
        let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
        let read = unsafe {
            libc::proc_pidinfo(
                process_id as libc::pid_t,
                libc::PROC_PIDTBSDINFO,
                0,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of_val(&info) as libc::c_int,
            )
        };
        if read != std::mem::size_of_val(&info) as libc::c_int {
            verdict = ScopeAlive::Unknown;
            continue;
        }
        if info.pbi_pgid != pgid {
            verdict = ScopeAlive::Unknown;
            continue;
        }
        if info.pbi_status != libc::SZOMB {
            return (ScopeAlive::Alive, members);
        }
    }
    (verdict, members)
}

/// Linux exposes process-group and state fields in `/proc/<pid>/stat`. When
/// `stable_verification` is set, scan twice and require stable membership
/// before certifying an all-zombie group; otherwise a successful kill(0) probe
/// already proved the group exists and a living member answers `Alive`
/// immediately, skipping the /proc traversal.
#[cfg(target_os = "linux")]
fn linux_process_group_alive(pgid: u32, stable_verification: bool) -> ScopeAlive {
    if !stable_verification {
        return ScopeAlive::Alive;
    }
    stable_process_group_scan_result(
        linux_process_group_scan(pgid),
        linux_process_group_scan(pgid),
    )
}

#[cfg(target_os = "linux")]
fn linux_process_group_scan(pgid: u32) -> (ScopeAlive, Vec<u32>) {
    let entries = match std::fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return (ScopeAlive::Unknown, Vec::new()),
    };
    let mut members = Vec::new();
    let mut verdict = ScopeAlive::Gone;
    for entry in entries {
        let Ok(entry) = entry else {
            verdict = ScopeAlive::Unknown;
            continue;
        };
        let Some(process_id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(_) => {
                verdict = ScopeAlive::Unknown;
                continue;
            }
        };
        let Some(after_name) = stat.rsplit_once(") ").map(|(_, fields)| fields) else {
            verdict = ScopeAlive::Unknown;
            continue;
        };
        let mut fields = after_name.split_whitespace();
        let state = fields.next();
        let _parent_pid = fields.next();
        let process_group = fields.next().and_then(|field| field.parse::<u32>().ok());
        if process_group != Some(pgid) {
            continue;
        }
        members.push(process_id);
        match state {
            Some("Z" | "X") => {}
            Some(_) => {
                members.sort_unstable();
                return (ScopeAlive::Alive, members);
            }
            None => verdict = ScopeAlive::Unknown,
        }
    }
    members.sort_unstable();
    members.dedup();
    (verdict, members)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn stable_process_group_scan_result(
    first: (ScopeAlive, Vec<u32>),
    second: (ScopeAlive, Vec<u32>),
) -> ScopeAlive {
    if first.0 != ScopeAlive::Gone {
        return first.0;
    }
    if second.0 != ScopeAlive::Gone {
        return second.0;
    }
    if first.1 == second.1 {
        ScopeAlive::Gone
    } else {
        ScopeAlive::Unknown
    }
}

#[cfg(windows)]
impl WindowsJob {
    fn create() -> std::io::Result<Self> {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw as _) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                raw,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            handle: Arc::new(handle),
        })
    }

    fn assign_process(&self, process: std::os::windows::io::RawHandle) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        if unsafe { AssignProcessToJobObject(self.handle.as_raw_handle() as _, process as _) } == 0
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn process_scope_alive(&self) -> ScopeAlive {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            QueryInformationJobObject,
        };

        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        if unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle() as _,
                JobObjectBasicAccountingInformation,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            ScopeAlive::Unknown
        } else if info.ActiveProcesses == 0 {
            ScopeAlive::Gone
        } else {
            ScopeAlive::Alive
        }
    }

    /// Apply one termination level to the Job Object scope.
    ///
    /// Windows has no job-scoped graceful signal, so the escalation ladder's
    /// `Hup` and `Term` phases are wait-only: the close loop keeps probing
    /// `ActiveProcesses` and gives members a chance to exit naturally, and no
    /// force-kill is issued. Only `Kill` force-terminates every member with
    /// `TerminateJobObject`. A scope that is already verifiably gone is
    /// success, mirroring the Unix `ESRCH` semantics.
    fn terminate(&self, level: TerminationLevel) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        match level {
            TerminationLevel::None => Ok(()),
            TerminationLevel::Hup | TerminationLevel::Term => {
                // Wait-only phase: no TerminateJobObject. The escalation loop
                // owns the bounded wait between milestones; a job whose members
                // already exited is success.
                Ok(())
            }
            TerminationLevel::Kill => {
                if unsafe { TerminateJobObject(self.handle.as_raw_handle() as _, 1) } != 0
                    || self.process_scope_alive() == ScopeAlive::Gone
                {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            }
        }
    }
}

#[cfg(windows)]
impl WindowsLaunchGate {
    fn create() -> Result<Self> {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::{CreateEventW, CreateSemaphoreW};

        let id = generate_provider_session_id()?;
        let event_name = OsString::from(format!("Local\\grok-bridge-launch-{id}"));
        let pid_report_name = OsString::from(format!("Local\\grok-bridge-pid-{id}"));
        let wide_event_name = windows_wide_null(&event_name);
        // Manual-reset (bManualReset=TRUE) so SetEvent stays signaled if the
        // ConPTY child reaches WaitForSingleObject after the parent released
        // the gate.
        let raw_event = unsafe { CreateEventW(std::ptr::null(), 1, 0, wide_event_name.as_ptr()) };
        if raw_event.is_null() {
            return Err(std::io::Error::last_os_error())
                .context("failed to create the Windows Grok launch event");
        }
        let event = unsafe { OwnedHandle::from_raw_handle(raw_event as _) };
        let wide_pid_report_name = windows_wide_null(&pid_report_name);
        let raw_pid_report = unsafe {
            CreateSemaphoreW(std::ptr::null(), 0, i32::MAX, wide_pid_report_name.as_ptr())
        };
        if raw_pid_report.is_null() {
            return Err(std::io::Error::last_os_error())
                .context("failed to create the Windows Grok PID report semaphore");
        }
        Ok(Self {
            event_name,
            event,
            pid_report_name,
            pid_report: unsafe { OwnedHandle::from_raw_handle(raw_pid_report as _) },
        })
    }

    fn signal(&self) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Threading::SetEvent;

        if unsafe { SetEvent(self.event.as_raw_handle() as _) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn wait_for_grok_pid(
        &self,
        timeout_ms: u32,
        launcher: Option<std::os::windows::io::RawHandle>,
    ) -> Result<u32> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::{
                GetExitCodeProcess, ReleaseSemaphore, WaitForMultipleObjects, WaitForSingleObject,
            },
        };

        let semaphore = self.pid_report.as_raw_handle() as _;
        let wait_status = if let Some(launcher) = launcher {
            let handles = [semaphore, launcher as _];
            unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout_ms) }
        } else {
            unsafe { WaitForSingleObject(semaphore, timeout_ms) }
        };
        match (wait_status, launcher) {
            (WAIT_OBJECT_0, _) => {}
            (status, Some(launcher)) if status == WAIT_OBJECT_0 + 1 => {
                let mut exit_code = 0_u32;
                if unsafe { GetExitCodeProcess(launcher as _, &mut exit_code) } == 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("the launcher process exited before reporting a PID");
                }
                bail!("the launcher process exited with code {exit_code} before reporting a PID");
            }
            (WAIT_TIMEOUT, _) => {
                bail!("timed out after {timeout_ms} ms waiting for the Windows Grok process ID")
            }
            (WAIT_FAILED, _) => {
                return Err(std::io::Error::last_os_error())
                    .context("failed while waiting for the Windows Grok process ID");
            }
            (status, _) => bail!("unexpected Windows Grok PID wait status: {status}"),
        }
        let mut previous = 0_i32;
        if unsafe { ReleaseSemaphore(semaphore, 1, &mut previous) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("failed to read the Windows Grok process ID");
        }
        u32::try_from(previous)
            .ok()
            .and_then(|value| value.checked_add(1))
            .filter(|pid| *pid != 0)
            .context("the Windows Grok launcher reported an invalid process ID")
    }
}

#[cfg(windows)]
fn windows_wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn wait_for_windows_launch_gate(name: &std::ffi::OsStr, timeout_ms: u32) -> Result<()> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenEventW, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject},
    };

    let wide_name = windows_wide_null(name);
    let raw = unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, 0, wide_name.as_ptr()) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error())
            .context("failed to open the Windows Grok launch event");
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    match unsafe {
        WaitForSingleObject(
            std::os::windows::io::AsRawHandle::as_raw_handle(&handle) as _,
            timeout_ms,
        )
    } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => {
            bail!("timed out after {timeout_ms} ms waiting for the Windows Grok launch gate")
        }
        WAIT_FAILED => Err(std::io::Error::last_os_error())
            .context("failed while waiting for the Windows Grok launch event"),
        status => bail!("unexpected Windows Grok launch wait status: {status}"),
    }
}

#[cfg(windows)]
fn report_windows_grok_pid(name: &std::ffi::OsStr, pid: u32) -> Result<()> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Threading::{
        OpenSemaphoreW, ReleaseSemaphore, SEMAPHORE_MODIFY_STATE,
    };

    let pid = i32::try_from(pid).context("the Windows Grok process ID exceeds i32::MAX")?;
    let wide_name = windows_wide_null(name);
    let raw = unsafe { OpenSemaphoreW(SEMAPHORE_MODIFY_STATE, 0, wide_name.as_ptr()) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error())
            .context("failed to open the Windows Grok PID report semaphore");
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    if unsafe {
        ReleaseSemaphore(
            std::os::windows::io::AsRawHandle::as_raw_handle(&handle) as _,
            pid,
            std::ptr::null_mut(),
        )
    } == 0
    {
        Err(std::io::Error::last_os_error()).context("failed to report the Windows Grok process ID")
    } else {
        Ok(())
    }
}

/// Apply one termination level to a bare Windows process, used when no Job
/// Object scope is retained (e.g. tests). Mirrors `WindowsJob::terminate`:
/// `Hup` and `Term` are wait-only phases and `Kill` force-terminates the
/// process with `TerminateProcess`; a process already gone is success.
#[cfg(windows)]
fn terminate_windows_process(pid: u32, level: TerminationLevel) -> std::io::Result<()> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    match level {
        TerminationLevel::None => Ok(()),
        TerminationLevel::Hup | TerminationLevel::Term => {
            // Wait-only phase: no force-kill. A process that already exited is
            // observed by the liveness probe.
            Ok(())
        }
        TerminationLevel::Kill => {
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
            if handle.is_null() {
                return if process_alive(pid) == ScopeAlive::Gone {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                };
            }
            let result = unsafe { TerminateProcess(handle, 1) };
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            if result != 0 || process_alive(pid) == ScopeAlive::Gone {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> ScopeAlive {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return if std::io::Error::last_os_error().raw_os_error()
            == Some(ERROR_INVALID_PARAMETER as i32)
        {
            ScopeAlive::Gone
        } else {
            ScopeAlive::Unknown
        };
    }
    let status = unsafe { WaitForSingleObject(handle, 0) };
    unsafe { CloseHandle(handle) };
    match status {
        WAIT_OBJECT_0 => ScopeAlive::Gone,
        WAIT_TIMEOUT => ScopeAlive::Alive,
        WAIT_FAILED => ScopeAlive::Unknown,
        _ => ScopeAlive::Unknown,
    }
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
    // The record is fully closed: clear the externally visible process id so a
    // naturally exited session never exposes a stale pid. The immutable
    // Session::scope_id keeps addressing the owned scope if a later close
    // still needs to verify or terminate it.
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
    if phase_is_terminal(current) {
        return None;
    }
    if !process_done {
        return None;
    }
    if shutdown {
        // An explicit close marks the scope terminated once the process is
        // verified gone; the PTY master is released right after so the reader
        // edge unblocks. Missing reader EOF does not keep the record half-open.
        Some(SessionPhase::Stopped)
    } else if !reader_done {
        None
    } else if failed || exit_code != Some(0) {
        Some(SessionPhase::Failed)
    } else {
        Some(SessionPhase::Exited)
    }
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

/// Map the raw hook activity to what a client should observe, resolving
/// phase/hook conflicts by fixed priority: a terminal phase overrides a stale
/// in-flight hook (the hook stream died with the process), and an idle TUI
/// resolves a stale working hook to done.
fn projected_activity(phase: SessionPhase, hook_activity: HookActivity) -> HookActivity {
    if phase_is_terminal(phase) {
        if matches!(hook_activity, HookActivity::Working | HookActivity::Waiting) {
            HookActivity::Unknown
        } else {
            hook_activity
        }
    } else if phase == SessionPhase::Idle && hook_activity == HookActivity::Working {
        HookActivity::Done
    } else {
        hook_activity
    }
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

fn ensure_grok_state_dir_writable(
    cwd: &Path,
    provider_session_id: &str,
) -> Result<Option<PathBuf>> {
    let Some(state_dir) = grok_state_dir(cwd) else {
        return Ok(None);
    };
    ensure_grok_state_dir_writable_at(&state_dir, provider_session_id)?;
    Ok(Some(state_dir))
}

fn grok_state_dir(cwd: &Path) -> Option<PathBuf> {
    let state_dir = grok_state_dir_from(
        env::var_os("GROK_HOME"),
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
        cfg!(windows),
    )?;
    Some(resolve_state_dir_from_cwd(cwd, state_dir))
}

fn resolve_state_dir_from_cwd(cwd: &Path, state_dir: PathBuf) -> PathBuf {
    if state_dir.is_absolute() {
        state_dir
    } else {
        cwd.join(state_dir)
    }
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
    data.iter().any(|byte| matches!(*byte, b'\r' | b'\n'))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const TEST_PROVIDER_SESSION_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[cfg(windows)]
    const WINDOWS_PROCESS_TREE_ROLE_ENV: &str = "GROK_BRIDGE_TEST_PROCESS_TREE_ROLE";
    #[cfg(windows)]
    const WINDOWS_PROCESS_TREE_DIR_ENV: &str = "GROK_BRIDGE_TEST_PROCESS_TREE_DIR";
    #[cfg(windows)]
    const WINDOWS_PROCESS_TREE_ROOT_ENV: &str = "GROK_BRIDGE_TEST_PROCESS_TREE_ROOT";
    #[cfg(windows)]
    const WINDOWS_PROCESS_TREE_GATE_ENV: &str = "GROK_BRIDGE_TEST_PROCESS_TREE_GATE";
    #[cfg(windows)]
    const WINDOWS_PROCESS_TREE_HELPER_TEST: &str = "session::tests::windows_process_tree_helper";
    #[cfg(windows)]
    const WINDOWS_JOB_CHILD_HELPER_ENV: &str = "GROK_BRIDGE_TEST_JOB_CHILD_HELPER";
    #[cfg(windows)]
    const WINDOWS_JOB_CHILD_GATE_ENV: &str = "GROK_BRIDGE_TEST_JOB_CHILD_GATE";
    #[cfg(windows)]
    const WINDOWS_JOB_CHILD_PID_ENV: &str = "GROK_BRIDGE_TEST_JOB_CHILD_PID";
    #[cfg(windows)]
    const WINDOWS_JOB_CHILD_HELPER_TEST: &str =
        "session::tests::windows_job_child_handshake_helper";
    #[cfg(windows)]
    const WINDOWS_PROCESS_TREE_TIMEOUT: Duration = Duration::from_secs(10);

    #[cfg(windows)]
    struct WindowsProcessTreeCleanup {
        root: Option<std::process::Child>,
        directory: PathBuf,
        pids: Vec<u32>,
        armed: bool,
    }

    #[cfg(windows)]
    impl WindowsProcessTreeCleanup {
        fn new(root: std::process::Child, directory: PathBuf) -> Self {
            let root_pid = root.id();
            Self {
                root: Some(root),
                directory,
                pids: vec![root_pid],
                armed: true,
            }
        }

        fn remember(&mut self, pids: &[u32]) {
            self.pids.extend_from_slice(pids);
            self.pids.sort_unstable();
            self.pids.dedup();
        }

        fn disarm(&mut self) {
            self.armed = false;
            self.root.take();
            self.pids.clear();
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[cfg(windows)]
    impl Drop for WindowsProcessTreeCleanup {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            for name in ["child.pid", "grandchild.pid"] {
                if let Ok(value) = fs::read_to_string(self.directory.join(name))
                    && let Ok(pid) = value.trim().parse::<u32>()
                {
                    self.pids.push(pid);
                }
            }
            self.pids.sort_unstable();
            self.pids.dedup();
            for &pid in self.pids.iter().rev() {
                terminate_windows_test_process(pid);
            }
            if let Some(root) = self.root.as_mut() {
                let _ = root.kill();
            }
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[cfg(windows)]
    fn terminate_windows_test_process(pid: u32) {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
        };

        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if !handle.is_null() {
            unsafe {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }

    #[cfg(windows)]
    fn spawn_windows_process_tree_helper(
        role: &str,
        directory: &Path,
        root_pid: u32,
        gate_name: Option<&std::ffi::OsStr>,
    ) -> std::process::Child {
        use std::process::{Command, Stdio};

        let mut command =
            Command::new(env::current_exe().expect("test executable must be discoverable"));
        command
            .args(["--exact", WINDOWS_PROCESS_TREE_HELPER_TEST, "--nocapture"])
            .env(WINDOWS_PROCESS_TREE_ROLE_ENV, role)
            .env(WINDOWS_PROCESS_TREE_DIR_ENV, directory)
            .env(WINDOWS_PROCESS_TREE_ROOT_ENV, root_pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(gate_name) = gate_name {
            command.env(WINDOWS_PROCESS_TREE_GATE_ENV, gate_name);
        }
        command
            .spawn()
            .expect("Windows process-tree helper must start")
    }

    #[cfg(windows)]
    fn wait_for_windows_process_tree_ready(path: &Path) -> [u32; 3] {
        let deadline = Instant::now() + WINDOWS_PROCESS_TREE_TIMEOUT;
        let mut last_contents = String::new();
        loop {
            if let Ok(contents) = fs::read_to_string(path) {
                last_contents = contents;
                let pids = last_contents
                    .split_whitespace()
                    .filter_map(|value| value.parse::<u32>().ok())
                    .collect::<Vec<_>>();
                if let [root, child, grandchild] = pids.as_slice()
                    && *root != 0
                    && *child != 0
                    && *grandchild != 0
                {
                    return [*root, *child, *grandchild];
                }
            }
            assert!(
                Instant::now() < deadline,
                "Windows process tree did not become ready within {WINDOWS_PROCESS_TREE_TIMEOUT:?}; last ready contents: {last_contents:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn wait_for_windows_process_tree_exit(pids: [u32; 3]) {
        let deadline = Instant::now() + WINDOWS_PROCESS_TREE_TIMEOUT;
        loop {
            let states = pids.map(process_alive);
            if states.iter().all(|state| *state == ScopeAlive::Gone) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Windows process tree did not exit within {WINDOWS_PROCESS_TREE_TIMEOUT:?}: pids={pids:?}, states={states:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn wait_for_windows_intermediates_exit(pids: [u32; 2]) {
        let deadline = Instant::now() + WINDOWS_PROCESS_TREE_TIMEOUT;
        loop {
            let states = pids.map(process_alive);
            if states.iter().all(|state| *state == ScopeAlive::Gone) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "wrapper and short-lived child did not exit within {WINDOWS_PROCESS_TREE_TIMEOUT:?}: pids={pids:?}, states={states:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn wait_for_windows_marker(path: &Path) {
        let deadline = Instant::now() + WINDOWS_PROCESS_TREE_TIMEOUT;
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "Windows process-tree marker did not appear within {WINDOWS_PROCESS_TREE_TIMEOUT:?}: {path:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

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
            scope_id: 42,
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
            pending_writer_bytes: AtomicUsize::new(0),
            master: Mutex::new(None),
            #[cfg(unix)]
            termination: Mutex::new(TerminationLevel::default()),
            #[cfg(windows)]
            scope_job: None,
            close_lock: Mutex::new(()),
            shutdown: AtomicBool::new(false),
            cleanup_claimed: AtomicBool::new(false),
            cleanup_committed: AtomicBool::new(false),
        }
    }

    fn test_host(provider_session_id: &str, phase: SessionPhase) -> SessionHost {
        test_host_with_session(provider_session_id, phase).0
    }

    /// Build a host that owns one test session plus the session itself, so
    /// tests can drive host APIs and manipulate the session (e.g. its close
    /// lock) from outside the session module.
    fn test_host_with_session(
        provider_session_id: &str,
        phase: SessionPhase,
    ) -> (SessionHost, Arc<Session>) {
        let revision = Arc::new(HostRevision::new());
        let session = Arc::new(test_session_with_revision(phase, Arc::clone(&revision)));
        let handle = session.state().unwrap().session;
        let host = SessionHost {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::from([(handle.clone(), Arc::clone(&session))]),
                provider_sessions: HashMap::from([(provider_session_id.to_owned(), handle)]),
                clients: HashMap::new(),
                closed: HashMap::new(),
            }),
            next_id: AtomicU64::new(1),
            orphan_policy: OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            },
            revision,
        };
        (host, session)
    }

    pub(crate) fn with_test_host_holding_close_lock<R>(
        provider_session_id: &str,
        phase: SessionPhase,
        run: impl FnOnce(SessionHost) -> R,
    ) -> R {
        let (host, session) = test_host_with_session(provider_session_id, phase);
        let _held = session.test_hold_close_lock();
        run(host)
    }

    pub(crate) fn test_host_with_poisoned_close_lock(
        provider_session_id: &str,
        phase: SessionPhase,
    ) -> SessionHost {
        let (host, session) = test_host_with_session(provider_session_id, phase);
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            session.test_poison_close_lock();
        }));
        assert!(poisoned.is_err());
        host
    }

    #[cfg(windows)]
    fn windows_process_host(root_pid: u32, job: WindowsJob) -> SessionHost {
        let revision = Arc::new(HostRevision::new());
        let mut session = test_session_with_revision(SessionPhase::Running, Arc::clone(&revision));
        session.scope_id = root_pid;
        session.scope_job = Some(job);
        session.inner.get_mut().unwrap().process_id = Some(root_pid);
        let session = Arc::new(session);
        SessionHost {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::from([("gbt-test".to_owned(), Arc::clone(&session))]),
                provider_sessions: HashMap::from([(
                    TEST_PROVIDER_SESSION_ID.to_owned(),
                    "gbt-test".to_owned(),
                )]),
                clients: HashMap::new(),
                closed: HashMap::new(),
            }),
            next_id: AtomicU64::new(1),
            orphan_policy: OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            },
            revision,
        }
    }

    /// Build a host that owns one Running test session whose immutable
    /// `scope_id` addresses a real Unix process group. Mirrors
    /// `windows_process_host`: the scope id is set before the session is
    /// wrapped in an Arc and inserted into the registry.
    #[cfg(unix)]
    fn unix_process_host(pgid: u32) -> SessionHost {
        let revision = Arc::new(HostRevision::new());
        let mut session = test_session_with_revision(SessionPhase::Running, Arc::clone(&revision));
        session.scope_id = pgid;
        session.inner.get_mut().unwrap().process_id = Some(pgid);
        let session = Arc::new(session);
        SessionHost {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions: HashMap::from([("gbt-test".to_owned(), Arc::clone(&session))]),
                provider_sessions: HashMap::from([(
                    TEST_PROVIDER_SESSION_ID.to_owned(),
                    "gbt-test".to_owned(),
                )]),
                clients: HashMap::new(),
                closed: HashMap::new(),
            }),
            next_id: AtomicU64::new(1),
            orphan_policy: OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            },
            revision,
        }
    }

    /// Like `unix_process_host` but registers one Running session per process
    /// group, with handles `gbt-1`, `gbt-2`, ... in pgid order.
    #[cfg(unix)]
    fn unix_process_hosts(pgids: &[u32]) -> SessionHost {
        let revision = Arc::new(HostRevision::new());
        let mut sessions = HashMap::new();
        for (index, &pgid) in pgids.iter().enumerate() {
            let handle = format!("gbt-{}", index + 1);
            let mut session =
                test_session_with_revision(SessionPhase::Running, Arc::clone(&revision));
            session.inner.get_mut().unwrap().session = handle.clone();
            session.scope_id = pgid;
            session.inner.get_mut().unwrap().process_id = Some(pgid);
            sessions.insert(handle, Arc::new(session));
        }
        let provider_sessions = sessions
            .keys()
            .cloned()
            .map(|handle| (handle.clone(), handle))
            .collect();
        SessionHost {
            registry: Mutex::new(SessionRegistry {
                accepting: true,
                sessions,
                provider_sessions,
                clients: HashMap::new(),
                closed: HashMap::new(),
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
    fn resolves_relative_grok_home_against_session_working_directory() {
        let cwd = PathBuf::from("/workspace/project");
        assert_eq!(
            resolve_state_dir_from_cwd(&cwd, PathBuf::from(".grok-state")),
            cwd.join(".grok-state")
        );
        assert_eq!(
            resolve_state_dir_from_cwd(&cwd, PathBuf::from("/custom/grok")),
            PathBuf::from("/custom/grok")
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
        let grok_home = PathBuf::from(r"C:\repo\.grok-state");
        let command = build_grok_command(&config, TEST_PROVIDER_SESSION_ID, Some(&grok_home));
        assert_eq!(command.get_env("GROK_BRIDGE_SESSION"), None);
        assert_eq!(command.get_env("GROK_BRIDGE_HOOK_TOKEN"), None);
        assert_eq!(command.get_env("GROK_HOME"), Some(grok_home.as_os_str()));
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

    #[cfg(windows)]
    #[test]
    fn builds_gated_windows_launcher_before_the_grok_command() {
        let config = LaunchConfig {
            grok_bin: OsString::from(r"C:\Program Files\Grok\grok.exe"),
            cwd: PathBuf::from(r"C:\repo"),
            prompt: Some("fix it".to_owned()),
            model: Some("grok-4".to_owned()),
            owner: None,
            always_approve: false,
            client_session_id: None,
            client_lease: None,
            orphan_policy: OrphanPolicy {
                lease_ms: 120_000,
                grace_ms: 600_000,
            },
        };
        let grok_home = PathBuf::from(r"C:\repo\.grok-state");
        let (command, gate, job) = build_windows_grok_launcher_command(
            &config,
            TEST_PROVIDER_SESSION_ID,
            Some(&grok_home),
        )
        .unwrap();
        let argv = command.get_argv();
        assert_eq!(PathBuf::from(&argv[0]), env::current_exe().unwrap());
        assert_eq!(argv[1], OsString::from("__windows-job-child"));
        assert_eq!(argv[2], gate.event_name);
        assert_eq!(argv[3], gate.pid_report_name);
        assert_eq!(argv[4], OsString::from(r"C:\Program Files\Grok\grok.exe"));
        assert_eq!(argv[5], OsString::from("--session-id"));
        assert_eq!(argv[6], OsString::from(TEST_PROVIDER_SESSION_ID));
        assert_eq!(
            command.get_cwd().map(OsString::as_os_str),
            Some(config.cwd.as_os_str())
        );
        assert_eq!(command.get_env("GROK_HOME"), Some(grok_home.as_os_str()));
        assert_eq!(job.process_scope_alive(), ScopeAlive::Gone);
    }

    #[cfg(windows)]
    #[test]
    fn launch_gate_reports_the_real_grok_pid_without_a_file_or_shell() {
        let gate = WindowsLaunchGate::create().unwrap();
        report_windows_grok_pid(&gate.pid_report_name, 1234).unwrap();
        assert_eq!(gate.wait_for_grok_pid(100, None).unwrap(), 1234);
    }

    #[cfg(windows)]
    #[test]
    fn unassigned_windows_launcher_gate_has_a_bounded_wait() {
        let gate = WindowsLaunchGate::create().unwrap();
        let error = wait_for_windows_launch_gate(&gate.event_name, 1).unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
    }

    #[cfg(windows)]
    #[test]
    fn launch_gate_signal_before_wait_is_not_lost() {
        let gate = WindowsLaunchGate::create().unwrap();
        gate.signal().unwrap();
        wait_for_windows_launch_gate(&gate.event_name, 100).unwrap();
    }

    #[test]
    fn title_callbacks_reply_to_conpty_cursor_position_report() {
        let mut parser = vt100::Parser::new_with_callbacks(
            INITIAL_ROWS,
            INITIAL_COLS,
            0,
            TitleCallbacks::default(),
        );
        parser.process(b"\x1b[6n");
        let responses = std::mem::take(&mut parser.callbacks_mut().responses);
        assert_eq!(responses, vec![b"\x1b[1;1R".to_vec()]);
        parser.process(b"\x1b[5n");
        let responses = std::mem::take(&mut parser.callbacks_mut().responses);
        assert_eq!(responses, vec![b"\x1b[0n".to_vec()]);
    }

    #[cfg(windows)]
    #[test]
    fn handshake_error_includes_escaped_pre_handshake_pty_output() {
        let session = test_session(SessionPhase::Starting);
        session.append_output(b"\x1b[6nhelper-failed".to_vec());
        let output = session.bounded_pre_handshake_output();
        assert!(output.contains("\\x1b[6n"), "{output}");
        assert!(output.contains("helper-failed"), "{output}");
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_child_handshake_helper() {
        if env::var_os(WINDOWS_JOB_CHILD_HELPER_ENV).is_none() {
            return;
        }
        let gate_name = env::var_os(WINDOWS_JOB_CHILD_GATE_ENV)
            .expect("job-child helper launch gate must be provided");
        let pid_name = env::var_os(WINDOWS_JOB_CHILD_PID_ENV)
            .expect("job-child helper PID report must be provided");
        let status = run_windows_job_child(vec![
            gate_name,
            pid_name,
            OsString::from("cmd.exe"),
            OsString::from("/c"),
            OsString::from("exit 0"),
        ])
        .expect("job-child helper must complete");
        assert_eq!(status, 0);
    }

    #[cfg(windows)]
    #[test]
    fn conpty_job_child_handshake_reports_pid() {
        let gate = WindowsLaunchGate::create().unwrap();
        let job = WindowsJob::create().unwrap();
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                cols: INITIAL_COLS,
                rows: INITIAL_ROWS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let mut command = CommandBuilder::new(env::current_exe().unwrap());
        command.args(["--exact", WINDOWS_JOB_CHILD_HELPER_TEST, "--nocapture"]);
        command.env(WINDOWS_JOB_CHILD_HELPER_ENV, "1");
        command.env(WINDOWS_JOB_CHILD_GATE_ENV, &gate.event_name);
        command.env(WINDOWS_JOB_CHILD_PID_ENV, &gate.pid_report_name);
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        pump_conpty_replies(reader, writer);
        job.assign_process(child.as_raw_handle().expect("launcher handle"))
            .unwrap();
        gate.signal().unwrap();
        let started = Instant::now();
        let pid = gate
            .wait_for_grok_pid(5_000, child.as_raw_handle())
            .expect("ConPTY job-child handshake must report a PID");
        let _ = child.kill();
        assert_ne!(pid, 0);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "handshake took {:?}",
            started.elapsed()
        );
    }

    #[cfg(windows)]
    fn pump_conpty_replies(mut reader: Box<dyn Read + Send>, mut writer: Box<dyn Write + Send>) {
        thread::spawn(move || {
            let mut parser = vt100::Parser::new_with_callbacks(
                INITIAL_ROWS,
                INITIAL_COLS,
                0,
                TitleCallbacks::default(),
            );
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(read) => {
                        parser.process(&buffer[..read]);
                        let responses = std::mem::take(&mut parser.callbacks_mut().responses);
                        for response in responses {
                            if writer
                                .write_all(&response)
                                .and_then(|()| writer.flush())
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }

    #[cfg(windows)]
    #[test]
    fn wait_for_grok_pid_fails_fast_when_the_launcher_exits() {
        use std::os::windows::io::AsRawHandle;
        use std::process::{Command, Stdio};

        let gate = WindowsLaunchGate::create().unwrap();
        let mut child = Command::new("cmd.exe")
            .args(["/c", "exit", "7"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let error = gate
            .wait_for_grok_pid(10_000, Some(child.as_raw_handle()))
            .unwrap_err();
        let _ = child.wait();
        assert!(
            error.to_string().contains("exited with code 7"),
            "{error:#}"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(windows)]
    #[test]
    fn wait_for_grok_pid_still_reports_when_watching_a_live_launcher() {
        use std::os::windows::io::AsRawHandle;
        use std::process::{Command, Stdio};

        let gate = WindowsLaunchGate::create().unwrap();
        let mut child = Command::new("cmd.exe")
            .args(["/c", "ping", "-n", "8", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        report_windows_grok_pid(&gate.pid_report_name, 1234).unwrap();
        let pid = gate.wait_for_grok_pid(1_000, Some(child.as_raw_handle()));
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(pid.unwrap(), 1234);
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
        // An explicit close finalizes as soon as the process scope is verified
        // gone; a missing reader EOF must not keep the record half-open.
        assert_eq!(
            completed_phase(SessionPhase::Running, true, false, true, false, None),
            Some(SessionPhase::Stopped)
        );
        // Without an explicit close, reader EOF is still required.
        assert_eq!(
            completed_phase(SessionPhase::Running, true, false, false, true, None),
            None
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
        let item = writer_rx.recv().unwrap();
        assert_eq!(item.data, b"resume\r");
        assert_eq!(item.budget, b"resume\r".len());
        assert_eq!(session.state().unwrap().phase, SessionPhase::Running);
    }

    #[test]
    fn writer_admission_rejects_whole_input_once_byte_budget_is_full() {
        let session = test_session(SessionPhase::Idle);
        let (writer_tx, writer_rx) = sync_channel(64);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        let payload = vec![0x61; crate::protocol::MAX_WRITE_BYTES];
        for _ in 0..4 {
            session.write_raw(payload.clone()).unwrap();
        }
        assert_eq!(
            session.pending_writer_bytes.load(Ordering::Acquire),
            4 * crate::protocol::MAX_WRITE_BYTES
        );

        // A fifth maximum-size write would exceed the byte budget: the whole
        // input is rejected and no bytes enter the queue.
        let error = session.write_raw(payload.clone()).unwrap_err();
        assert!(format!("{error:#}").contains("queue is full"));
        assert_eq!(
            session.pending_writer_bytes.load(Ordering::Acquire),
            4 * crate::protocol::MAX_WRITE_BYTES
        );
        // Exactly the four accepted writes are queued; nothing partial.
        let queued = drain_writer(&writer_rx);
        assert_eq!(queued.len(), 4);
        assert!(queued.iter().all(|item| item.data == payload));
        assert!(
            queued
                .iter()
                .all(|item| item.budget == crate::protocol::MAX_WRITE_BYTES)
        );
        // Entry-count admission is still enforced for small payloads.
        let (tiny_tx, tiny_rx) = sync_channel(1);
        *session.writer_tx.lock().unwrap() = Some(tiny_tx);
        session.pending_writer_bytes.store(0, Ordering::Release);
        assert!(session.write_raw(b"a".to_vec()).is_ok());
        assert!(
            format!("{:#}", session.write_raw(b"b".to_vec()).unwrap_err())
                .contains("queue is full")
        );
        drop(tiny_rx);
    }

    #[test]
    fn writer_byte_budget_is_released_after_bytes_are_written() {
        let session = Arc::new(test_session(SessionPhase::Idle));
        let (writer_tx, writer_rx) = sync_channel(8);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        spawn_writer(Arc::clone(&session), Box::new(std::io::sink()), writer_rx);
        session.write_raw(vec![0x62; 4096]).unwrap();
        session.write_raw(vec![0x63; 8192]).unwrap();
        // The writer thread drains concurrently; the budget must never exceed
        // the sum and must settle back to zero once everything is written.
        let initial = session.pending_writer_bytes.load(Ordering::Acquire);
        assert!(initial <= 4096 + 8192);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while session.pending_writer_bytes.load(Ordering::Acquire) != 0 {
            assert!(std::time::Instant::now() < deadline, "writer never drained");
            std::thread::sleep(Duration::from_millis(1));
        }
        // Channel closed: the writer loop exits; budget stays released.
        session.close_writer();
    }

    #[test]
    fn terminal_responses_never_count_toward_the_writer_byte_budget() {
        let session = test_session(SessionPhase::Idle);
        let (writer_tx, writer_rx) = sync_channel(64);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        // Interleave the two production enqueue paths: write_raw admits bytes,
        // queue_terminal_response (vt100 response frames) must not touch the
        // admission counter at all.
        session.write_raw(vec![0x62; 2048]).unwrap();
        session.queue_terminal_response(b"\x1b[0n".to_vec());
        session.write_raw(vec![0x63; 4096]).unwrap();
        session.queue_terminal_response(b"\x1b[?1;2c".to_vec());
        session.write_raw(b"tail".to_vec()).unwrap();

        // Budget reflects only the admitted input bytes, never the responses.
        assert_eq!(
            session.pending_writer_bytes.load(Ordering::Acquire),
            2048 + 4096 + b"tail".len()
        );

        let items = drain_writer(&writer_rx);
        assert_eq!(
            items.iter().map(|item| item.budget).collect::<Vec<_>>(),
            vec![2048, 0, 4096, 0, b"tail".len()]
        );
        // Interleaved response frames still reach the PTY in arrival order.
        assert_eq!(&items[1].data, b"\x1b[0n");
        assert_eq!(&items[3].data, b"\x1b[?1;2c");
        // Budget stays unchanged once the queue is drained.
        assert_eq!(
            session.pending_writer_bytes.load(Ordering::Acquire),
            2048 + 4096 + b"tail".len()
        );
    }

    #[test]
    fn writer_mixed_input_and_terminal_response_drain_settles_budget_to_zero() {
        let session = Arc::new(test_session(SessionPhase::Idle));
        let (writer_tx, writer_rx) = sync_channel(64);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        spawn_writer(Arc::clone(&session), Box::new(std::io::sink()), writer_rx);
        session.write_raw(vec![0x62; 4096]).unwrap();
        session.queue_terminal_response(b"\x1b[0n".to_vec());
        session.write_raw(vec![0x63; 8192]).unwrap();
        session.queue_terminal_response(b"\x1b[?1;2c".to_vec());

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while session.pending_writer_bytes.load(Ordering::Acquire) != 0 {
            assert!(std::time::Instant::now() < deadline, "writer never drained");
            std::thread::sleep(Duration::from_millis(1));
        }
        // Responses contributed zero budget, so the counter can only settle to
        // exactly zero when releases never exceeded adds: no underflow.
        assert_eq!(session.pending_writer_bytes.load(Ordering::Acquire), 0);
        session.close_writer();
    }

    fn drain_writer(rx: &std::sync::mpsc::Receiver<WriterItem>) -> Vec<WriterItem> {
        let mut out = Vec::new();
        while let Ok(item) = rx.try_recv() {
            out.push(item);
        }
        out
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
    fn close_client_removes_the_lease_and_publishes_a_revision() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Exited);
        let lease = Arc::new(AtomicU64::new(1_000));
        {
            let session = host.get("gbt-test").unwrap();
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-close".to_owned());
            inner.client_lease = Some(Arc::clone(&lease));
        }
        host.registry
            .lock()
            .unwrap()
            .clients
            .insert("codex-close".to_owned(), lease);
        let seen = host.revision();
        let result = host.close_client("codex-close").unwrap();
        assert_eq!(result.closed, 1);
        assert!(
            host.registry.lock().unwrap().clients.is_empty(),
            "the client lease must be removed after a fully successful close"
        );
        // A WebUI waiter must observe a revision published after the lease
        // removal, not only the one apply_close_outcomes emitted before it.
        let advanced = host.wait_revision(seen, Duration::from_millis(50));
        assert_ne!(advanced, seen);
        assert_ne!(host.revision(), seen);
    }

    #[test]
    fn close_client_keeps_the_lease_when_the_close_fails() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let lease = Arc::new(AtomicU64::new(1_000));
        {
            let session = host.get("gbt-test").unwrap();
            let mut inner = session.inner.lock().unwrap();
            inner.client_session_id = Some("codex-fail".to_owned());
            inner.client_lease = Some(Arc::clone(&lease));
        }
        host.registry
            .lock()
            .unwrap()
            .clients
            .insert("codex-fail".to_owned(), lease);
        // Poison the close lock so the group close fails fast and
        // deterministically instead of terminating a real process scope.
        let session = host.get("gbt-test").unwrap();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            session.test_poison_close_lock();
        }));
        assert!(poisoned.is_err());
        let result = host.close_client("codex-fail").unwrap();
        assert_eq!(result.closed, 0);
        assert_eq!(result.failures.len(), 1);
        assert!(
            host.registry
                .lock()
                .unwrap()
                .clients
                .contains_key("codex-fail"),
            "a failed close must retain the client lease for a later retry"
        );
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
    fn idle_phase_projects_stale_working_activity_as_done() {
        let session = test_session(SessionPhase::Running);
        let mut inner = session.inner.lock().unwrap();
        inner.hook.activity = HookActivity::Working;

        inner.phase = SessionPhase::Running;
        assert_eq!(inner.to_state(1, false).activity, HookActivity::Working);
        inner.phase = SessionPhase::Idle;
        assert_eq!(inner.to_state(1, false).activity, HookActivity::Done);

        inner.phase = SessionPhase::Starting;
        assert_eq!(inner.to_state(1, false).activity, HookActivity::Working);
    }

    #[test]
    fn terminal_phases_override_a_stale_hook_activity() {
        let session = test_session(SessionPhase::Running);
        let mut inner = session.inner.lock().unwrap();
        inner.hook.activity = HookActivity::Working;
        for phase in [
            SessionPhase::Failed,
            SessionPhase::Exited,
            SessionPhase::Stopped,
        ] {
            inner.phase = phase;
            assert_eq!(inner.to_state(1, false).activity, HookActivity::Unknown);
        }
        inner.hook.activity = HookActivity::Waiting;
        assert_eq!(inner.to_state(1, false).activity, HookActivity::Unknown);
        inner.hook.activity = HookActivity::Done;
        assert_eq!(inner.to_state(1, false).activity, HookActivity::Done);
    }

    #[test]
    fn ctrl_c_send_preserves_bytes_and_does_not_start_or_finish_a_turn() {
        let host = test_host(TEST_PROVIDER_SESSION_ID, SessionPhase::Running);
        let session = host.get("gbt-test").unwrap();
        let (writer_tx, writer_rx) = sync_channel(1);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        session
            .apply_hook_event(hook_event(HookEventKind::PreToolUse))
            .unwrap();
        session.send("\u{3}".to_owned()).unwrap();
        assert_eq!(writer_rx.recv().unwrap().data, b"\x03");
        let state = session.state().unwrap();
        assert_eq!(state.phase, SessionPhase::Running);
        assert_eq!(state.activity, HookActivity::Working);

        let idle = test_session(SessionPhase::Idle);
        let (idle_tx, idle_rx) = sync_channel(1);
        *idle.writer_tx.lock().unwrap() = Some(idle_tx);
        idle.send("\u{3}".to_owned()).unwrap();
        assert_eq!(idle_rx.recv().unwrap().data, b"\x03");
        let state = idle.state().unwrap();
        assert_eq!(state.phase, SessionPhase::Idle);
        assert_eq!(state.activity, HookActivity::Unknown);

        session.send("next task".to_owned()).unwrap();
        assert_eq!(
            writer_rx.recv().unwrap().data,
            b"\x1b[200~next task\x1b[201~\r"
        );

        let full = test_session(SessionPhase::Idle);
        let (full_tx, _full_rx) = sync_channel(1);
        full_tx
            .try_send(WriterItem {
                data: vec![b'x'],
                budget: 1,
            })
            .unwrap();
        *full.writer_tx.lock().unwrap() = Some(full_tx);
        let before = full.state().unwrap();
        let error = full.send("\u{3}".to_owned()).unwrap_err();
        assert!(error.to_string().contains("input queue is full"));
        let after = full.state().unwrap();
        assert_eq!(after.phase, before.phase);
        assert_eq!(after.activity, before.activity);

        let closed = test_session(SessionPhase::Idle);
        let (closed_tx, closed_rx) = sync_channel(1);
        drop(closed_rx);
        *closed.writer_tx.lock().unwrap() = Some(closed_tx);
        let before = closed.state().unwrap();
        let error = closed.send("\u{3}".to_owned()).unwrap_err();
        assert!(error.to_string().contains("input channel is closed"));
        let after = closed.state().unwrap();
        assert_eq!(after.phase, before.phase);
        assert_eq!(after.activity, before.activity);
    }

    #[test]
    fn raw_write_passes_ctrl_c_through_without_starting_a_turn() {
        let session = test_session(SessionPhase::Idle);
        let (writer_tx, writer_rx) = sync_channel(1);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        session.write_raw(b"\x03".to_vec()).unwrap();
        assert_eq!(writer_rx.recv().unwrap().data, b"\x03");
        let state = session.state().unwrap();
        assert_eq!(state.phase, SessionPhase::Idle);
        assert_eq!(state.activity, HookActivity::Unknown);

        assert!(raw_input_starts_turn(b"\r"));
        assert!(raw_input_starts_turn(b"\n"));
        assert!(raw_input_starts_turn(b"new task\r"));
        assert!(!raw_input_starts_turn(b"\x03"));

        let full = test_session(SessionPhase::Idle);
        let (full_tx, _full_rx) = sync_channel(1);
        full_tx
            .try_send(WriterItem {
                data: vec![b'x'],
                budget: 1,
            })
            .unwrap();
        *full.writer_tx.lock().unwrap() = Some(full_tx);
        let before = full.state().unwrap();
        let error = full.write_raw(b"\x03".to_vec()).unwrap_err();
        assert!(error.to_string().contains("input queue is full"));
        let after = full.state().unwrap();
        assert_eq!(after.phase, before.phase);
        assert_eq!(after.activity, before.activity);

        let closed = test_session(SessionPhase::Idle);
        let (closed_tx, closed_rx) = sync_channel(1);
        drop(closed_rx);
        *closed.writer_tx.lock().unwrap() = Some(closed_tx);
        let before = closed.state().unwrap();
        let error = closed.write_raw(b"\x03".to_vec()).unwrap_err();
        assert!(error.to_string().contains("input channel is closed"));
        let after = closed.state().unwrap();
        assert_eq!(after.phase, before.phase);
        assert_eq!(after.activity, before.activity);
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
        assert!(!raw_input_starts_turn(&[0x03]));
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

    #[cfg(windows)]
    #[test]
    fn windows_process_tree_helper() {
        let Some(role) = env::var_os(WINDOWS_PROCESS_TREE_ROLE_ENV) else {
            return;
        };
        let directory = PathBuf::from(
            env::var_os(WINDOWS_PROCESS_TREE_DIR_ENV)
                .expect("process-tree helper directory must be provided"),
        );
        let role = role.to_string_lossy();
        match role.as_ref() {
            "wrapper" => {
                let gate_name = env::var_os(WINDOWS_PROCESS_TREE_GATE_ENV)
                    .expect("wrapper launch gate must be provided");
                wait_for_windows_launch_gate(&gate_name, WINDOWS_LAUNCH_HANDSHAKE_TIMEOUT_MS)
                    .expect("wrapper launch gate must open");
                let mut child = spawn_windows_process_tree_helper(
                    "short-child",
                    &directory,
                    std::process::id(),
                    None,
                );
                child.wait().expect("short-lived child must exit");
                fs::write(directory.join("wrapper-exited"), b"ready")
                    .expect("wrapper exit marker must be written");
            }
            "short-child" => {
                let root_pid = env::var(WINDOWS_PROCESS_TREE_ROOT_ENV)
                    .expect("root pid must be provided")
                    .parse::<u32>()
                    .expect("root pid must be numeric");
                let grandchild =
                    spawn_windows_process_tree_helper("grandchild", &directory, root_pid, None);
                fs::write(directory.join("child.pid"), std::process::id().to_string())
                    .expect("child pid marker must be written");
                fs::write(
                    directory.join("grandchild.pid"),
                    grandchild.id().to_string(),
                )
                .expect("grandchild pid marker must be written");
                fs::write(
                    directory.join("ready"),
                    format!("{root_pid} {} {}\n", std::process::id(), grandchild.id()),
                )
                .expect("ready marker must be written");
                drop(grandchild);
            }
            "grandchild" => thread::sleep(WINDOWS_PROCESS_TREE_TIMEOUT),
            unexpected => panic!("unexpected Windows process-tree helper role: {unexpected}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_session_close_contains_descendant_after_intermediates_exit() {
        use std::os::windows::io::AsRawHandle;

        let directory = temporary_test_directory("windows-job-tree");
        fs::create_dir_all(&directory).unwrap();
        let gate = WindowsLaunchGate::create().unwrap();
        let job = WindowsJob::create().unwrap();
        let root = spawn_windows_process_tree_helper(
            "wrapper",
            &directory,
            0,
            Some(gate.event_name.as_os_str()),
        );
        let root_pid = root.id();
        job.assign_process(root.as_raw_handle()).unwrap();
        gate.signal().unwrap();
        let mut cleanup = WindowsProcessTreeCleanup::new(root, directory.clone());

        let pids = wait_for_windows_process_tree_ready(&directory.join("ready"));
        cleanup.remember(&pids);
        assert_eq!(pids[0], root_pid, "ready marker reported another root");
        assert_ne!(pids[0], pids[1]);
        assert_ne!(pids[1], pids[2]);
        wait_for_windows_marker(&directory.join("wrapper-exited"));
        // The marker only proves the wrapper wrote it; the wrapper and
        // short-lived child processes may still be tearing down. Wait for
        // both to actually exit before asserting Gone, while the grandchild
        // and the job must remain alive.
        wait_for_windows_intermediates_exit([pids[0], pids[1]]);
        assert_eq!(process_alive(pids[2]), ScopeAlive::Alive);
        assert_eq!(job.process_scope_alive(), ScopeAlive::Alive);

        let host = windows_process_host(root_pid, job);
        assert!(host.close("gbt-test").unwrap());
        wait_for_windows_process_tree_exit(pids);
        assert!(host.show("gbt-test").is_err());
        assert!(
            host.close("gbt-test").unwrap(),
            "tombstone re-close must succeed"
        );
        cleanup.disarm();
    }

    /// Spawn the wrapper/short-child/grandchild process tree, assign the
    /// wrapper to a fresh Job Object, and wait until the wrapper and its
    /// short-lived child have exited so only the long-lived grandchild remains
    /// in the job. Returns the job, the three pids, and the armed cleanup
    /// guard.
    #[cfg(windows)]
    fn spawn_windows_tree_in_job(label: &str) -> (WindowsJob, [u32; 3], WindowsProcessTreeCleanup) {
        use std::os::windows::io::AsRawHandle;

        let directory = temporary_test_directory(label);
        fs::create_dir_all(&directory).unwrap();
        let gate = WindowsLaunchGate::create().unwrap();
        let job = WindowsJob::create().unwrap();
        let root = spawn_windows_process_tree_helper(
            "wrapper",
            &directory,
            0,
            Some(gate.event_name.as_os_str()),
        );
        job.assign_process(root.as_raw_handle()).unwrap();
        gate.signal().unwrap();
        let mut cleanup = WindowsProcessTreeCleanup::new(root, directory.clone());
        let pids = wait_for_windows_process_tree_ready(&directory.join("ready"));
        cleanup.remember(&pids);
        wait_for_windows_marker(&directory.join("wrapper-exited"));
        wait_for_windows_intermediates_exit([pids[0], pids[1]]);
        (job, pids, cleanup)
    }

    /// The Windows escalation ladder must match its documented semantics: Hup
    /// and Term are wait-only phases that never TerminateJobObject, and only
    /// Kill force-terminates the whole job scope.
    #[cfg(windows)]
    #[test]
    fn windows_job_terminate_waits_on_hup_and_term_until_kill() {
        let (job, pids, mut cleanup) = spawn_windows_tree_in_job("windows-job-phases");
        assert_eq!(job.process_scope_alive(), ScopeAlive::Alive);

        job.terminate(TerminationLevel::Hup).unwrap();
        assert_eq!(
            job.process_scope_alive(),
            ScopeAlive::Alive,
            "Hup is a wait-only phase and must not terminate the job"
        );
        job.terminate(TerminationLevel::Term).unwrap();
        assert_eq!(
            job.process_scope_alive(),
            ScopeAlive::Alive,
            "Term is a wait-only phase and must not terminate the job"
        );

        job.terminate(TerminationLevel::Kill).unwrap();
        wait_for_windows_process_tree_exit(pids);
        assert_eq!(job.process_scope_alive(), ScopeAlive::Gone);
        cleanup.disarm();
    }

    /// The bare-process fallback (`terminate_windows_process`) must mirror the
    /// job semantics: Hup and Term wait, Kill force-terminates.
    #[cfg(windows)]
    #[test]
    fn windows_bare_process_terminate_waits_on_hup_and_term_until_kill() {
        let directory = temporary_test_directory("windows-bare-process-phases");
        fs::create_dir_all(&directory).unwrap();
        let child = spawn_windows_process_tree_helper("grandchild", &directory, 0, None);
        let pid = child.id();
        let mut cleanup = WindowsProcessTreeCleanup::new(child, directory.clone());

        terminate_windows_process(pid, TerminationLevel::Hup).unwrap();
        assert_eq!(
            process_alive(pid),
            ScopeAlive::Alive,
            "Hup is a wait-only phase and must not terminate the process"
        );
        terminate_windows_process(pid, TerminationLevel::Term).unwrap();
        assert_eq!(
            process_alive(pid),
            ScopeAlive::Alive,
            "Term is a wait-only phase and must not terminate the process"
        );

        terminate_windows_process(pid, TerminationLevel::Kill).unwrap();
        let deadline = Instant::now() + WINDOWS_PROCESS_TREE_TIMEOUT;
        while process_alive(pid) != ScopeAlive::Gone {
            assert!(
                Instant::now() < deadline,
                "bare Windows process {pid} survived TerminateProcess"
            );
            thread::sleep(Duration::from_millis(10));
        }
        cleanup.disarm();
    }

    /// `shutdown_all` must force-terminate the Job Object scope even when the
    /// bounded group close times out waiting for the close lock, so server
    /// exit never leaves a Windows Grok descendant behind.
    #[cfg(windows)]
    #[test]
    fn windows_shutdown_all_force_kills_the_job_scope_after_a_close_timeout() {
        let (job, pids, mut cleanup) = spawn_windows_tree_in_job("windows-shutdown-all-timeout");
        let host = windows_process_host(pids[0], job);
        let session = host.get("gbt-test").unwrap();
        // Hold the close lock so close_attempt spins until the shared deadline
        // and reports Timeout; shutdown_all's final forced pass must still
        // terminate the job scope.
        let _held = session.test_hold_close_lock();

        host.shutdown_all().unwrap();
        wait_for_windows_process_tree_exit(pids);
        cleanup.disarm();
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn all_zombie_scope_requires_stable_membership() {
        assert_eq!(
            stable_process_group_scan_result(
                (ScopeAlive::Gone, vec![41]),
                (ScopeAlive::Gone, vec![41]),
            ),
            ScopeAlive::Gone
        );
        assert_eq!(
            stable_process_group_scan_result(
                (ScopeAlive::Gone, vec![41]),
                (ScopeAlive::Gone, vec![41, 42]),
            ),
            ScopeAlive::Unknown
        );
        assert_eq!(
            stable_process_group_scan_result(
                (ScopeAlive::Gone, vec![41]),
                (ScopeAlive::Alive, vec![41, 42]),
            ),
            ScopeAlive::Alive
        );
    }

    #[test]
    #[cfg(unix)]
    fn unix_scope_probe_tracks_descendant_until_group_kill() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let directory = temporary_test_directory("unix-process-group");
        fs::create_dir_all(&directory).unwrap();
        let descendant_pid_path = directory.join("descendant-pid");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(trap '' HUP TERM; sleep 30) & printf '%s\\n' \"$!\" > \"$1\"; exit 0",
                "grok-bridge-unix-process-group-test",
            ])
            .arg(&descendant_pid_path);
        // portable-pty uses setsid for the child. Mirror that ownership model
        // so the root pid is also the process-group id used by the probe.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut root = command.spawn().unwrap();
        let pgid = root.id();
        root.wait().unwrap();
        let descendant_pid = fs::read_to_string(&descendant_pid_path)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();

        assert_eq!(process_scope_alive(pgid, true), ScopeAlive::Alive);
        let deadline = default_close_deadline();
        let mut scope_gone = false;
        while Instant::now() < deadline {
            if process_scope_alive(pgid, true) == ScopeAlive::Gone {
                scope_gone = true;
                break;
            }
            send_termination_signal(pgid, TerminationLevel::Kill).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            scope_gone,
            "process group {pgid} remained live after repeated SIGKILL"
        );
        assert_eq!(
            unix_test_process_alive(descendant_pid),
            ScopeAlive::Gone,
            "tracked descendant {descendant_pid} remained executable"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    /// Spawn a setsid'd shell that exits immediately while a descendant keeps
    /// running in the same process group, ignoring HUP and TERM. Returns the
    /// group id (the leader pid) and the descendant pid.
    #[cfg(unix)]
    fn spawn_unix_process_group_with_descendant(
        directory: &Path,
    ) -> (std::process::Child, u32, u32) {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let descendant_pid_path = directory.join("descendant-pid");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(trap '' HUP TERM; sleep 30) & printf '%s\\n' \"$!\" > \"$1\"; exit 0",
                "grok-bridge-shutdown-all-test",
            ])
            .arg(&descendant_pid_path);
        // portable-pty uses setsid for the child. Mirror that ownership model
        // so the root pid is also the process-group id used by the probe.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut root = command.spawn().unwrap();
        let pgid = root.id();
        root.wait().unwrap();
        let descendant_pid = fs::read_to_string(&descendant_pid_path)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        (root, pgid, descendant_pid)
    }

    /// `shutdown_all` must force-kill and reap a surviving process group even
    /// when the bounded group close fails (poisoned close lock): server exit
    /// must never orphan a Unix Grok process or its descendants.
    #[test]
    #[cfg(unix)]
    fn shutdown_all_force_kills_the_process_group_after_a_close_failure() {
        let directory = temporary_test_directory("shutdown-all-failed");
        fs::create_dir_all(&directory).unwrap();
        let (mut root, pgid, descendant_pid) = spawn_unix_process_group_with_descendant(&directory);

        let host = unix_process_host(pgid);
        let session = host.get("gbt-test").unwrap();
        // Poison the close lock so close_attempt fails fast and deterministically;
        // shutdown_all must still terminate and reap the whole owned scope.
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            session.test_poison_close_lock();
        }));
        assert!(poisoned.is_err());

        host.shutdown_all().unwrap();
        assert_eq!(
            process_scope_alive(pgid, true),
            ScopeAlive::Gone,
            "shutdown_all must reap the whole process group after a failed close"
        );
        assert_eq!(
            unix_test_process_alive(descendant_pid),
            ScopeAlive::Gone,
            "shutdown_all must reap the descendant after a failed close"
        );
        let _ = root.kill();
        let _ = root.wait();
        fs::remove_dir_all(directory).unwrap();
    }

    /// `shutdown_all` must force-kill and reap a surviving process group even
    /// when the bounded group close times out waiting for the close lock.
    #[test]
    #[cfg(unix)]
    fn shutdown_all_force_kills_the_process_group_after_a_close_timeout() {
        let directory = temporary_test_directory("shutdown-all-timeout");
        fs::create_dir_all(&directory).unwrap();
        let (mut root, pgid, descendant_pid) = spawn_unix_process_group_with_descendant(&directory);

        let host = unix_process_host(pgid);
        let session = host.get("gbt-test").unwrap();
        // Hold the close lock so close_attempt spins until the shared deadline
        // and reports Timeout; the session keeps ownership and shutdown_all's
        // final forced pass must still terminate and reap the whole group.
        let _held = session.test_hold_close_lock();

        host.shutdown_all().unwrap();
        assert_eq!(
            process_scope_alive(pgid, true),
            ScopeAlive::Gone,
            "shutdown_all must reap the whole process group after a close timeout"
        );
        assert_eq!(
            unix_test_process_alive(descendant_pid),
            ScopeAlive::Gone,
            "shutdown_all must reap the descendant after a close timeout"
        );
        let _ = root.kill();
        let _ = root.wait();
        fs::remove_dir_all(directory).unwrap();
    }

    /// Every surviving session must get its own forced-termination window:
    /// with two independent process groups whose closes all fail, `shutdown_all`
    /// must Kill and reap both instead of sharing one deadline that a slow first
    /// scope could exhaust.
    #[test]
    #[cfg(unix)]
    fn shutdown_all_force_kills_every_group_when_all_closes_fail() {
        let directory = temporary_test_directory("shutdown-all-multi");
        fs::create_dir_all(&directory).unwrap();
        let first_dir = directory.join("first");
        let second_dir = directory.join("second");
        fs::create_dir_all(&first_dir).unwrap();
        fs::create_dir_all(&second_dir).unwrap();
        let (mut root_a, pgid_a, descendant_a) =
            spawn_unix_process_group_with_descendant(&first_dir);
        let (mut root_b, pgid_b, descendant_b) =
            spawn_unix_process_group_with_descendant(&second_dir);
        assert_ne!(pgid_a, pgid_b);
        let host = unix_process_hosts(&[pgid_a, pgid_b]);
        // Fail both close paths fast and deterministically; shutdown_all must
        // still force-kill and reap both owned scopes.
        for handle in ["gbt-1", "gbt-2"] {
            let session = host.get(handle).unwrap();
            let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                session.test_poison_close_lock();
            }));
            assert!(poisoned.is_err(), "{handle} close lock must be poisoned");
        }

        host.shutdown_all().unwrap();
        assert_eq!(process_scope_alive(pgid_a, true), ScopeAlive::Gone);
        assert_eq!(process_scope_alive(pgid_b, true), ScopeAlive::Gone);
        assert_eq!(unix_test_process_alive(descendant_a), ScopeAlive::Gone);
        assert_eq!(unix_test_process_alive(descendant_b), ScopeAlive::Gone);
        let _ = root_a.kill();
        let _ = root_a.wait();
        let _ = root_b.kill();
        let _ = root_b.wait();
        fs::remove_dir_all(directory).unwrap();
    }

    /// True once `pid` has exited but is still an unreaped zombie held by this
    /// test process. Linux reads the state field from /proc; macOS reports no
    /// process info for an exited-but-unreaped child (proc_pidinfo returns 0).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn test_unreaped_zombie(pid: u32) -> bool {
        #[cfg(target_os = "linux")]
        {
            match fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(stat) => stat
                    .rsplit_once(") ")
                    .is_some_and(|(_, fields)| fields.split_whitespace().next() == Some("Z")),
                Err(_) => false,
            }
        }
        #[cfg(target_os = "macos")]
        {
            let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
            let read = unsafe {
                libc::proc_pidinfo(
                    pid as libc::pid_t,
                    libc::PROC_PIDTBSDINFO,
                    0,
                    std::ptr::from_mut(&mut info).cast(),
                    std::mem::size_of_val(&info) as libc::c_int,
                )
            };
            read == 0
        }
    }

    /// A full terminal-response queue is transient backpressure: the error is
    /// recorded, but the owned process scope must never be escalated.
    #[test]
    #[cfg(unix)]
    fn full_terminal_response_queue_does_not_terminate_the_process_scope() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let mut root = Command::new("/bin/sh");
        root.args(["-c", "exec sleep 60", "grok-bridge-queue-backpressure-test"]);
        unsafe {
            root.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut root = root.spawn().unwrap();
        let pgid = root.id();

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && process_scope_alive(pgid, true) != ScopeAlive::Alive {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(process_scope_alive(pgid, true), ScopeAlive::Alive);

        // A zero-capacity writer queue makes the next enqueue report Full.
        let mut session = test_session(SessionPhase::Running);
        session.scope_id = pgid;
        let (writer_tx, _writer_rx) = sync_channel(0);
        *session.writer_tx.lock().unwrap() = Some(writer_tx);

        session.queue_terminal_response(vec![0x1b; 64]);
        assert!(
            session
                .inner
                .lock()
                .unwrap()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("queue is full")),
            "queue backpressure must be recorded for observers"
        );
        assert_eq!(session.state().unwrap().phase, SessionPhase::Running);
        assert_eq!(
            process_scope_alive(pgid, true),
            ScopeAlive::Alive,
            "queue backpressure must not escalate the owned process scope"
        );

        // Cleanup: the scope is killed only by an explicit termination signal,
        // then reaped so the probe can observe the group empty.
        send_termination_signal(pgid, TerminationLevel::Kill).unwrap();
        let _ = root.wait();
        let deadline = default_close_deadline();
        let mut scope_gone = false;
        while Instant::now() < deadline {
            if process_scope_alive(pgid, true) == ScopeAlive::Gone {
                scope_gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(scope_gone, "process group {pgid} survived cleanup SIGKILL");
    }

    /// The Linux fast kill(0) probe must answer Alive without the /proc scan,
    /// while the stable probe still distinguishes an all-zombie group; macOS
    /// keeps its existing always-scan behavior (an unreaped zombie answers
    /// kill(0) with EPERM, mapped to Alive as before).
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn fast_scope_probe_skips_the_scan_outside_the_stable_confirmation_window() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let mut child = Command::new("/bin/sh");
        child.args(["-c", "exec sleep 60", "grok-bridge-fast-probe-test"]);
        unsafe {
            child.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = child.spawn().unwrap();
        let pgid = child.id();

        // Live group: both probe modes agree the scope is alive.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && process_scope_alive(pgid, true) != ScopeAlive::Alive {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(process_scope_alive(pgid, true), ScopeAlive::Alive);
        assert_eq!(process_scope_alive(pgid, false), ScopeAlive::Alive);

        // SIGKILL leaves the child as an unreaped zombie held by this test.
        send_termination_signal(pgid, TerminationLevel::Kill).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !test_unreaped_zombie(pgid) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            test_unreaped_zombie(pgid),
            "process {pgid} never became a zombie"
        );
        #[cfg(target_os = "linux")]
        {
            // The fast probe answers from kill(0) alone: Alive when the
            // zombie group still answers the existence probe, Gone when it
            // no longer does — never a /proc walk.
            let fast = process_scope_alive(pgid, false);
            let kill0_succeeds = unsafe { libc::kill(-(pgid as libc::pid_t), 0) } == 0;
            assert_eq!(
                fast == ScopeAlive::Alive,
                kill0_succeeds,
                "fast probe must mirror the kill(0) result"
            );
            // The stable probe certifies the all-zombie group as gone.
            assert_eq!(process_scope_alive(pgid, true), ScopeAlive::Gone);
        }
        #[cfg(target_os = "macos")]
        assert_eq!(
            process_scope_alive(pgid, true),
            ScopeAlive::Alive,
            "macOS keeps mapping an unreaped zombie to Alive"
        );

        // Reaping the zombie empties the group: both probes now report Gone.
        child.wait().unwrap();
        assert_eq!(process_scope_alive(pgid, true), ScopeAlive::Gone);
        assert_eq!(process_scope_alive(pgid, false), ScopeAlive::Gone);
    }

    #[cfg(target_os = "macos")]
    fn unix_test_process_alive(pid: u32) -> ScopeAlive {
        let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
        let read = unsafe {
            libc::proc_pidinfo(
                pid as libc::pid_t,
                libc::PROC_PIDTBSDINFO,
                0,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of_val(&info) as libc::c_int,
            )
        };
        if read == std::mem::size_of_val(&info) as libc::c_int {
            if info.pbi_status == libc::SZOMB {
                ScopeAlive::Gone
            } else {
                ScopeAlive::Alive
            }
        } else if unsafe { libc::kill(pid as libc::pid_t, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            ScopeAlive::Gone
        } else {
            ScopeAlive::Unknown
        }
    }

    #[cfg(target_os = "linux")]
    fn unix_test_process_alive(pid: u32) -> ScopeAlive {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => match stat
                .rsplit_once(") ")
                .and_then(|(_, fields)| fields.split_whitespace().next())
            {
                Some("Z" | "X") => ScopeAlive::Gone,
                Some(_) => ScopeAlive::Alive,
                None => ScopeAlive::Unknown,
            },
            Err(error) if error.kind() == ErrorKind::NotFound => ScopeAlive::Gone,
            Err(_) => ScopeAlive::Unknown,
        }
    }

    #[test]
    fn tombstone_eviction_keeps_recent_handles() {
        let mut registry = SessionRegistry {
            accepting: true,
            sessions: HashMap::new(),
            provider_sessions: HashMap::new(),
            clients: HashMap::new(),
            closed: HashMap::new(),
        };
        let now = now_millis();
        for index in 0..CLOSED_TOMBSTONE_CAP {
            registry.closed.insert(
                format!("old-{index}"),
                now.saturating_sub(CLOSED_TOMBSTONE_TTL_MS + 1),
            );
        }
        registry.closed.insert("recent".to_owned(), now);
        registry.remember_closed("newest");
        assert!(registry.was_closed("recent"));
        assert!(registry.was_closed("newest"));
        assert!(!registry.was_closed("old-0"));
    }
}
