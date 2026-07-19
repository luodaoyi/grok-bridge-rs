import { describe, expect, it } from "vitest";
import {
  TERMINAL_HEIGHT_DEFAULT,
  TERMINAL_HEIGHT_MAX,
  TERMINAL_HEIGHT_MIN,
  canFitElement,
  clampTerminalHeight,
  maxTerminalHeight,
  readTerminalHeight,
  writeTerminalHeight,
} from "./terminalHeight.js";

describe("terminalHeight", () => {
  it("clamps to min/max and viewport cap", () => {
    expect(clampTerminalHeight(10)).toBe(TERMINAL_HEIGHT_MIN);
    expect(clampTerminalHeight(9999, 2000)).toBe(TERMINAL_HEIGHT_MAX);
    expect(clampTerminalHeight(9999, 400)).toBe(maxTerminalHeight(400));
    expect(clampTerminalHeight(Number.NaN)).toBe(
      Math.min(maxTerminalHeight(), TERMINAL_HEIGHT_DEFAULT),
    );
    expect(maxTerminalHeight(1000)).toBe(
      Math.min(TERMINAL_HEIGHT_MAX, Math.floor(1000 * 0.7)),
    );
  });

  it("persists height per session independently", () => {
    const storage = {
      data: {},
      getItem(key) {
        return this.data[key] ?? null;
      },
      setItem(key, value) {
        this.data[key] = String(value);
      },
    };
    writeTerminalHeight("a", 400, storage);
    writeTerminalHeight("b", 160, storage);
    expect(readTerminalHeight("a", storage)).toBe(400);
    expect(readTerminalHeight("b", storage)).toBe(160);
    expect(readTerminalHeight("missing", storage)).toBe(
      clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT),
    );
  });

  it("rejects zero-size hosts for fit", () => {
    expect(canFitElement(null)).toBe(false);
    expect(
      canFitElement({ clientWidth: 0, clientHeight: 0, offsetWidth: 0, offsetHeight: 0 }),
    ).toBe(false);
    expect(
      canFitElement({
        clientWidth: 640,
        clientHeight: 200,
        offsetWidth: 640,
        offsetHeight: 200,
      }),
    ).toBe(true);
  });
});
