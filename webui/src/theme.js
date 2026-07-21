export const THEME_KEY = "grok-bridge-theme";
export const THEMES = ["auto", "light", "dark"];

export function isTheme(value) {
  return THEMES.includes(value);
}

export function resolveTheme(preference, prefersDark) {
  return preference === "auto" ? (prefersDark ? "dark" : "light") : preference;
}

export function readTheme() {
  const fromDocument = document.documentElement.dataset.theme;
  if (isTheme(fromDocument)) return fromDocument;
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (isTheme(stored)) return stored;
  } catch {}
  return "auto";
}

export function applyTheme(preference, mediaQuery) {
  const safePreference = isTheme(preference) ? preference : "auto";
  const resolved = resolveTheme(safePreference, mediaQuery.matches);
  const root = document.documentElement;
  root.dataset.theme = safePreference;
  root.dataset.resolvedTheme = resolved;
  root.dataset.bsTheme = resolved;
  root.style.colorScheme = resolved;
  const colorScheme = document.querySelector('meta[name="color-scheme"]');
  if (colorScheme) colorScheme.content = resolved;
  try {
    localStorage.setItem(THEME_KEY, safePreference);
  } catch {}
  return safePreference;
}
