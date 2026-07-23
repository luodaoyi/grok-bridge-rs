import { useEffect, useId, useRef, useState } from "react";
import { ChevronRight, Network, Trash2 } from "lucide-react";
import { useI18n } from "../i18n/index.js";
import {
  activityLabel,
  activityOf,
  dominantClientState,
  groupSummary,
} from "../sessions.js";
import { dangerButton } from "../utils/ui.js";
import { ActivityBadge, LifecycleBadge } from "./StatusBadge.jsx";
import { SubagentCard } from "./SubagentCard.jsx";

function sessionTitle(session) {
  return session.title || session.session;
}

export function SupervisorGroup({
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
  const { t, locale, formatNumber } = useI18n();
  const displayOwner = owner ?? t("group.unowned");
  const lifecycleState = dominantClientState(sessions);
  const baseId = useId();
  const tabRefs = useRef(new Map());
  const [selectedSessionId, setSelectedSessionId] = useState(
    () => sessions[0]?.session ?? null,
  );

  // Keep selection stable across metadata updates; fall back when removed.
  useEffect(() => {
    setSelectedSessionId((prev) => {
      if (sessions.some((session) => session.session === prev)) return prev;
      return sessions[0]?.session ?? null;
    });
  }, [sessions]);

  const selectedId = sessions.some(
    (session) => session.session === selectedSessionId,
  )
    ? selectedSessionId
    : (sessions[0]?.session ?? null);

  const selectSession = (sessionId, { focus = false } = {}) => {
    setSelectedSessionId(sessionId);
    if (focus) {
      queueMicrotask(() => {
        tabRefs.current.get(sessionId)?.focus();
      });
    }
  };

  const moveSelection = (nextIndex, { focus = true } = {}) => {
    if (sessions.length === 0) return;
    const clamped = Math.max(0, Math.min(sessions.length - 1, nextIndex));
    selectSession(sessions[clamped].session, { focus });
  };

  const onTabKeyDown = (event, index) => {
    switch (event.key) {
      case "ArrowRight":
        event.preventDefault();
        moveSelection(index + 1);
        break;
      case "ArrowLeft":
        event.preventDefault();
        moveSelection(index - 1);
        break;
      case "Home":
        event.preventDefault();
        moveSelection(0);
        break;
      case "End":
        event.preventDefault();
        moveSelection(sessions.length - 1);
        break;
      default:
        break;
    }
  };

  return (
    <details
      className="card supervisor-card group"
      data-owner-key={groupKey}
      open={!collapsed}
      onToggle={(event) => onToggle(event.currentTarget.open)}
    >
      <summary className="card-header supervisor-summary">
        <ChevronRight
          aria-hidden="true"
          className="supervisor-chevron"
          size={18}
        />
        <span className="avatar avatar-md bg-cyan-lt text-cyan supervisor-icon">
          <Network aria-hidden="true" size={18} />
        </span>
        <div className="supervisor-copy">
          <div className="supervisor-labels">
            <span className="subheader">
              {t("group.supervisor")}
            </span>
            <LifecycleBadge clientState={lifecycleState} />
          </div>
          <h2
            className="card-title supervisor-title"
            title={displayOwner}
          >
            {displayOwner}
          </h2>
          <p className="text-secondary supervisor-summary-text">
            {groupSummary(sessions, t, locale)}
            {clientSessionId
              ? ` · ${t("group.idPrefix", { id: clientSessionId })}`
              : ""}
          </p>
        </div>
        <span className="badge bg-azure-lt text-azure supervisor-count">
          {t("group.subagentCount", { n: formatNumber(sessions.length) })}
        </span>
        {owner == null && clientSessionId == null ? null : (
          <button
            className={`${dangerButton} supervisor-close`}
            type="button"
            disabled={busy}
            aria-label={t("group.closeAllAria", { owner: displayOwner })}
            data-close-all-group="true"
            onClick={(event) => {
              // Buttons inside <summary> must not toggle the details group.
              event.preventDefault();
              event.stopPropagation();
              onCloseGroup(owner, clientSessionId, sessions.length);
            }}
          >
            <Trash2 aria-hidden="true" size={14} />
            <span>{t("group.closeAll")}</span>
          </button>
        )}
      </summary>
      {/* Keep children mounted when collapsed so session terminals stay alive. */}
      <div className="supervisor-body">
        {sessions.length > 0 ? (
          <>
            <div
              role="tablist"
              aria-label={displayOwner}
              className="nav nav-tabs flex-nowrap session-tabs"
            >
              {sessions.map((session, index) => {
                const activity = activityOf(session);
                const status = activityLabel(activity, t);
                const selected = session.session === selectedId;
                const tabId = `${baseId}-tab-${session.session}`;
                const panelId = `${baseId}-panel-${session.session}`;
                const ordinal = index + 1;
                return (
                  <button
                    key={session.session}
                    ref={(node) => {
                      if (node) tabRefs.current.set(session.session, node);
                      else tabRefs.current.delete(session.session);
                    }}
                    type="button"
                    role="tab"
                    id={tabId}
                    data-session-tab={session.session}
                    aria-selected={selected}
                    aria-controls={panelId}
                    tabIndex={selected ? 0 : -1}
                    aria-label={`${ordinal} ${status} · ${sessionTitle(session)}`}
                    className={`nav-link session-tab ${selected ? "active" : ""}`}
                    onClick={() => selectSession(session.session)}
                    onKeyDown={(event) => onTabKeyDown(event, index)}
                  >
                    <span aria-hidden="true" className="session-ordinal">
                      {ordinal}
                    </span>
                    <ActivityBadge
                      activity={activity}
                      phase={session.phase}
                      className="session-tab-badge"
                    />
                  </button>
                );
              })}
            </div>
            {sessions.map((session) => {
              const selected = session.session === selectedId;
              const tabId = `${baseId}-tab-${session.session}`;
              const panelId = `${baseId}-panel-${session.session}`;
              return (
                <div
                  key={session.session}
                  role="tabpanel"
                  id={panelId}
                  data-session-panel={session.session}
                  aria-labelledby={tabId}
                  hidden={!selected}
                >
                  <SubagentCard
                    session={session}
                    heightKey={groupKey}
                    collapsed={collapsedSessions.has(session.session)}
                    onToggle={(open) => onToggleSession(session.session, open)}
                    onClose={onCloseSession}
                    busy={busy}
                  />
                </div>
              );
            })}
          </>
        ) : null}
      </div>
    </details>
  );
}
