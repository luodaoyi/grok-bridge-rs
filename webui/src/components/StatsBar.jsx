import { useI18n } from "../i18n/index.js";

function Stat({ label, model, value, tone }) {
  const labelParts = model ? label.split(model) : null;

  return (
    <div className={`card stat-card ${tone ? `stat-${tone}` : ""}`}>
      <div className="card-body">
        <div className="subheader">
          {labelParts?.length === 2 ? (
            <>
              {labelParts[0]}
              <span className="stat-model">{model}</span>
              {labelParts[1]}
            </>
          ) : (
            label
          )}
        </div>
        <div className="h1 mb-0 stat-value">{value}</div>
      </div>
    </div>
  );
}

export function StatsBar({ stats }) {
  const { t, formatNumber } = useI18n();
  return (
    <section
      className="stats-grid"
      aria-label={t("stats.aria")}
    >
      <Stat
        label={t("stats.owners")}
        model="Codex"
        value={formatNumber(stats.owners)}
      />
      <Stat
        label={t("stats.sessions")}
        model="Grok"
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
      <Stat label={t("stats.done")} value={formatNumber(stats.done)} tone="done" />
    </section>
  );
}
