import { ChevronDown, Languages } from "lucide-react";
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { useI18n } from "../i18n/index.js";

function clampIndex(index, length) {
  if (length <= 0) return 0;
  if (index < 0) return 0;
  if (index >= length) return length - 1;
  return index;
}

export function LanguageSwitcher() {
  const { locale, locales, localeLabels, setLocale, t } = useI18n();
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const rootRef = useRef(null);
  const buttonRef = useRef(null);
  const listRef = useRef(null);
  const listId = useId();

  const currentLabel = localeLabels[locale] ?? locale;

  const close = useCallback((restoreFocus = true) => {
    setOpen(false);
    if (restoreFocus) {
      queueMicrotask(() => buttonRef.current?.focus());
    }
  }, []);

  const selectLocale = useCallback(
    (code) => {
      if (!locales.includes(code)) return;
      setLocale(code);
      close(true);
    },
    [close, locales, setLocale],
  );

  const openMenu = useCallback(() => {
    const selected = Math.max(0, locales.indexOf(locale));
    setActiveIndex(selected);
    setOpen(true);
  }, [locale, locales]);

  useEffect(() => {
    if (!open) return undefined;

    const onPointerDown = (event) => {
      if (!rootRef.current?.contains(event.target)) {
        close(false);
      }
    };
    const onFocusIn = (event) => {
      if (!rootRef.current?.contains(event.target)) {
        close(false);
      }
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("focusin", onFocusIn);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("focusin", onFocusIn);
    };
  }, [close, open]);

  useEffect(() => {
    if (!open) return;
    const option = listRef.current?.querySelector(
      `[data-language-option-index="${activeIndex}"]`,
    );
    option?.scrollIntoView?.({ block: "nearest" });
    // Focus the listbox container so arrow keys work without tabbing each option.
    listRef.current?.focus({ preventScroll: true });
  }, [activeIndex, open]);

  const onTriggerKeyDown = (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) openMenu();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) close(true);
      else openMenu();
      return;
    }
    if (event.key === "Escape" && open) {
      event.preventDefault();
      close(true);
    }
  };

  const onListKeyDown = (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close(true);
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      setActiveIndex(0);
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      setActiveIndex(locales.length - 1);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((index) => clampIndex(index + 1, locales.length));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => clampIndex(index - 1, locales.length));
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      const code = locales[activeIndex];
      if (code) selectLocale(code);
      return;
    }
    if (event.key === "Tab") {
      close(false);
    }
  };

  return (
    <div
      ref={rootRef}
      className="lang-switcher relative inline-flex max-w-full"
      data-language-switcher="true"
      data-open={open ? "true" : "false"}
    >
      <button
        ref={buttonRef}
        type="button"
        className="lang-switcher-trigger inline-flex min-h-9 max-w-full items-center gap-1.5 rounded-lg border border-[var(--button-border)] bg-[var(--theme-control-bg)] px-2 py-1 text-[11px] font-semibold text-[var(--button-text)] shadow-[var(--shadow-sm)] outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)]"
        title={t("lang.label")}
        aria-label={t("lang.aria")}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listId}
        data-language-trigger="true"
        onClick={() => {
          if (open) close(true);
          else openMenu();
        }}
        onKeyDown={onTriggerKeyDown}
      >
        <Languages
          aria-hidden="true"
          size={14}
          className="shrink-0 text-[var(--muted)]"
        />
        <span className="min-w-0 max-w-[7.5rem] truncate sm:max-w-[10rem]">
          {currentLabel}
        </span>
        <ChevronDown
          aria-hidden="true"
          size={14}
          className={`shrink-0 text-[var(--muted)] transition-transform duration-150 motion-reduce:transition-none ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open ? (
        <ul
          ref={listRef}
          id={listId}
          role="listbox"
          tabIndex={-1}
          aria-label={t("lang.aria")}
          aria-activedescendant={
            locales[activeIndex]
              ? `${listId}-opt-${locales[activeIndex]}`
              : undefined
          }
          className="lang-menu absolute top-[calc(100%+0.35rem)] end-0 z-40 m-0 max-h-72 min-w-[11rem] max-w-[min(18rem,calc(100vw-1.25rem))] list-none overflow-y-auto overscroll-contain rounded-xl border p-1 shadow-[var(--shadow-md)] outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)]"
          data-language-menu="true"
          onKeyDown={onListKeyDown}
        >
          {locales.map((code, index) => {
            const selected = code === locale;
            const active = index === activeIndex;
            const label = localeLabels[code] ?? code;
            return (
              <li
                key={code}
                id={`${listId}-opt-${code}`}
                role="option"
                aria-selected={selected}
                data-language-option={code}
                data-language-option-index={index}
                data-active={active ? "true" : "false"}
                data-selected={selected ? "true" : "false"}
                className={`lang-menu-option flex cursor-pointer items-center rounded-lg px-2.5 py-2 text-start text-[12px] font-semibold break-words ${
                  selected ? "lang-menu-option-selected" : ""
                } ${active ? "lang-menu-option-active" : ""}`}
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => selectLocale(code)}
              >
                <span className="min-w-0 flex-1 break-words">{label}</span>
                {selected ? (
                  <span className="lang-menu-check ms-2 shrink-0 text-[10px]" aria-hidden="true">
                    ✓
                  </span>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
    </div>
  );
}
