import { describe, expect, it } from "vitest";
import {
  TERMINAL_HEIGHT_DEFAULT,
  TERMINAL_HEIGHT_MAX,
  TERMINAL_HEIGHT_MIN,
  canFitElement,
  clampTerminalHeight,
  maxTerminalHeight,
  readTerminalHeight,
  subscribeTerminalHeight,
  writeTerminalHeight,
} from "./terminalHeight.js";

function memoryStorage() {
  return {
    data: {},
    getItem(key) {
      return this.data[key] ?? null;
    },
    setItem(key, value) {
      this.data[key] = String(value);
    },
  };
}

describe("terminalHeight", () => {
  it("defaults to doubled 560px height under clamp", () => {
    expect(TERMINAL_HEIGHT_DEFAULT).toBe(560);
    expect(clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT, 2000)).toBe(560);
    expect(readTerminalHeight("client:thread-a", memoryStorage())).toBe(
      clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT),
    );
  });

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

  it("persists height per supervisor group, not per session", () => {
    const storage = memoryStorage();
    writeTerminalHeight("client:thread-a", 400, storage);
    writeTerminalHeight("client:thread-b", 160, storage);
    expect(readTerminalHeight("client:thread-a", storage)).toBe(400);
    expect(readTerminalHeight("client:thread-b", storage)).toBe(160);
    // Same group key always shares one stored value.
    expect(readTerminalHeight("client:thread-a", storage)).toBe(400);
    expect(readTerminalHeight("missing-group", storage)).toBe(
      clampTerminalHeight(TERMINAL_HEIGHT_DEFAULT),
    );
  });

  it("notifies same-group subscribers immediately and isolates other groups", () => {
    const storage = memoryStorage();
    const seenA = [];
    const seenB = [];
    const unsubA = subscribeTerminalHeight("client:a", (h) => seenA.push(h));
    const unsubB = subscribeTerminalHeight("client:b", (h) => seenB.push(h));

    writeTerminalHeight("client:a", 320, storage);
    writeTerminalHeight("client:a", 400, storage);
    writeTerminalHeight("client:b", 200, storage);

    expect(seenA).toEqual([320, 400]);
    expect(seenB).toEqual([200]);
    expect(readTerminalHeight("client:a", storage)).toBe(400);
    expect(readTerminalHeight("client:b", storage)).toBe(200);

    unsubA();
    unsubB();
    writeTerminalHeight("client:a", 360, storage);
    expect(seenA).toEqual([320, 400]);
  });

  it("rejects zero-size hosts for fit", () => {
    expect(canFitElement(null)).toBe(false);
    expect(
      canFitElement({
        clientWidth: 0,
        clientHeight: 0,
        offsetWidth: 0,
        offsetHeight: 0,
      }),
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
