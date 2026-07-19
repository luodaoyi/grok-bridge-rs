import { ExternalLink } from "lucide-react";
import { useI18n } from "../i18n/index.js";
import { secondaryButton } from "../utils/ui.js";

export function UpdateBanner({ version, onDismiss }) {
  const { t } = useI18n();
  if (!version?.update_available || !version.latest) return null;
  return (
    <div
      className="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--notice-border)] bg-[var(--notice-bg)] px-3.5 py-3 text-xs text-[var(--notice-text)] shadow-[var(--shadow-sm)]"
      role="status"
      aria-live="polite"
      data-update-banner="true"
    >
      <div className="min-w-0">
        <strong className="block text-sm text-[var(--strong)]">
          {t("update.title", { version: version.latest })}
        </strong>
        <p className="mt-0.5 break-words text-[var(--muted)]">
          {t("update.body", { current: version.current })}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <a
          className={`${secondaryButton} no-underline`}
          href={version.release_url}
          target="_blank"
          rel="noreferrer"
        >
          <ExternalLink aria-hidden="true" size={14} />
          {t("update.openRelease")}
        </a>
        <button className={secondaryButton} type="button" onClick={onDismiss}>
          {t("update.dismiss")}
        </button>
      </div>
    </div>
  );
}
