import { useI18n } from "../i18n/index.js";
import { countdownLabel, lifecycleHintModel } from "../sessions.js";

/**
 * Compact but distinguishable offline tip: soft fill + thin accent, still one line.
 * Visually stronger than connected (no banner), weaker than risk cards.
 */
const COMPACT_DISCONNECTED =
  "my-1 rounded border-l-2 border-[var(--disconnected-text)] bg-[var(--disconnected-bg)] px-2 py-0.5 text-[11px] leading-snug text-[var(--disconnected-text)]";

/** Prominent risk card for orphaned / closing cleanup paths. */
const PROMINENT =
  "my-2 rounded-lg border-l-[3px] px-3 py-2 text-xs leading-5 font-medium";

const PROMINENT_TONE = {
  orphaned:
    "border-[var(--cleanup-text)] bg-[var(--cleanup-bg)] text-[var(--cleanup-text)]",
  closing:
    "border-[var(--cleanup-text)] bg-[var(--cleanup-bg)] text-[var(--cleanup-text)]",
};

/**
 * Adaptive managed-session lifecycle hint.
 * Connected: no banner (keep-alive/lease/grace prose removed).
 * Disconnected: compact offline notice (no lease/grace policy footnotes).
 * Orphaned / closing: prominent risk density (countdown and cleanup safety).
 * Orphaned countdown uses the parent-owned `now` clock (one interval per session).
 */
export function SessionLifecycleHint({ session, now = Date.now() }) {
  const { t, locale, formatClock } = useI18n();
  const model = lifecycleHintModel(session);

  if (model.kind === "none") return null;

  if (model.kind === "disconnected") {
    return (
      <div
        className={COMPACT_DISCONNECTED}
        data-lifecycle-hint="disconnected"
        data-density="compact"
        role="status"
      >
        <span className="font-semibold tracking-tight">
          {t("session.lifecycle.disconnectedTitle")}
        </span>
        <span className="opacity-95">
          {" — "}
          {t("session.lifecycle.disconnectedBody")}
        </span>
      </div>
    );
  }

  if (model.kind === "orphaned") {
    if (model.deadlineMs == null) {
      return (
        <div
          className={`${PROMINENT} ${PROMINENT_TONE.orphaned}`}
          data-lifecycle-hint="orphaned"
          data-density="prominent"
          role="alert"
        >
          <p className="font-bold tracking-tight">
            {t("session.lifecycle.orphanedTitle")}
          </p>
          <p className="mt-0.5">{t("session.lifecycle.orphanedNoDeadline")}</p>
        </div>
      );
    }
    const due = now >= model.deadlineMs;
    const remaining = countdownLabel(model.deadlineMs, now, locale);
    const at = formatClock(model.deadlineMs);
    return (
      <div
        className={`${PROMINENT} ${PROMINENT_TONE.orphaned}`}
        data-lifecycle-hint="orphaned"
        data-density="prominent"
        data-auto-close-at={String(model.deadlineMs)}
        data-cleanup-due={due ? "true" : "false"}
        role="alert"
      >
        <p className="font-bold tracking-tight">
          {t("session.lifecycle.orphanedTitle")}
        </p>
        <p className="mt-0.5 tabular-nums" data-lifecycle-countdown>
          {due
            ? t("session.lifecycle.orphanedCountdownDue")
            : t("session.lifecycle.orphanedCountdown", { remaining })}
        </p>
        <p className="mt-0.5 tabular-nums opacity-95">
          {t("session.lifecycle.orphanedAt", { at })}
        </p>
      </div>
    );
  }

  if (model.kind === "closing") {
    return (
      <div
        className={`${PROMINENT} ${PROMINENT_TONE.closing}`}
        data-lifecycle-hint="closing"
        data-density="prominent"
        role="status"
      >
        <p className="font-bold tracking-tight">
          {t("session.lifecycle.closingTitle")}
        </p>
        <p className="mt-0.5">{t("session.lifecycle.closingBody")}</p>
      </div>
    );
  }

  return null;
}
