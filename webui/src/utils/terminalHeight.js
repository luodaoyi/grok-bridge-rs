/** Default viewport height for each terminal panel (px). */
export const TERMINAL_HEIGHT_DEFAULT = 280;
/** Smallest useful interactive height (px). */
export const TERMINAL_HEIGHT_MIN = 120;
/** Hard ceiling so one panel cannot dominate the page (px). */
export const TERMINAL_HEIGHT_MAX = 720;
/** Soft ceiling relative to the browser viewport. */
export const TERMINAL_HEIGHT_MAX_VH = 0.7;

export const TERMINAL_HEIGHT_STORAGE_PREFIX = "grok-bridge-terminal-height:";

export function terminalHeightStorageKey(sessionId) {
  return `${TERMINAL_HEIGHT_STORAGE_PREFIX}${sessionId}`;
}

export function maxTerminalHeight(viewportHeight = getViewportHeight()) {
  const vhCap = Math.floor(Number(viewportHeight) * TERMINAL_HEIGHT_MAX_VH);
  const capped = Number.isFinite(vhCap) ? vhCap : TERMINAL_HEIGHT_MAX;
  return Math.max(TERMINAL_HEIGHT_MIN, Math.min(TERMINAL_HEIGHT_MAX, capped));
}

export function clampTerminalHeight(
  height,
  viewportHeight = getViewportHeight(),
) {
  const max = maxTerminalHeight(viewportHeight);
  const value = Math.round(Number(height));
  if (!Number.isFinite(value)) {
    return Math.min(max, TERMINAL_HEIGHT_DEFAULT);
  }
  return Math.min(max, Math.max(TERMINAL_HEIGHT_MIN, value));
}

export function readTerminalHeight(sessionId, storage) {
  if (!sessionId) return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
  try {
    const store =
      storage ??
      (typeof window !== "undefined" ? window.localStorage : null);
    if (!store) return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
    const raw = store.getItem(terminalHeightStorageKey(sessionId));
    if (raw == null) return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
    return clampTerminalHeight(Number(raw));
  } catch {
    return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
  }
}

export function writeTerminalHeight(sessionId, height, storage) {
  if (!sessionId) return;
  const next = clampTerminalHeight(height);
  try {
    const store =
      storage ??
      (typeof window !== "undefined" ? window.localStorage : null);
    if (!store) return;
    store.setItem(terminalHeightStorageKey(sessionId), String(next));
  } catch {
    // ignore private mode
  }
}

/** True when the host has a non-zero box suitable for FitAddon.fit(). */
export function canFitElement(element) {
  if (!element) return false;
  const width = element.clientWidth || element.offsetWidth || 0;
  const height = element.clientHeight || element.offsetHeight || 0;
  return width > 0 && height > 0;
}

function getViewportHeight() {
  if (typeof window === "undefined") return 900;
  return window.innerHeight || 900;
}
