import { ChevronRight, Network, Trash2 } from "lucide-react";
import {
  dominantClientState,
  groupSummary,
} from "../sessions.js";
import { dangerButton } from "../utils/ui.js";
import { LifecycleBadge } from "./StatusBadge.jsx";
import { SubagentCard } from "./SubagentCard.jsx";

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
  const displayOwner = owner ?? "未标记的 Codex 对话";
  const lifecycleState = dominantClientState(sessions);

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
              监督者 · Codex
            </span>
            <LifecycleBadge clientState={lifecycleState} />
          </div>
          <h2
            className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold text-[var(--strong)] sm:text-[15px]"
            title={displayOwner}
          >
            {displayOwner}
          </h2>
          <p className="mt-0.5 text-[11px] text-[var(--subtle)]">
            {groupSummary(sessions)}
            {clientSessionId ? ` · id ${clientSessionId}` : ""}
          </p>
        </div>
        <span className="shrink-0 rounded-full border border-[var(--pill-border)] bg-[var(--pill-bg)] px-2.5 py-1 text-[11px] font-bold text-[var(--accent-text)]">
          {sessions.length} 个子代理
        </span>
      </summary>
      {/* Keep children mounted when collapsed so session terminals stay alive. */}
      <div className="border-t border-[var(--group-body-border)] bg-[var(--group-body-bg)]">
        {owner == null && clientSessionId == null ? null : (
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--group-body-border)] px-3 py-2.5 sm:px-4">
            <p className="text-[11px] text-[var(--subtle)]">
              关闭监督者会终止其下全部 Grok 子代理进程。
            </p>
            <button
              className={`${dangerButton} max-sm:w-full max-sm:min-h-10`}
              type="button"
              disabled={busy}
              aria-label={`关闭监督者 ${displayOwner} 下的全部 Grok 子代理`}
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
          <SubagentCard
            key={session.session}
            session={session}
            collapsed={collapsedSessions.has(session.session)}
            onToggle={(open) => onToggleSession(session.session, open)}
            onClose={onCloseSession}
            busy={busy}
          />
        ))}
      </div>
    </details>
  );
}
