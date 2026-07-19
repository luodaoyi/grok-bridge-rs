import { useI18n } from "../i18n/index.js";

function Stat({ label, value, tone }) {
  return (
    <div
      className={`stat-cell min-w-0 border-r border-[var(--border)] px-2.5 py-3 last:border-r-0 max-md:nth-[2n]:border-r-0 max-md:nth-[-n+4]:border-b max-md:last:col-span-2 max-md:last:border-b-0 sm:px-3 ${
        tone ? `stat-${tone}` : ""
      }`}
    >
      <span className="block text-[11px] leading-snug font-medium tracking-wide break-words text-[var(--subtle)]">
        {label}
      </span>
      <strong className="mt-1 block text-2xl leading-none font-bold tabular-nums text-[var(--strong)]">
        {value}
      </strong>
    </div>
  );
}

export function StatsBar({ stats }) {
  const { t, formatNumber } = useI18n();
  return (
    <section
      className="my-4 overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--stats-bg)] shadow-[var(--shadow-sm)]"
      aria-label={t("stats.aria")}
    >
      <div className="grid grid-cols-5 max-md:grid-cols-2">
        <Stat label={t("stats.owners")} value={formatNumber(stats.owners)} />
        <Stat
          label={t("stats.sessions")}
          value={formatNumber(stats.sessions)}
        />
        <Stat
          label={t("stats.working")}
          value={formatNumber(stats.working)}
          tone="working"
        />
        <Stat
          label={t("stats.waiting")}
          value={formatNumber(stats.waiting)}
          tone="waiting"
        />
        <Stat
          label={t("stats.done")}
          value={formatNumber(stats.done)}
          tone="done"
        />
      </div>
    </section>
  );
}
