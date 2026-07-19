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
      className="group overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--group-bg)] shadow-[var(--shadow-md)] open:border-[var(--open-border)] open:shadow-[var(--shadow-lg)]"
      data-owner-key={groupKey}
      open={!collapsed}
      onToggle={(event) => onToggle(event.currentTarget.open)}
    >
      <summary className="flex min-h-16 cursor-pointer list-none items-center gap-3 px-3 py-3 marker:hidden focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-[var(--focus)] sm:px-4">
        <ChevronRight
          aria-hidden="true"
          className="shrink-0 text-[var(--accent)] transition-transform duration-150 group-open:rotate-90 motion-reduce:transition-none"
          size={18}
        />
        <span className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-[var(--supervisor-icon-border)] bg-[var(--supervisor-icon-bg)] text-[var(--accent)]">
          <Network aria-hidden="true" size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[10px] font-bold tracking-[0.14em] text-[var(--faint)] uppercase">
              {t("group.supervisor")}
            </span>
            <LifecycleBadge clientState={lifecycleState} />
          </div>
          <h2
            className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold text-[var(--strong)] sm:text-[15px]"
            title={displayOwner}
          >
            {displayOwner}
          </h2>
          <p className="mt-0.5 break-words text-[11px] text-[var(--subtle)]">
            {groupSummary(sessions, t, locale)}
            {clientSessionId
              ? ` · ${t("group.idPrefix", { id: clientSessionId })}`
              : ""}
          </p>
        </div>
        <span className="shrink-0 rounded-full border border-[var(--pill-border)] bg-[var(--pill-bg)] px-2.5 py-1 text-[11px] font-bold break-words text-[var(--accent-text)]">
          {t("group.subagentCount", { n: formatNumber(sessions.length) })}
        </span>
      </summary>
      {/* Keep children mounted when collapsed so session terminals stay alive. */}
      <div className="border-t border-[var(--group-body-border)] bg-[var(--group-body-bg)]">
        {owner == null && clientSessionId == null ? null : (
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--group-body-border)] px-3 py-2.5 sm:px-4">
            <p className="min-w-0 flex-1 text-[11px] leading-5 break-words text-[var(--subtle)]">
              {t("group.closeHint")}
            </p>
            <button
              className={`${dangerButton} max-sm:w-full max-sm:min-h-10 whitespace-normal text-left`}
              type="button"
              disabled={busy}
              aria-label={t("group.closeAllAria", { owner: displayOwner })}
              onClick={() =>
                onCloseGroup(owner, clientSessionId, sessions.length)
              }
            >
              <Trash2 aria-hidden="true" size={14} className="shrink-0" />
              <span className="min-w-0 break-words">{t("group.closeAll")}</span>
            </button>
          </div>
        )}
        {sessions.length > 0 ? (
          <>
            <div
              role="tablist"
              aria-label={displayOwner}
              className="flex min-w-0 flex-nowrap items-center gap-1 overflow-x-auto border-b border-[var(--group-body-border)] px-3 py-2 sm:px-4"
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
                    className={`inline-flex shrink-0 items-center gap-1.5 rounded-lg border px-2 py-1 text-left text-xs font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)] ${
                      selected
                        ? "border-[var(--accent)] bg-[var(--session-open-bg)] text-[var(--strong)]"
                        : "border-[var(--border)] bg-[var(--session-bg)] text-[var(--subtle)] hover:border-[var(--open-border)]"
                    }`}
                    onClick={() => selectSession(session.session)}
                    onKeyDown={(event) => onTabKeyDown(event, index)}
                  >
                    <span
                      aria-hidden="true"
                      className="tabular-nums text-[11px] text-[var(--faint)]"
                    >
                      {ordinal}
                    </span>
                    <ActivityBadge
                      activity={activity}
                      phase={session.phase}
                      className="pointer-events-none"
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
