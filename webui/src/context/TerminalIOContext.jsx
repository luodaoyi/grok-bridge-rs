import { createContext, useContext } from "react";

/**
 * Shared WebSocket I/O + interactive flag for all terminal panels.
 * interactive is session-scoped to the page load (not persisted).
 */
export const TerminalIOContext = createContext({
  interactive: false,
  setInteractive: () => {},
  connectionState: "initial",
  sendTerminalInput: () => ({ ok: false, error: "send_failed" }),
  sendTerminalResize: () => ({ ok: false, error: "send_failed" }),
});

export function useTerminalIO() {
  return useContext(TerminalIOContext);
}
