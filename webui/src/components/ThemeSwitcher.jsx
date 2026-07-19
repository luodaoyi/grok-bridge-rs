import { MonitorCog, Moon, Sun } from "lucide-react";
import { useTheme } from "../hooks/useTheme.js";

const themeOptions = [
  { value: "auto", label: "自动", Icon: MonitorCog },
  { value: "light", label: "浅色", Icon: Sun },
  { value: "dark", label: "深色", Icon: Moon },
];

export function ThemeSwitcher() {
  const { theme, setTheme } = useTheme();

  return (
    <div
      className="inline-flex rounded-lg border border-[var(--button-border)] bg-[var(--theme-control-bg)] p-0.5 shadow-[var(--shadow-sm)]"
      role="group"
      aria-label="颜色主题"
    >
      {themeOptions.map(({ value, label, Icon }) => {
        const active = theme === value;
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
            title={`${label}主题`}
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
