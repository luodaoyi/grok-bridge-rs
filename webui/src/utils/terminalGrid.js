/** Server validate_terminal_size bounds (protocol.rs). */
export const MIN_TERMINAL_COLS = 20;
export const MAX_TERMINAL_COLS = 500;
export const MIN_TERMINAL_ROWS = 5;
export const MAX_TERMINAL_ROWS = 200;

export function clampTerminalCols(cols) {
  const value = Math.floor(Number(cols));
  if (!Number.isFinite(value)) return MIN_TERMINAL_COLS;
  return Math.min(MAX_TERMINAL_COLS, Math.max(MIN_TERMINAL_COLS, value));
}

export function clampTerminalRows(rows) {
  const value = Math.floor(Number(rows));
  if (!Number.isFinite(value)) return MIN_TERMINAL_ROWS;
  return Math.min(MAX_TERMINAL_ROWS, Math.max(MIN_TERMINAL_ROWS, value));
}

/** Clamp a fitted grid to the Runtime-accepted range before terminal_resize. */
export function clampTerminalGrid(cols, rows) {
  return {
    cols: clampTerminalCols(cols),
    rows: clampTerminalRows(rows),
  };
}
