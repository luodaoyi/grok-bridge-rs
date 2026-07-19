import { useMemo, useState } from "react";
import { TerminalIOContext } from "../context/TerminalIOContext.jsx";
import { useCollapseState } from "../hooks/useCollapseState.js";
import { useInteractiveMode } from "../hooks/useInteractiveMode.js";
import { useSessionActions } from "../hooks/useSessionActions.js";
import { useSessionStream } from "../hooks/useSessionStream.js";
import { useVersionPolling } from "../hooks/useVersionPolling.js";
import { useI18n } from "../i18n/index.js";
import { groupSessions, sessionStats } from "../sessions.js";
import { EmptyState } from "./EmptyState.jsx";
import { Header } from "./Header.jsx";
import { Notice } from "./Notice.jsx";
import { StatsBar } from "./StatsBar.jsx";
import { SupervisorGroup } from "./SupervisorGroup.jsx";
import { UpdateBanner } from "./UpdateBanner.jsx";

function streamStatusText(connectionState, lastUpdated, t, formatClock) {
  if (connectionState === "initial") return t("stream.initial");
  if (connectionState === "retrying") return t("stream.retrying");
  if (connectionState === "disconnected") return t("stream.disconnected");
  if (lastUpdated) {
    return t("stream.updated", { time: formatClock(lastUpdated) });
  }
  return t("stream.waitingData");
}

export function AppShell() {
  const { t, locale, formatClock } = useI18n();
  const [notice, setNotice] = useState(null);
  const { interactive, setInteractive } = useInteractiveMode();
  const {
    sessions,
    loading,
    connectionState,
    lastUpdated,
    reconnect,
    loadingRef,
    setLoading,
    sendTerminalInput,
    sendTerminalResize,
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

  const groups = useMemo(
    () => groupSessions(sessions, locale),
    [sessions, locale],
  );
  const stats = useMemo(() => sessionStats(sessions), [sessions]);
  const showEmpty =
    sessions.length === 0 &&
    connectionState !== "initial" &&
    connectionState !== "retrying";
  const showConnecting =
    sessions.length === 0 &&
    (connectionState === "initial" || connectionState === "retrying");

  const terminalIO = useMemo(
    () => ({
      interactive,
      setInteractive,
      connectionState,
      sendTerminalInput,
      sendTerminalResize,
    }),
    [
      interactive,
      setInteractive,
      connectionState,
      sendTerminalInput,
      sendTerminalResize,
    ],
  );

  return (
    <TerminalIOContext.Provider value={terminalIO}>
      <div className="app-shell mx-auto min-h-screen w-full max-w-[1440px] px-2.5 py-4 sm:px-5 sm:py-7">
        <a href="#session-board" className="skip-link">
          {t("app.skipToSessions")}
        </a>

        <Header
          version={version}
          connectionState={connectionState}
          loading={loading}
          interactive={interactive}
          onInteractiveChange={setInteractive}
          onReconnect={reconnect}
          onExpandAll={() => setAllExpanded(true, groups, sessions)}
          onCollapseAll={() => setAllExpanded(false, groups, sessions)}
        />

        <StatsBar stats={stats} />

        <div className="mb-2.5 flex flex-wrap items-center justify-between gap-2 px-0.5 text-[11px] text-[var(--faint)]">
          <span
            className="min-w-0 break-words"
            data-stream-status={connectionState}
          >
            {streamStatusText(connectionState, lastUpdated, t, formatClock)}
          </span>
          <span className="shrink-0" data-stream-mode="websocket">
            {t("stream.pushMode")}
          </span>
        </div>

        {interactive ? (
          <div
            className="mb-3 rounded-xl border border-[var(--runtime-warn-border)] bg-[var(--runtime-warn-bg)] px-3.5 py-2.5 text-xs text-[var(--runtime-warn)] shadow-[var(--shadow-sm)]"
            role="status"
            data-interactive-warning="true"
          >
            {t("interactive.warning")}
          </div>
        ) : null}

        <UpdateBanner version={visibleUpdate} onDismiss={dismissUpdate} />
        <Notice notice={notice} />

        <main
          id="session-board"
          className="grid gap-3"
          aria-label={t("board.aria")}
        >
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
                {t("connecting.title")}
              </strong>
              <p className="mx-auto max-w-md text-sm leading-6 text-[var(--subtle)]">
                {t("connecting.body")}
              </p>
            </div>
          ) : showEmpty ? (
            <EmptyState />
          ) : null}
        </main>
      </div>
    </TerminalIOContext.Provider>
  );
}
