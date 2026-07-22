import { ExternalLink } from "lucide-react";
import { useI18n } from "../i18n/index.js";
import { secondaryButton } from "../utils/ui.js";

export function UpdateBanner({ version, onDismiss }) {
  const { t } = useI18n();
  if (!version?.update_available || !version.latest) return null;
  return (
    <div
      className="alert alert-success update-banner"
      role="status"
      aria-live="polite"
      data-update-banner="true"
    >
      <div className="update-copy">
        <strong className="d-block">
          {t("update.title", { version: version.latest })}
        </strong>
        <p className="mb-0 text-secondary">
          {t("update.body", { current: version.current })}
        </p>
      </div>
      <div className="btn-list">
        <a
          className={secondaryButton}
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
