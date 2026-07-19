import { createTranslator } from "../i18n/translate.js";

/**
 * Localize known client-side error wrappers; keep backend/detail messages as-is.
 * @param {unknown} error
 * @param {(key: string, params?: Record<string, string | number>) => string} [t]
 */
export function errorMessage(error, t = createTranslator("en")) {
  if (!error) return t("error.unknown");
  if (error.name === "AbortError") return t("error.timeout");
  return error.message || String(error);
}
