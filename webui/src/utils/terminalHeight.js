/** Default viewport height for each terminal panel (px). */
export const TERMINAL_HEIGHT_DEFAULT = 560;
/** Smallest useful interactive height (px). */
export const TERMINAL_HEIGHT_MIN = 120;
/** Hard ceiling so one panel cannot dominate the page (px). */
export const TERMINAL_HEIGHT_MAX = 720;
/** Soft ceiling relative to the browser viewport. */
export const TERMINAL_HEIGHT_MAX_VH = 0.7;

/** Group-scoped height (one value per Codex supervisor group). */
export const TERMINAL_HEIGHT_STORAGE_PREFIX = "grok-bridge-group-terminal-height:";

/** In-tab listeners so every terminal in a group shares height immediately. */
const heightListeners = new Map();

export function terminalHeightStorageKey(groupKey) {
  return `${TERMINAL_HEIGHT_STORAGE_PREFIX}${groupKey}`;
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

export function readTerminalHeight(groupKey, storage) {
  if (!groupKey) return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
  try {
    const store =
      storage ??
      (typeof window !== "undefined" ? window.localStorage : null);
    if (!store) return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
    const raw = store.getItem(terminalHeightStorageKey(groupKey));
    if (raw == null) return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
    return clampTerminalHeight(Number(raw));
  } catch {
    return clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
  }
}

export function writeTerminalHeight(groupKey, height, storage) {
  if (!groupKey) return clampTerminalHeight(height);
  const next = clampTerminalHeight(height);
  try {
    const store =
      storage ??
      (typeof window !== "undefined" ? window.localStorage : null);
    if (store) {
      store.setItem(terminalHeightStorageKey(groupKey), String(next));
    }
  } catch {
    // ignore private mode
  }
  notifyTerminalHeight(groupKey, next);
  return next;
}

/**
 * Subscribe to in-tab height changes for a supervisor group.
 * Returns an unsubscribe function.
 */
export function subscribeTerminalHeight(groupKey, listener) {
  if (!groupKey || typeof listener !== "function") {
    return () => {};
  }
  let set = heightListeners.get(groupKey);
  if (!set) {
    set = new Set();
    heightListeners.set(groupKey, set);
  }
  set.add(listener);
  return () => {
    set.delete(listener);
    if (set.size === 0) {
      heightListeners.delete(groupKey);
    }
  };
}

function notifyTerminalHeight(groupKey, height) {
  const set = heightListeners.get(groupKey);
  if (!set) return;
  for (const listener of set) {
    listener(height);
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
