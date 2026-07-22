import { Bot } from "lucide-react";
import { useI18n } from "../i18n/index.js";

export function EmptyState() {
  const { t } = useI18n();
  return (
    <div className="card empty-state-card">
      <div className="card-body empty">
        <div className="empty-img text-cyan">
          <Bot aria-hidden="true" size={42} strokeWidth={1.5} />
        </div>
        <p className="empty-title">{t("empty.title")}</p>
        <p className="empty-subtitle text-secondary">{t("empty.body")}</p>
      </div>
    </div>
  );
}
