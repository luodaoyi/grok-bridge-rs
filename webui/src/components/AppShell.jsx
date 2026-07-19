import { useMemo, useState } from "react";
import { useCollapseState } from "../hooks/useCollapseState.js";
import { useSessionActions } from "../hooks/useSessionActions.js";
import { useSessionStream } from "../hooks/useSessionStream.js";
import { useVersionPolling } from "../hooks/useVersionPolling.js";
import { groupSessions, sessionStats } from "../sessions.js";
import { EmptyState } from "./EmptyState.jsx";
import { Header } from "./Header.jsx";
import { Notice } from "./Notice.jsx";
import { StatsBar } from "./StatsBar.jsx";
import { SupervisorGroup } from "./SupervisorGroup.jsx";
import { UpdateBanner } from "./UpdateBanner.jsx";

function streamStatusText(connectionState, lastUpdated) {
  if (connectionState === "initial") return "正在连接实时通道…";
  if (connectionState === "retrying") return "实时通道重连中…";
  if (connectionState === "disconnected") return "实时通道已断开，等待重连…";
  if (lastUpdated) {
    return `实时更新：${lastUpdated.toLocaleTimeString("zh-CN", { hour12: false })}`;
  }
  return "实时通道已连接，等待会话数据…";
}

export function AppShell() {
  const [notice, setNotice] = useState(null);
  const {
    sessions,
    loading,
    connectionState,
    lastUpdated,
    reconnect,
    loadingRef,
    setLoading,
  } = useSessionStream({ setNotice });
  const { version, visibleUpdate, dismissUpdate } = useVersionPolling();
  const {
    collapsedOwners,
    collapsedSessions,
    toggleOwner,
    toggleSession,
    setAllExpanded,
  } = useCollapseState();
  const { closeSession, closeGroup } = useSessionActions({
    loadingRef,
    setLoading,
    setNotice,
  });

  const groups = useMemo(() => groupSessions(sessions), [sessions]);
  const stats = useMemo(() => sessionStats(sessions), [sessions]);
  const showEmpty =
    sessions.length === 0 &&
    connectionState !== "initial" &&
    connectionState !== "retrying";
  const showConnecting =
    sessions.length === 0 &&
    (connectionState === "initial" || connectionState === "retrying");

  return (
    <div className="app-shell mx-auto min-h-screen w-full max-w-[1440px] px-2.5 py-4 sm:px-5 sm:py-7">
      <a href="#session-board" className="skip-link">
        跳到会话列表
      </a>

      <Header
        version={version}
        connectionState={connectionState}
        loading={loading}
        onReconnect={reconnect}
        onExpandAll={() => setAllExpanded(true, groups, sessions)}
        onCollapseAll={() => setAllExpanded(false, groups, sessions)}
      />

      <StatsBar stats={stats} />

      <div className="mb-2.5 flex flex-wrap items-center justify-between gap-2 px-0.5 text-[11px] text-[var(--faint)]">
        <span data-stream-status={connectionState}>
          {streamStatusText(connectionState, lastUpdated)}
        </span>
        <span className="shrink-0" data-stream-mode="websocket">
          推送：WebSocket · /api/events
        </span>
      </div>

      <UpdateBanner version={visibleUpdate} onDismiss={dismissUpdate} />
      <Notice notice={notice} />

      <main id="session-board" className="grid gap-3" aria-label="监督者分组">
        {groups.length ? (
          groups.map(([key, ownerSessions]) => {
            const owner = ownerSessions[0]?.owner ?? null;
            const clientSessionId =
              ownerSessions[0]?.client_session_id ?? null;
            return (
              <SupervisorGroup
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
        ) : showConnecting ? (
          <div
            className="rounded-2xl border border-dashed border-[var(--empty-border)] bg-[var(--group-bg)] px-5 py-14 text-center shadow-[var(--shadow-sm)]"
            role="status"
            aria-live="polite"
            data-connecting="true"
          >
            <strong className="mb-1 block text-base text-[var(--button-text)]">
              正在连接实时通道
            </strong>
            <p className="mx-auto max-w-md text-sm leading-6 text-[var(--subtle)]">
              已建立到本机 Runtime 的 WebSocket 连接请求，首帧会话快照会立即渲染。
            </p>
          </div>
        ) : showEmpty ? (
          <EmptyState />
        ) : null}
      </main>
    </div>
  );
}
