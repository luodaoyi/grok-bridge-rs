import { catalogs } from "./catalog/index.js";
import {
  DEFAULT_LOCALE,
  RTL_LOCALES,
  SUPPORTED_LOCALES,
} from "./constants.js";

/**
 * Replace `{name}` placeholders. Missing params leave the placeholder intact.
 * @param {string} template
 * @param {Record<string, string | number> | undefined} params
 */
export function interpolate(template, params) {
  if (!params) return template;
  return String(template).replace(/\{(\w+)\}/g, (match, key) => {
    if (Object.prototype.hasOwnProperty.call(params, key)) {
      return String(params[key]);
    }
    return match;
  });
}

/**
 * Safe translator: locale catalog → en fallback → raw key.
 * @param {string} locale
 */
export function createTranslator(locale) {
  const active = SUPPORTED_LOCALES.includes(locale) ? locale : DEFAULT_LOCALE;
  const primary = catalogs[active] ?? catalogs[DEFAULT_LOCALE];
  const fallback = catalogs[DEFAULT_LOCALE];

  return function t(key, params) {
    const raw = primary[key] ?? fallback[key] ?? key;
    return interpolate(raw, params);
  };
}

/** Document direction for a supported locale (`rtl` only for Arabic). */
export function localeDirection(locale) {
  const active = SUPPORTED_LOCALES.includes(locale) ? locale : DEFAULT_LOCALE;
  return RTL_LOCALES.includes(active) ? "rtl" : "ltr";
}

export function applyDocumentLocale(locale, t) {
  if (typeof document === "undefined") return;
  const active = SUPPORTED_LOCALES.includes(locale) ? locale : DEFAULT_LOCALE;
  document.documentElement.lang = active;
  document.documentElement.dir = localeDirection(active);
  const translate = t ?? createTranslator(active);
  document.title = translate("doc.title");
  const meta = document.querySelector('meta[name="description"]');
  if (meta) meta.setAttribute("content", translate("doc.description"));
}
