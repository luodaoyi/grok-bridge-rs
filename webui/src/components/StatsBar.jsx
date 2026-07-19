function Stat({ label, value, tone }) {
  return (
    <div
      className={`stat-cell min-w-0 border-r border-[var(--border)] px-3 py-3 last:border-r-0 max-md:nth-[2n]:border-r-0 max-md:nth-[-n+4]:border-b max-md:last:col-span-2 max-md:last:border-b-0 ${
        tone ? `stat-${tone}` : ""
      }`}
    >
      <span className="block text-[11px] font-medium tracking-wide text-[var(--subtle)]">
        {label}
      </span>
      <strong className="mt-1 block text-2xl leading-none font-bold tabular-nums text-[var(--strong)]">
        {value}
      </strong>
    </div>
  );
}

export function StatsBar({ stats }) {
  return (
    <section
      className="my-4 overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--stats-bg)] shadow-[var(--shadow-sm)]"
      aria-label="会话统计"
    >
      <div className="grid grid-cols-5 max-md:grid-cols-2">
        <Stat label="监督者（Codex）" value={stats.owners} />
        <Stat label="子代理（Grok）" value={stats.sessions} />
        <Stat label="工作中" value={stats.working} tone="working" />
        <Stat label="等待输入" value={stats.waiting} tone="waiting" />
        <Stat label="完成 / 空闲" value={stats.done} tone="done" />
      </div>
    </section>
  );
}
