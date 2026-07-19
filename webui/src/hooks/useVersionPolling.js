import { useCallback, useEffect, useState } from "react";
import { getVersionStatus } from "../api.js";
import { VERSION_POLL_MS } from "../utils/constants.js";
import {
  readDismissedUpdate,
  writeDismissedUpdate,
} from "../utils/storage.js";

export function useVersionPolling() {
  const [version, setVersion] = useState(null);
  const [dismissedUpdate, setDismissedUpdate] = useState(readDismissedUpdate);

  useEffect(() => {
    let cancelled = false;
    let timer = 0;

    const loadVersion = async () => {
      try {
        const next = await getVersionStatus();
        if (!cancelled) setVersion(next);
      } catch (error) {
        console.warn("version check failed:", error);
      }
      if (!cancelled) {
        timer = window.setTimeout(loadVersion, VERSION_POLL_MS);
      }
    };

    loadVersion();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, []);

  const visibleUpdate =
    version?.update_available &&
    version.latest &&
    version.latest !== dismissedUpdate
      ? version
      : null;

  const dismissUpdate = useCallback(() => {
    if (!version?.latest) return;
    writeDismissedUpdate(version.latest);
    setDismissedUpdate(version.latest);
  }, [version]);

  return { version, visibleUpdate, dismissUpdate };
}
