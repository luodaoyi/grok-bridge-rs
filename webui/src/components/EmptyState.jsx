import { Bot } from "lucide-react";
import { useI18n } from "../i18n/index.js";

export function EmptyState() {
  const { t } = useI18n();
  return (
    <div className="rounded-2xl border border-dashed border-[var(--empty-border)] bg-[var(--group-bg)] px-5 py-14 text-center shadow-[var(--shadow-sm)]">
      <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--border)] bg-[var(--session-bg)] text-[var(--accent)]">
        <Bot aria-hidden="true" size={22} />
      </div>
      <strong className="mb-1 block text-base text-[var(--button-text)]">
        {t("empty.title")}
      </strong>
      <p className="mx-auto max-w-md text-sm leading-6 text-[var(--subtle)]">
        {t("empty.body")}
      </p>
    </div>
  );
}
