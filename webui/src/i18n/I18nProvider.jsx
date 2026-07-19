import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react";
import {
  LOCALE_LABELS,
  SUPPORTED_LOCALES,
} from "./constants.js";
import { resolveLocale, writeStoredLocale } from "./detect.js";
import {
  formatAge,
  formatClock,
  formatNumber,
  formatRemaining,
} from "./format.js";
import { applyDocumentLocale, createTranslator } from "./translate.js";

const I18nContext = createContext(null);

function buildValue(locale, setLocale) {
  const t = createTranslator(locale);
  return {
    locale,
    locales: SUPPORTED_LOCALES,
    localeLabels: LOCALE_LABELS,
    setLocale,
    t,
    formatNumber: (value) => formatNumber(value, locale),
    formatClock: (date) => formatClock(date, locale),
    formatAge: (updatedAt, now) => formatAge(updatedAt, now, locale),
    formatRemaining: (deadline, now) => formatRemaining(deadline, now, locale),
  };
}

export function I18nProvider({ children, initialLocale }) {
  const [locale, setLocaleState] = useState(() => {
    const resolved = initialLocale ?? resolveLocale();
    applyDocumentLocale(resolved);
    return resolved;
  });

  const setLocale = useCallback((next) => {
    if (!SUPPORTED_LOCALES.includes(next)) return;
    writeStoredLocale(next);
    applyDocumentLocale(next);
    setLocaleState(next);
  }, []);

  const value = useMemo(
    () => buildValue(locale, setLocale),
    [locale, setLocale],
  );

  return (
    <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
  );
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    // Safe fallback for tests / early render outside provider.
    const locale = resolveLocale();
    return buildValue(locale, () => {});
  }
  return ctx;
}
