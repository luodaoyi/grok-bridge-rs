import { Bot, ChevronRight, CircleStop } from "lucide-react";
import {
  activityOf,
  ageLabel,
  clientStateLabel,
  remainingLabel,
} from "../sessions.js";
import { dangerButton } from "../utils/ui.js";
import { MetaField } from "./MetaField.jsx";
import { ActivityBadge, LifecycleBadge } from "./StatusBadge.jsx";
import { Terminal } from "./Terminal.jsx";

export function SubagentCard({ session, collapsed, onToggle, onClose, busy }) {
  const activity = activityOf(session);
  const title = session.title || session.session;

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
          className="max-sm:hidden"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
            <span className="text-[10px] font-bold tracking-[0.12em] text-[var(--faint)] uppercase">
              子代理
            </span>
            <h3
              className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold text-[var(--text)]"
              title={title}
            >
              {title}
            </h3>
          </div>
          {collapsed ? (
            <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-[var(--subtle)]">
              {session.session}
              {session.tool_name ? ` · ${session.tool_name}` : ""}
              {session.waiting_reason
                ? ` · 等待：${session.waiting_reason}`
                : ""}
            </p>
          ) : null}
        </div>
        <button
          className={`${dangerButton} shrink-0 max-sm:min-h-10`}
          type="button"
          disabled={busy}
          aria-label={`关闭子代理 ${session.session}`}
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
      {/* Always keep body mounted so xterm survives collapse without losing stream. */}
      <div className="px-3 pb-4 sm:px-4">
        <div className="my-1 flex min-w-0 flex-wrap items-start gap-x-5 gap-y-2.5 rounded-xl border border-[var(--meta-border)] bg-[var(--meta-bg)] px-3 py-3">
          <MetaField label="会话 ID" title={session.session}>
            {session.session}
          </MetaField>
          <MetaField label="进程">
            PID {session.process_id ?? "-"}
          </MetaField>
          <MetaField label="最近更新">
            {ageLabel(session.updated_at_ms)}
          </MetaField>
          <MetaField label="Codex 连接">
            {clientStateLabel(session.client_state)}
          </MetaField>
          {session.auto_close_at_ms ? (
            <MetaField label="自动清理倒计时">
              {remainingLabel(session.auto_close_at_ms)}
            </MetaField>
          ) : null}
          {session.hook_event ? (
            <MetaField label="最近 Hook">{session.hook_event}</MetaField>
          ) : null}
          {session.tool_name ? (
            <MetaField label="当前工具" title={session.tool_name}>
              {session.tool_name}
            </MetaField>
          ) : null}
          <MetaField label="工作目录" title={session.cwd} wide>
            {session.cwd}
          </MetaField>
        </div>
        {session.waiting_reason ? (
          <p className="my-3 rounded-r-lg border-l-[3px] border-[var(--waiting-note-border)] bg-[var(--waiting-note-bg)] px-3 py-2.5 text-xs leading-5 text-[var(--waiting-text)]">
            等待 Codex：{session.waiting_reason}
          </p>
        ) : (
          <div className="h-3" />
        )}
        <Terminal
          id={session.session}
          rows={session.rows}
          cols={session.cols}
          label={`子代理 ${title} 的终端画面`}
        />
      </div>
    </details>
  );
}
