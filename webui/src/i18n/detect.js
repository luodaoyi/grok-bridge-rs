import {
  DEFAULT_LOCALE,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES,
} from "./constants.js";

/**
 * Map a BCP 47 tag to a supported locale, or null if unsupported.
 * zh-Hant / TW → zh-TW; zh-Hans / zh / CN → zh-CN;
 * pt-BR / BR → pt-BR; other Portuguese tags (pt, pt-PT, pt-AO, …) → pt-PT;
 * others by primary language.
 */
export function normalizeLocale(tag) {
  if (tag == null || tag === "") return null;
  const raw = String(tag).trim();
  if (!raw) return null;

  const lower = raw.toLowerCase().replace(/_/g, "-");
  for (const locale of SUPPORTED_LOCALES) {
    if (lower === locale.toLowerCase()) return locale;
  }

  const parts = lower.split("-").filter(Boolean);
  if (parts.length === 0) return null;
  const lang = parts[0];

  if (lang === "zh") {
    if (
      parts.includes("hant") ||
      parts.includes("tw") ||
      parts.includes("hk") ||
      parts.includes("mo")
    ) {
      return "zh-TW";
    }
    // zh, zh-Hans, zh-CN, zh-SG, plain zh → zh-CN
    return "zh-CN";
  }

  if (lang === "pt") {
    // pt-BR, pt-Latn-BR, and other Brazil region tags → pt-BR
    if (parts.includes("br")) return "pt-BR";
    // pt, pt-PT, pt-AO, pt-MZ, … → European Portuguese catalog
    return "pt-PT";
  }

  const primary = {
    en: "en",
    fr: "fr",
    de: "de",
    ru: "ru",
    ja: "ja",
    ko: "ko",
    es: "es",
    id: "id",
    th: "th",
    vi: "vi",
    ar: "ar",
  };
  return primary[lang] ?? null;
}

/**
 * Detect locale from navigator.languages (or a provided list).
 * Unsupported tags fall through; if none match, returns en.
 */
export function detectLocale(languages) {
  const list =
    languages ??
    (typeof navigator !== "undefined"
      ? navigator.languages?.length
        ? [...navigator.languages]
        : navigator.language
          ? [navigator.language]
          : []
      : []);

  for (const tag of list) {
    const normalized = normalizeLocale(tag);
    if (normalized) return normalized;
  }
  return DEFAULT_LOCALE;
}

export function readStoredLocale(storage) {
  try {
    const store =
      storage ??
      (typeof window !== "undefined" ? window.localStorage : null);
    if (!store) return null;
    const stored = store.getItem(LOCALE_STORAGE_KEY);
    if (stored && SUPPORTED_LOCALES.includes(stored)) return stored;
  } catch {
    // private mode / blocked storage
  }
  return null;
}

export function writeStoredLocale(locale, storage) {
  if (!SUPPORTED_LOCALES.includes(locale)) return;
  try {
    const store =
      storage ??
      (typeof window !== "undefined" ? window.localStorage : null);
    if (!store) return;
    store.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // ignore
  }
}

/** Stored preference wins; otherwise navigator detection; else en. */
export function resolveLocale(options = {}) {
  const stored = readStoredLocale(options.storage);
  if (stored) return stored;
  return detectLocale(options.languages);
}
