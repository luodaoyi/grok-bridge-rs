import { useCallback, useState } from "react";

/**
 * Global interactive mode for all WebUI terminals.
 * Default OFF on every page load; intentionally NOT persisted.
 */
export function useInteractiveMode() {
  const [interactive, setInteractiveState] = useState(false);

  const setInteractive = useCallback((value) => {
    setInteractiveState(Boolean(value));
  }, []);

  const toggleInteractive = useCallback(() => {
    setInteractiveState((current) => !current);
  }, []);

  return { interactive, setInteractive, toggleInteractive };
}
