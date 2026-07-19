import { useEffect, useState } from "react";

/**
 * Local wall-clock that ticks once per second while enabled.
 * Used for orphan auto-close countdowns so the UI stays accurate
 * without depending on WebSocket frames. Clears the timer on unmount.
 */
export function useNow(enabled = true) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!enabled) return undefined;
    setNow(Date.now());
    const id = window.setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => window.clearInterval(id);
  }, [enabled]);

  return now;
}
