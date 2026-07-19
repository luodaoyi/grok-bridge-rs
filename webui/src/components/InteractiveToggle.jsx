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
      className={`${buttonBase} max-w-full whitespace-normal border px-2.5 py-1.5 text-left ${
        interactive
          ? "border-[var(--runtime-warn-border)] bg-[var(--runtime-warn-bg)] text-[var(--runtime-warn)]"
          : "border-[var(--button-border)] bg-[var(--button-bg)] text-[var(--button-text)]"
      }`}
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
      <Icon aria-hidden="true" size={14} className="shrink-0" />
      <span className="min-w-0 break-words">
        {t("interactive.label")}:{" "}
        {interactive ? t("interactive.on") : t("interactive.off")}
      </span>
    </button>
  );
}
