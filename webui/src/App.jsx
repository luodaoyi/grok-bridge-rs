import {
  ChevronsDownUp,
  ChevronsUpDown,
  ChevronRight,
  CircleStop,
  ExternalLink,
  Moon,
  MonitorCog,
  RefreshCw,
  Server,
  Sun,
  Trash2,
} from "lucide-react";
import {
  Component,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  closeClientRequest,
  closeOwnerRequest,
  closeSessionRequest,
  getSessions,
} from "./api.js";
import {
  activityLabel,
  activityOf,
  ageLabel,
  clientStateLabel,
  groupSessions,
  groupSummary,
  remainingLabel,
  sessionsSignature,
  sessionStats,
} from "./sessions.js";
import { applyTheme, readTheme } from "./theme.js";

const terminalScroll = new Map();
const POLL_MS = 2000;
const GITHUB_URL = "https://github.com/luodaoyi/grok-bridge-rs";

const buttonBase =
  "inline-flex min-h-8 items-center justify-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)] disabled:cursor-not-allowed disabled:opacity-50";
const secondaryButton = `${buttonBase} border-[var(--button-border)] bg-[var(--button-bg)] text-[var(--button-text)] hover:border-[var(--button-hover-border)] hover:bg-[var(--button-hover-bg)]`;
const dangerButton = `${buttonBase} border-[var(--danger-border)] bg-[var(--danger-bg)] text-[var(--danger-text)] hover:border-[var(--danger-hover-border)] hover:bg-[var(--danger-hover-bg)]`;

const themeOptions = [
  { value: "auto", label: "自动", Icon: MonitorCog },
  { value: "light", label: "浅色", Icon: Sun },
  { value: "dark", label: "深色", Icon: Moon },
];

function ThemeSwitcher() {
  const [theme, setTheme] = useState(readTheme);
  const mediaQuery = useMemo(
    () => window.matchMedia("(prefers-color-scheme: dark)"),
    [],
  );

  useEffect(() => {
    applyTheme(theme, mediaQuery);
  }, [mediaQuery, theme]);

  useEffect(() => {
    const handleChange = () => {
      if (document.documentElement.dataset.theme === "auto") {
        applyTheme("auto", mediaQuery);
      }
    };
    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [mediaQuery]);

  return (
    <div
      className="inline-flex rounded-md border border-[var(--button-border)] bg-[var(--theme-control-bg)] p-0.5"
      role="group"
      aria-label="颜色主题"
    >
      {themeOptions.map(({ value, label, Icon }) => (
        <button
          key={value}
          type="button"
          className={`inline-flex min-h-7 items-center gap-1 rounded-sm px-2 py-1 text-[11px] font-semibold focus-visible:outline-2 focus-visible:outline-[var(--focus)] ${
            theme === value
              ? "bg-[var(--theme-active-bg)] text-[var(--theme-active-text)]"
              : "text-[var(--muted)] hover:text-[var(--text)]"
          }`}
          aria-pressed={theme === value}
          title={`${label}主题`}
          onClick={() => setTheme(value)}
        >
          <Icon aria-hidden="true" size={13} strokeWidth={2} />
          <span>{label}</span>
        </button>
      ))}
    </div>
  );
}

function Stat({ label, value }) {
  return (
    <div className="min-w-0 border-r border-[var(--border)] px-3 py-2 last:border-r-0 max-md:nth-[2n]:border-r-0 max-md:nth-[-n+4]:border-b max-md:last:col-span-2 max-md:last:border-b-0">
      <span className="block text-[11px] font-medium text-[var(--subtle)]">
        {label}
      </span>
      <strong className="mt-0.5 block text-xl leading-none text-[var(--strong)]">
        {value}
      </strong>
    </div>
  );
}

function SessionMeta({ label, children, wide = false, title }) {
  return (
    <div className={`min-w-0 ${wide ? "flex-1 basis-80" : "basis-32"}`}>
      <span className="mb-0.5 block text-[10px] font-bold text-[var(--faint)]">
        {label}
      </span>
      <code
        className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-[var(--code-text)]"
        title={title}
      >
        {children}
      </code>
    </div>
  );
}

function captureTerminal(terminal) {
  return terminal
    ? {
        top: terminal.scrollTop,
        left: terminal.scrollLeft,
        stickToBottom:
          terminal.scrollHeight - terminal.scrollTop - terminal.clientHeight < 8,
      }
    : null;
}

function restoreTerminal(terminal, saved) {
  if (!terminal || !saved) return;
  terminal.scrollLeft = saved.left;
  terminal.scrollTop = saved.stickToBottom ? terminal.scrollHeight : saved.top;
}

class Terminal extends Component {
  terminal = null;

  remember = () => {
    const saved = captureTerminal(this.terminal);
    if (saved) terminalScroll.set(this.props.id, saved);
  };

  componentDidMount() {
    restoreTerminal(this.terminal, terminalScroll.get(this.props.id));
  }

  getSnapshotBeforeUpdate() {
    return captureTerminal(this.terminal);
  }

  componentDidUpdate(_previousProps, _previousState, snapshot) {
    restoreTerminal(this.terminal, snapshot);
    this.remember();
  }

  componentWillUnmount() {
    this.remember();
  }

  render() {
    const { id, screen } = this.props;
    return (
      <pre
        ref={(terminal) => {
          this.terminal = terminal;
        }}
        data-terminal={id}
        tabIndex="0"
        aria-label={`${id} 的终端画面`}
        onScroll={this.remember}
        className="max-h-[460px] min-h-36 w-full overflow-auto rounded-md border border-[var(--terminal-border)] bg-[var(--terminal-bg)] p-3 font-mono text-[13px] leading-5 whitespace-pre text-[var(--terminal-text)] [tab-size:4] focus-visible:outline-2 focus-visible:outline-[var(--focus)]"
      >
        {screen || "(终端尚无输出)"}
      </pre>
    );
  }
}

class AppErrorBoundary extends Component {
  state = { error: null };

  static getDerivedStateFromError(error) {
    return { error };
  }

  componentDidCatch(error) {
    console.error("WebUI render error:", error);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="mx-auto max-w-xl px-4 py-16 text-center">
          <h1 className="text-lg font-bold text-[var(--strong)]">
            页面渲染异常
          </h1>
          <p className="mt-2 text-sm text-[var(--muted)]">
            {String(this.state.error?.message || this.state.error)}
          </p>
          <button
            className={`${secondaryButton} mt-4`}
            type="button"
            onClick={() => {
              this.setState({ error: null });
              window.location.reload();
            }}
          >
            重新加载
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

function SessionRow({ session, collapsed, onToggle, onClose, busy }) {
  const activity = activityOf(session);
  const title = session.title || session.session;
  return (
    <details
      className="session group/session min-w-0 border-t border-[var(--border)] bg-[var(--session-bg)] first:border-t-0 open:bg-[var(--session-bg)]"
      data-session={session.session}
      open={!collapsed}
      onToggle={(event) => onToggle(event.currentTarget.open)}
    >
      <summary className="flex min-h-12 cursor-pointer list-none items-center gap-2 px-3 py-2.5 marker:hidden focus-visible:outline-2 focus-visible:outline-[var(--focus)] sm:px-4">
        <ChevronRight
          aria-hidden="true"
          className="shrink-0 text-[var(--accent)] transition-transform group-open/session:rotate-90"
          size={16}
        />
        <span
          className={`badge badge-${activity} inline-flex shrink-0 rounded-full px-2 py-0.5 text-[11px] font-bold`}
          title={`PTY 阶段：${session.phase}`}
        >
          {activityLabel(activity)}
        </span>
        <div className="min-w-0 flex-1">
          <h3
            className="overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold text-[var(--text)]"
            title={title}
          >
            {title}
          </h3>
          {collapsed ? (
            <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-[var(--subtle)]">
              {session.session}
              {session.tool_name ? ` · ${session.tool_name}` : ""}
              {session.waiting_reason ? ` · 等待：${session.waiting_reason}` : ""}
            </p>
          ) : null}
        </div>
        <button
          className={`${dangerButton} shrink-0`}
          type="button"
          disabled={busy}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onClose(session.session);
          }}
        >
          <CircleStop aria-hidden="true" size={14} />
          关闭 Grok
        </button>
      </summary>
      {collapsed ? null : (
        <div className="px-3 pb-3 sm:px-4">
          <div className="my-1 flex min-w-0 flex-wrap items-start gap-x-5 gap-y-2">
            <SessionMeta label="会话 ID" title={session.session}>
              {session.session}
            </SessionMeta>
            <SessionMeta label="进程">
              PID {session.process_id ?? "-"}
            </SessionMeta>
            <SessionMeta label="最近更新">
              <Age updatedAt={session.updated_at_ms} />
            </SessionMeta>
            <SessionMeta label="Codex 连接">
              {clientStateLabel(session.client_state)}
            </SessionMeta>
            {session.auto_close_at_ms ? (
              <SessionMeta label="自动清理倒计时">
                {remainingLabel(session.auto_close_at_ms)}
              </SessionMeta>
            ) : null}
            {session.hook_event ? (
              <SessionMeta label="最近 Hook">{session.hook_event}</SessionMeta>
            ) : null}
            {session.tool_name ? (
              <SessionMeta label="当前工具" title={session.tool_name}>
                {session.tool_name}
              </SessionMeta>
            ) : null}
            <SessionMeta label="工作目录" title={session.cwd} wide>
              {session.cwd}
            </SessionMeta>
          </div>
          {session.waiting_reason ? (
            <p className="mb-3 rounded-r-md border-l-[3px] border-[var(--waiting-note-border)] bg-[var(--waiting-note-bg)] px-2.5 py-2 text-xs text-[var(--waiting-text)]">
              等待 Codex：{session.waiting_reason}
            </p>
          ) : null}
          <Terminal id={session.session} screen={session.screen} />
        </div>
      )}
    </details>
  );
}

function Age({ updatedAt }) {
  return ageLabel(updatedAt);
}

function OwnerGroup({
  groupKey,
  owner,
  clientSessionId,
  sessions,
  collapsed,
  collapsedSessions,
  onToggle,
  onToggleSession,
  onCloseGroup,
  onCloseSession,
  busy,
}) {
  const displayOwner = owner ?? "未标记的 Codex 对话";
  return (
    <details
      className="group overflow-hidden border-y border-[var(--border)] bg-[var(--group-bg)] open:border-[var(--open-border)]"
      data-owner-key={groupKey}
      open={!collapsed}
      onToggle={(event) => onToggle(event.currentTarget.open)}
    >
      <summary className="flex min-h-14 cursor-pointer list-none items-center gap-3 px-3 py-2.5 marker:hidden focus-visible:outline-2 focus-visible:outline-[var(--focus)] sm:px-4">
        <ChevronRight
          aria-hidden="true"
          className="shrink-0 text-[var(--accent)] transition-transform group-open:rotate-90"
          size={18}
        />
        <div className="min-w-0 flex-1">
          <h2
            className="overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold text-[var(--strong)]"
            title={displayOwner}
          >
            {displayOwner}
          </h2>
          <p className="mt-0.5 text-[11px] text-[var(--subtle)]">
            {groupSummary(sessions)}
          </p>
        </div>
        <span className="shrink-0 rounded-full border border-[var(--pill-border)] bg-[var(--pill-bg)] px-2 py-0.5 text-[11px] font-bold text-[var(--accent-text)]">
          {sessions.length} 个 Grok
        </span>
      </summary>
      {collapsed ? null : (
        <div className="border-t border-[var(--group-body-border)] bg-[var(--group-body-bg)]">
          {owner == null && clientSessionId == null ? null : (
            <div className="flex justify-end border-b border-[var(--group-body-border)] px-3 py-2 sm:px-4">
              <button
                className={`${dangerButton} max-sm:w-full`}
                type="button"
                disabled={busy}
                onClick={() =>
                  onCloseGroup(owner, clientSessionId, sessions.length)
                }
              >
                <Trash2 aria-hidden="true" size={14} />
                关闭该 Codex 全部 Grok
              </button>
            </div>
          )}
          {sessions.map((session) => (
            <SessionRow
              key={session.session}
              session={session}
              collapsed={collapsedSessions.has(session.session)}
              onToggle={(open) => onToggleSession(session.session, open)}
              onClose={onCloseSession}
              busy={busy}
            />
          ))}
        </div>
      )}
    </details>
  );
}

function errorMessage(error) {
  if (!error) return "unknown error";
  if (error.name === "AbortError") return "请求超时，正在自动重试";
  return error.message || String(error);
}

export default function App() {
  return (
    <AppErrorBoundary>
      <AppShell />
    </AppErrorBoundary>
  );
}

function AppShell() {
  const [sessions, setSessions] = useState([]);
  const [loading, setLoading] = useState(false);
  const [connected, setConnected] = useState(true);
  const [lastUpdated, setLastUpdated] = useState(null);
  const [notice, setNotice] = useState(null);
  const [collapsedOwners, setCollapsedOwners] = useState(() => new Set());
  const [collapsedSessions, setCollapsedSessions] = useState(() => new Set());
  const signatureRef = useRef(null);
  const loadingRef = useRef(false);
  const mountedRef = useRef(true);

  const groups = useMemo(() => groupSessions(sessions), [sessions]);
  const stats = useMemo(() => sessionStats(sessions), [sessions]);

  const load = useCallback(async ({ quiet = false } = {}) => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    if (!quiet) setLoading(true);
    try {
      const nextSessions = await getSessions();
      if (!mountedRef.current) return;
      const signature = sessionsSignature(nextSessions);
      if (signature !== signatureRef.current) {
        signatureRef.current = signature;
        setSessions(nextSessions);
      }
      setConnected(true);
      setLastUpdated(new Date());
      setNotice((current) =>
        current?.tone === "error" &&
        String(current.text || "").startsWith("读取 Runtime 状态失败")
          ? null
          : current,
      );
    } catch (error) {
      if (!mountedRef.current) return;
      setConnected(false);
      setNotice({
        tone: "error",
        text: `读取 Runtime 状态失败：${errorMessage(error)}（将自动重试）`,
      });
    } finally {
      loadingRef.current = false;
      if (mountedRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    let cancelled = false;
    let timer = 0;

    const tick = async () => {
      if (cancelled) return;
      try {
        await load({ quiet: true });
      } catch (error) {
        // load already swallows request errors; keep the loop alive.
        console.warn("poll tick failed:", error);
      }
      if (!cancelled) {
        timer = window.setTimeout(tick, POLL_MS);
      }
    };

    tick();
    return () => {
      cancelled = true;
      mountedRef.current = false;
      window.clearTimeout(timer);
    };
  }, [load]);

  const closeSession = useCallback(
    async (id) => {
      if (loadingRef.current) return;
      if (!window.confirm(`确认关闭 ${id} 及其 Grok 进程？`)) return;
      loadingRef.current = true;
      setLoading(true);
      try {
        await closeSessionRequest(id);
        setNotice({ tone: "info", text: `已关闭 Grok 会话 ${id}。` });
      } catch (error) {
        setNotice({
          tone: "error",
          text: `关闭失败：${errorMessage(error)}`,
        });
      } finally {
        loadingRef.current = false;
        try {
          await load();
        } catch {
          setLoading(false);
        }
      }
    },
    [load],
  );

  const closeGroup = useCallback(
    async (owner, clientSessionId, count) => {
      if (loadingRef.current) return;
      const displayOwner = owner ?? clientSessionId;
      if (
        !window.confirm(
          `确认关闭 Codex“${displayOwner}”下的全部 ${count} 个 Grok 会话？`,
        )
      ) {
        return;
      }
      loadingRef.current = true;
      setLoading(true);
      try {
        const result = clientSessionId
          ? await closeClientRequest(clientSessionId)
          : await closeOwnerRequest(owner);
        if (result.matched === 0) {
          setNotice({
            tone: "info",
            text: "该 Codex 分组已没有活跃 Grok 会话。",
          });
        } else if (
          result.failures?.length ||
          result.closed !== result.matched
        ) {
          setNotice({
            tone: "error",
            text: `已关闭 ${result.closed}/${result.matched} 个会话；失败：${(result.failures || []).join("、")}`,
          });
        } else {
          setNotice({
            tone: "info",
            text: `已关闭 Codex“${displayOwner}”下的全部 ${result.closed} 个 Grok 会话。`,
          });
        }
      } catch (error) {
        setNotice({
          tone: "error",
          text: `关闭失败：${errorMessage(error)}`,
        });
      } finally {
        loadingRef.current = false;
        try {
          await load();
        } catch {
          setLoading(false);
        }
      }
    },
    [load],
  );

  const toggleOwner = useCallback((key, open) => {
    setCollapsedOwners((current) => {
      const next = new Set(current);
      if (open) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const toggleSession = useCallback((sessionId, open) => {
    setCollapsedSessions((current) => {
      const next = new Set(current);
      if (open) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  }, []);

  const setAllExpanded = useCallback(
    (open) => {
      setCollapsedOwners(
        open ? new Set() : new Set(groups.map(([groupKey]) => groupKey)),
      );
      setCollapsedSessions(
        open ? new Set() : new Set(sessions.map((session) => session.session)),
      );
    },
    [groups, sessions],
  );

  return (
    <div className="mx-auto min-h-screen w-full max-w-[1440px] px-2.5 py-4 sm:px-4 sm:py-7">
      <header className="border-b border-[var(--border)] pb-4">
        <div className="flex items-start justify-between gap-3 max-sm:flex-col">
          <div>
            <p className="text-xs font-bold text-[var(--accent)]">
              GROK BRIDGE RUNTIME
            </p>
            <h1 className="mt-1 text-2xl leading-tight font-bold text-[var(--strong)] sm:text-[28px]">
              Grok 会话管理
            </h1>
          </div>
          <div className="flex flex-wrap items-center justify-end gap-3 max-sm:w-full max-sm:justify-between">
            <span
              className={`inline-flex items-center gap-1.5 text-xs font-semibold ${
                connected
                  ? "text-[var(--runtime-ok)]"
                  : "text-[var(--runtime-error)]"
              }`}
            >
              <Server aria-hidden="true" size={14} />
              {connected ? "本机服务已连接" : "Runtime 连接异常"}
            </span>
            <a
              className={`${secondaryButton} no-underline`}
              href={GITHUB_URL}
              target="_blank"
              rel="noreferrer"
              title="在 GitHub 打开 grok-bridge-rs"
            >
              <ExternalLink aria-hidden="true" size={14} />
              GitHub
            </a>
            <ThemeSwitcher />
          </div>
        </div>
        <p className="mt-2 max-w-4xl text-sm leading-5 text-[var(--muted)]">
          实时查看 Codex 所属 Grok 进程与终端状态。单个 Grok 窗口可折叠，方便在多会话时保持总览。
        </p>
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            className={secondaryButton}
            type="button"
            disabled={loading}
            onClick={() => load()}
          >
            <RefreshCw
              aria-hidden="true"
              className={loading ? "animate-spin" : ""}
              size={14}
            />
            立即刷新
          </button>
          <button
            className={secondaryButton}
            type="button"
            onClick={() => setAllExpanded(true)}
          >
            <ChevronsUpDown aria-hidden="true" size={14} />
            全部展开
          </button>
          <button
            className={secondaryButton}
            type="button"
            onClick={() => setAllExpanded(false)}
          >
            <ChevronsDownUp aria-hidden="true" size={14} />
            全部折叠
          </button>
        </div>
      </header>

      <section
        className="my-4 grid grid-cols-5 border-y border-[var(--border)] bg-[var(--stats-bg)] max-md:grid-cols-2"
        aria-label="会话统计"
      >
        <Stat label="Codex 分组" value={stats.owners} />
        <Stat label="Grok 会话" value={stats.sessions} />
        <Stat label="工作中" value={stats.working} />
        <Stat label="等待输入" value={stats.waiting} />
        <Stat label="完成 / 空闲" value={stats.done} />
      </section>

      <div className="mb-2 flex items-center justify-between gap-3 px-0.5 text-[11px] text-[var(--faint)]">
        <span>
          {lastUpdated
            ? `最后刷新：${lastUpdated.toLocaleTimeString("zh-CN", { hour12: false })}`
            : "正在读取 Runtime 状态…"}
        </span>
        <span className="shrink-0">自动刷新：2 秒</span>
      </div>

      {notice ? (
        <div
          className={`mb-2 rounded-md border px-3 py-2 text-xs ${
            notice.tone === "error"
              ? "border-[var(--danger-border)] bg-[var(--notice-error-bg)] text-[var(--danger-text)]"
              : "border-[var(--notice-border)] bg-[var(--notice-bg)] text-[var(--notice-text)]"
          }`}
          role="status"
          aria-live="polite"
        >
          {notice.text}
        </div>
      ) : null}

      <main className="grid gap-2">
        {groups.length ? (
          groups.map(([key, ownerSessions]) => {
            const owner = ownerSessions[0]?.owner ?? null;
            const clientSessionId =
              ownerSessions[0]?.client_session_id ?? null;
            return (
              <OwnerGroup
                key={key}
                groupKey={key}
                owner={owner}
                clientSessionId={clientSessionId}
                sessions={ownerSessions}
                collapsed={collapsedOwners.has(key)}
                collapsedSessions={collapsedSessions}
                onToggle={(open) => toggleOwner(key, open)}
                onToggleSession={toggleSession}
                onCloseGroup={closeGroup}
                onCloseSession={closeSession}
                busy={loading}
              />
            );
          })
        ) : (
          <div className="rounded-md border border-dashed border-[var(--empty-border)] px-5 py-12 text-center text-sm text-[var(--subtle)]">
            <strong className="mb-1 block text-base text-[var(--button-text)]">
              暂无 Grok 会话
            </strong>
            新的 Codex 调用会自动显示在这里。
          </div>
        )}
      </main>
    </div>
  );
}

export { ThemeSwitcher };
