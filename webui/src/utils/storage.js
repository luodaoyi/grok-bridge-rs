import { DISMISS_UPDATE_KEY } from "./constants.js";

export function readDismissedUpdate() {
  try {
    return window.localStorage.getItem(DISMISS_UPDATE_KEY);
  } catch {
    return null;
  }
}

export function writeDismissedUpdate(version) {
  try {
    if (version) window.localStorage.setItem(DISMISS_UPDATE_KEY, version);
    else window.localStorage.removeItem(DISMISS_UPDATE_KEY);
  } catch {
    // ignore storage failures in private mode
  }
}
