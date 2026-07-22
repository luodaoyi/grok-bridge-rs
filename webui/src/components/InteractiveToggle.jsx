import { Keyboard, KeyboardOff } from "lucide-react";
import { useI18n } from "../i18n/index.js";
import { buttonBase } from "../utils/ui.js";

export function InteractiveToggle({
  interactive,
  onChange,
  connectionState = "initial",
}) {
  const { t } = useI18n();
  const connected = connectionState === "connected";
  const Icon = interactive ? Keyboard : KeyboardOff;

  return (
    <button
      type="button"
      className={`${buttonBase} ${interactive ? "btn-warning" : "btn-outline-secondary"}`}
      role="switch"
      aria-checked={interactive}
      aria-label={t("interactive.aria")}
      title={
        interactive
          ? t("interactive.warning")
          : connected
            ? t("interactive.offHint")
            : t("interactive.disconnected")
      }
      data-interactive-toggle="true"
      data-interactive={interactive ? "on" : "off"}
      onClick={() => onChange(!interactive)}
    >
      <Icon aria-hidden="true" size={14} />
      <span>
        {t("interactive.label")}:{" "}
        {interactive ? t("interactive.on") : t("interactive.off")}
      </span>
    </button>
  );
}
