import { Check, MonitorCog, Moon, Sun } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
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
  const [open, setOpen] = useState(false);
  const rootRef = useRef(null);
  const buttonRef = useRef(null);
  const menuRef = useRef(null);
  const menuId = useId();
  const activeOption = themeOptions.find((option) => option.value === theme) ?? themeOptions[0];
  const ActiveIcon = activeOption.Icon;
  const activeLabel = t(activeOption.labelKey);

  const close = useCallback((restoreFocus = true) => {
    setOpen(false);
    if (restoreFocus) {
      queueMicrotask(() => buttonRef.current?.focus());
    }
  }, []);

  useEffect(() => {
    if (!open) return undefined;
    const onPointerDown = (event) => {
      if (!rootRef.current?.contains(event.target)) close(false);
    };
    const onFocusIn = (event) => {
      if (!rootRef.current?.contains(event.target)) close(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("focusin", onFocusIn);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("focusin", onFocusIn);
    };
  }, [close, open]);

  const selectTheme = (value) => {
    setTheme(value);
    close(true);
  };

  const onTriggerKeyDown = (event) => {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      setOpen(true);
      return;
    }
    if (event.key === "Escape" && open) {
      event.preventDefault();
      close(true);
    }
  };

  const onMenuKeyDown = (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close(true);
      return;
    }
    if (event.key === "Tab") close(false);
  };

  return (
    <div
      ref={rootRef}
      className="theme-switcher"
      aria-label={t("theme.aria")}
      data-theme-switcher="true"
      data-open={open ? "true" : "false"}
    >
      <button
        ref={buttonRef}
        type="button"
        className="btn btn-sm btn-icon btn-outline-secondary header-icon-button theme-switcher-trigger"
        aria-label={t("theme.aria")}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={menuId}
        title={t("theme.title", { label: activeLabel })}
        data-theme-trigger="true"
        onClick={() => (open ? close(true) : setOpen(true))}
        onKeyDown={onTriggerKeyDown}
      >
        <ActiveIcon aria-hidden="true" size={18} strokeWidth={2} />
      </button>
      {open ? (
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          tabIndex={-1}
          aria-label={t("theme.aria")}
          className="dropdown-menu show theme-menu"
          data-theme-menu="true"
          onKeyDown={onMenuKeyDown}
        >
          {themeOptions.map(({ value, labelKey, Icon }) => {
            const active = theme === value;
            const label = t(labelKey);
            return (
              <button
                key={value}
                type="button"
                role="menuitemradio"
                aria-checked={active}
                className={`dropdown-item theme-menu-option ${active ? "theme-menu-option-selected" : ""}`}
                data-theme-option={value}
                onClick={() => selectTheme(value)}
              >
                <span className="theme-menu-label">
                  <Icon aria-hidden="true" size={16} strokeWidth={2} />
                  <span>{label}</span>
                </span>
                {active ? <Check aria-hidden="true" size={16} className="theme-menu-check" /> : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
