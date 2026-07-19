import { describe, expect, it } from "vitest";
import {
  MAX_TERMINAL_COLS,
  MAX_TERMINAL_ROWS,
  MIN_TERMINAL_COLS,
  MIN_TERMINAL_ROWS,
  clampTerminalGrid,
} from "./terminalGrid.js";

describe("clampTerminalGrid", () => {
  it("clamps extreme narrow/short and oversized grids to server bounds", () => {
    expect(clampTerminalGrid(1, 1)).toEqual({
      cols: MIN_TERMINAL_COLS,
      rows: MIN_TERMINAL_ROWS,
    });
    expect(clampTerminalGrid(2, 3)).toEqual({
      cols: MIN_TERMINAL_COLS,
      rows: MIN_TERMINAL_ROWS,
    });
    expect(clampTerminalGrid(9999, 9999)).toEqual({
      cols: MAX_TERMINAL_COLS,
      rows: MAX_TERMINAL_ROWS,
    });
    expect(clampTerminalGrid(80, 24)).toEqual({ cols: 80, rows: 24 });
  });
});
