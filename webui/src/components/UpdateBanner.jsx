import { ExternalLink } from "lucide-react";
import { secondaryButton } from "../utils/ui.js";

export function UpdateBanner({ version, onDismiss }) {
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
          发现新版本 v{version.latest}
        </strong>
        <p className="mt-0.5 text-[var(--muted)]">
          当前 Runtime 为 v{version.current}
          。请手动下载并替换本地二进制，重启后生效。
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
          打开最新 Release
        </a>
        <button className={secondaryButton} type="button" onClick={onDismiss}>
          稍后提醒
        </button>
      </div>
    </div>
  );
}
