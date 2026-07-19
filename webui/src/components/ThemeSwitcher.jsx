import { MonitorCog, Moon, Sun } from "lucide-react";
import { useTheme } from "../hooks/useTheme.js";
import { useI18n } from "../i18n/index.js";

const themeOptions = [
  { value: "auto", labelKey: "theme.auto", Icon: MonitorCog },
  { value: "light", labelKey: "theme.light", Icon: Sun },
  { value: "dark", labelKey: "theme.dark", Icon: Moon },
];

export function ThemeSwitcher() {
  const { theme, setTheme } = useTheme();
  const { t } = useI18n();

  return (
    <div
      className="inline-flex max-w-full flex-wrap rounded-lg border border-[var(--button-border)] bg-[var(--theme-control-bg)] p-0.5 shadow-[var(--shadow-sm)]"
      role="group"
      aria-label={t("theme.aria")}
    >
      {themeOptions.map(({ value, labelKey, Icon }) => {
        const active = theme === value;
        const label = t(labelKey);
        return (
          <button
            key={value}
            type="button"
            className={`inline-flex min-h-8 items-center gap-1 rounded-md px-2.5 py-1 text-[11px] font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)] motion-reduce:transition-none ${
              active
                ? "bg-[var(--theme-active-bg)] text-[var(--theme-active-text)] shadow-[var(--shadow-sm)]"
                : "text-[var(--muted)] hover:text-[var(--text)]"
            }`}
            aria-pressed={active}
            title={t("theme.title", { label })}
            onClick={() => setTheme(value)}
          >
            <Icon aria-hidden="true" size={13} strokeWidth={2} />
            <span>{label}</span>
          </button>
        );
      })}
    </div>
  );
}
