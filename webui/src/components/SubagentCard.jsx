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
      className="session group/session min-w-0 border-t border-[var(--border)] bg-[var(--session-bg)] first:border-t-0 open:bg-[var(--session-open-bg)]"
      data-session={session.session}
      open={!collapsed}
      onToggle={(event) => onToggle(event.currentTarget.open)}
    >
      <summary className="flex min-h-12 cursor-pointer list-none items-center gap-2.5 px-3 py-3 marker:hidden focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-[var(--focus)] sm:px-4">
        <ChevronRight
          aria-hidden="true"
          className="shrink-0 text-[var(--accent)] transition-transform duration-150 group-open/session:rotate-90 motion-reduce:transition-none"
          size={16}
        />
        <span className="hidden h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-[var(--border)] bg-[var(--subagent-icon-bg)] text-[var(--accent)] sm:inline-flex">
          <Bot aria-hidden="true" size={15} />
        </span>
        <ActivityBadge activity={activity} phase={session.phase} />
        <LifecycleBadge
          clientState={session.client_state}
          className={riskStates ? "" : "max-sm:hidden"}
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
            <span className="text-[10px] font-bold tracking-[0.12em] text-[var(--faint)] uppercase">
              {t("session.subagent")}
            </span>
            <h3
              className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold text-[var(--text)]"
              title={title}
            >
              {title}
            </h3>
          </div>
          {collapsed ? (
            <p
              className={`mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-[11px] ${
                riskStates
                  ? "font-semibold text-[var(--cleanup-text)]"
                  : "text-[var(--subtle)]"
              }`}
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
          className={`${dangerButton} shrink-0 max-sm:min-h-10 whitespace-normal`}
          type="button"
          disabled={busy}
          aria-label={t("session.closeAria", { id: session.session })}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onClose(session.session);
          }}
        >
          <CircleStop aria-hidden="true" size={14} className="shrink-0" />
          <span className="break-words">{t("session.close")}</span>
        </button>
      </summary>
      {/* Always keep body mounted so xterm survives collapse without losing stream. */}
      <div className="px-3 pb-4 sm:px-4">
        <SessionLifecycleHint session={session} now={now} />
        <div className="my-1 flex min-w-0 flex-wrap items-start gap-x-5 gap-y-2.5 rounded-xl border border-[var(--meta-border)] bg-[var(--meta-bg)] px-3 py-3">
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
          <p className="my-3 rounded-r-lg border-l-[3px] border-[var(--waiting-note-border)] bg-[var(--waiting-note-bg)] px-3 py-2.5 text-xs leading-5 break-words text-[var(--waiting-text)]">
            {t("session.waitingNote", { reason: session.waiting_reason })}
          </p>
        ) : (
          <div className="h-3" />
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
