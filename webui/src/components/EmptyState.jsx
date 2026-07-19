import { Bot } from "lucide-react";

export function EmptyState() {
  return (
    <div className="rounded-2xl border border-dashed border-[var(--empty-border)] bg-[var(--group-bg)] px-5 py-14 text-center shadow-[var(--shadow-sm)]">
      <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--border)] bg-[var(--session-bg)] text-[var(--accent)]">
        <Bot aria-hidden="true" size={22} />
      </div>
      <strong className="mb-1 block text-base text-[var(--button-text)]">
        暂无 Grok 会话
      </strong>
      <p className="mx-auto max-w-md text-sm leading-6 text-[var(--subtle)]">
        新的 Codex 监督者调用会自动出现在这里；每个 Grok
        会话作为持久子代理显示其终端与生命周期状态。
      </p>
    </div>
  );
}
