export const SUPPORTED_LOCALES = [
  "zh-CN",
  "zh-TW",
  "en",
  "ru",
  "es",
  "fr",
  "de",
  "ja",
  "ko",
  "id",
  "vi",
  "th",
  "ar",
  "pt-BR",
  "pt-PT",
];

export const DEFAULT_LOCALE = "en";

export const LOCALE_STORAGE_KEY = "grok-bridge-locale";

/** Native display names for the language switcher (not translated). */
export const LOCALE_LABELS = {
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
  fr: "Français",
  de: "Deutsch",
  ru: "Русский",
  ja: "日本語",
  ko: "한국어",
  es: "Español",
  id: "Bahasa Indonesia",
  th: "ไทย",
  vi: "Tiếng Việt",
  ar: "العربية",
  "pt-BR": "Português (Brasil)",
  "pt-PT": "Português (Portugal)",
};

/** Locales that require right-to-left document direction. */
export const RTL_LOCALES = ["ar"];
