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
      <div className="page app-shell">
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

        <div className="page-wrapper">
          <div className="page-body">
            <div className="container-xl">
              <StatsBar stats={stats} />

              <div className="stream-meta text-secondary small">
                <span data-stream-status={connectionState}>
                  {streamStatusText(connectionState, lastUpdated, t, formatClock)}
                </span>
                <span data-stream-mode="websocket">{t("stream.pushMode")}</span>
              </div>

              {interactive ? (
                <div
                  className="alert alert-warning"
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
                className="session-board"
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
                    className="card empty-state-card"
                    role="status"
                    aria-live="polite"
                    data-connecting="true"
                  >
                    <div className="card-body empty">
                      <p className="empty-title">{t("connecting.title")}</p>
                      <p className="empty-subtitle text-secondary">
                        {t("connecting.body")}
                      </p>
                    </div>
                  </div>
                ) : showEmpty ? (
                  <EmptyState />
                ) : null}
              </main>
            </div>
          </div>
        </div>
      </div>
    </TerminalIOContext.Provider>
  );
}
