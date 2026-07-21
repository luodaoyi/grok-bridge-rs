import { Bot, ChevronRight, CircleStop } from "lucide-react";
import { useI18n } from "../i18n/index.js";
import { useNow } from "../hooks/useNow.js";
import {
  activityOf,
  ageLabel,
  clientStateLabel,
  lifecycleCollapsedSummary,
  lifecycleHintModel,
} from "../sessions.js";
import { dangerButton } from "../utils/ui.js";
import { MetaField } from "./MetaField.jsx";
import { SessionLifecycleHint } from "./SessionLifecycleHint.jsx";
import { ActivityBadge, LifecycleBadge } from "./StatusBadge.jsx";
import { Terminal } from "./Terminal.jsx";

export function SubagentCard({
  session,
  heightKey,
  collapsed,
  onToggle,
  onClose,
  busy,
}) {
  const { t, locale } = useI18n();
  const activity = activityOf(session);
  const title = session.title || session.session;
  const hint = lifecycleHintModel(session);
  const riskStates = hint.kind === "orphaned" || hint.kind === "closing";
  // Single local clock per orphaned session: shared by collapsed summary + banner.
  const needsClock = hint.kind === "orphaned" && hint.deadlineMs != null;
  const now = useNow(needsClock);
  const riskSummary = lifecycleCollapsedSummary(session, t, locale, now);

  return (
    <details
      className="session subagent-card"
      data-session={session.session}
      open={!collapsed}
      onToggle={(event) => onToggle(event.currentTarget.open)}
    >
      <summary className="subagent-summary">
        <ChevronRight
          aria-hidden="true"
          className="subagent-chevron"
          size={16}
        />
        <span className="avatar avatar-sm bg-teal-lt text-teal subagent-icon">
          <Bot aria-hidden="true" size={15} />
        </span>
        <ActivityBadge activity={activity} phase={session.phase} />
        <LifecycleBadge
          clientState={session.client_state}
          className={riskStates ? "" : "lifecycle-secondary"}
        />
        <div className="subagent-copy">
          <div className="subagent-title-row">
            <span className="subheader">
              {t("session.subagent")}
            </span>
            <h3
              className="subagent-title"
              title={title}
            >
              {title}
            </h3>
          </div>
          {collapsed ? (
            <p
              className={`subagent-collapsed text-secondary ${riskStates ? "text-danger fw-semibold" : ""}`}
              data-lifecycle-collapsed={riskStates ? hint.kind : undefined}
            >
              {riskSummary ? `${riskSummary} · ` : ""}
              {session.session}
              {session.tool_name ? ` · ${session.tool_name}` : ""}
              {session.waiting_reason
                ? ` · ${t("session.waitingCollapsed", { reason: session.waiting_reason })}`
                : ""}
            </p>
          ) : null}
        </div>
        <button
          className={`${dangerButton} subagent-close`}
          type="button"
          disabled={busy}
          aria-label={t("session.closeAria", { id: session.session })}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onClose(session.session);
          }}
        >
          <CircleStop aria-hidden="true" size={14} />
          <span>{t("session.close")}</span>
        </button>
      </summary>
      {/* Always keep body mounted so xterm survives collapse without losing stream. */}
      <div className="subagent-body">
        <SessionLifecycleHint session={session} now={now} />
        <div className="meta-grid">
          <MetaField label={t("session.meta.id")} title={session.session}>
            {session.session}
          </MetaField>
          <MetaField label={t("session.meta.pid")}>
            {t("session.meta.pidValue", {
              pid: session.process_id ?? "-",
            })}
          </MetaField>
          <MetaField label={t("session.meta.updated")}>
            {ageLabel(session.updated_at_ms, Date.now(), locale)}
          </MetaField>
          <MetaField label={t("session.meta.client")}>
            {clientStateLabel(session.client_state, t)}
          </MetaField>
          {session.hook_event ? (
            <MetaField label={t("session.meta.hook")}>
              {session.hook_event}
            </MetaField>
          ) : null}
          {session.tool_name ? (
            <MetaField label={t("session.meta.tool")} title={session.tool_name}>
              {session.tool_name}
            </MetaField>
          ) : null}
          <MetaField label={t("session.meta.cwd")} title={session.cwd} wide>
            {session.cwd}
          </MetaField>
        </div>
        {session.waiting_reason ? (
          <p className="alert alert-warning waiting-note">
            {t("session.waitingNote", { reason: session.waiting_reason })}
          </p>
        ) : (
          <div className="terminal-spacer" />
        )}
        <Terminal
          id={session.session}
          heightKey={heightKey}
          rows={session.rows}
          cols={session.cols}
          label={t("session.terminalAria", { title })}
        />
      </div>
    </details>
  );
}
