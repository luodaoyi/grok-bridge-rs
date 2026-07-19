import { afterEach, describe, expect, it, vi } from "vitest";
import {
  disposeTerminalSession,
  peekTerminalBuffer,
  pushTerminalEntries,
  reconcileTerminalSessions,
  resetTerminalFeeds,
  subscribeTerminal,
} from "./terminalFeeds.js";

afterEach(() => {
  resetTerminalFeeds();
});

describe("terminalFeeds", () => {
  it("replays bounded pre-subscription backlog then releases it", () => {
    pushTerminalEntries([
      {
        session: "s1",
        reset: true,
        data_base64: btoa("SNAP"),
        cursor: 0,
        next_cursor: 4,
      },
      {
        session: "s1",
        reset: false,
        data_base64: btoa("A"),
        cursor: 4,
        next_cursor: 5,
      },
    ]);
    expect(peekTerminalBuffer("s1")).toHaveLength(2);

    const received = [];
    const unsubscribe = subscribeTerminal("s1", (entry) => received.push(entry));
    expect(received).toHaveLength(2);
    expect(received[0].reset).toBe(true);
    expect(received[1].data_base64).toBe(btoa("A"));
    // Backlog released immediately after replay.
    expect(peekTerminalBuffer("s1")).toHaveLength(0);

    pushTerminalEntries([
      {
        session: "s1",
        reset: false,
        data_base64: btoa("B"),
        cursor: 5,
        next_cursor: 6,
      },
    ]);
    expect(received).toHaveLength(3);
    // Live traffic is not retained.
    expect(peekTerminalBuffer("s1")).toHaveLength(0);
    unsubscribe();
  });

  it("does not accumulate entries while live subscribers exist", () => {
    const received = [];
    subscribeTerminal("s1", (entry) => received.push(entry));

    for (let i = 0; i < 200; i += 1) {
      pushTerminalEntries([
        {
          session: "s1",
          reset: i === 0,
          data_base64: btoa(String(i)),
        },
      ]);
    }

    expect(received).toHaveLength(200);
    expect(peekTerminalBuffer("s1")).toHaveLength(0);
  });

  it("late initial mount still replays last reset plus subsequent in order", () => {
    pushTerminalEntries([
      { session: "s1", reset: true, data_base64: btoa("old") },
      { session: "s1", reset: false, data_base64: btoa("x") },
      { session: "s1", reset: true, data_base64: btoa("SNAP") },
      { session: "s1", reset: false, data_base64: btoa("A") },
      { session: "s1", reset: false, data_base64: btoa("B") },
    ]);
    // Bounded: last reset + subsequent only.
    expect(peekTerminalBuffer("s1").map((e) => e.data_base64)).toEqual([
      btoa("SNAP"),
      btoa("A"),
      btoa("B"),
    ]);

    const received = [];
    subscribeTerminal("s1", (entry) => received.push(entry.data_base64));
    expect(received).toEqual([btoa("SNAP"), btoa("A"), btoa("B")]);
    expect(peekTerminalBuffer("s1")).toHaveLength(0);
  });

  it("clears backlog on reset=true and disposes removed sessions", () => {
    pushTerminalEntries([
      { session: "s1", reset: true, data_base64: btoa("old") },
      { session: "s1", reset: false, data_base64: btoa("x") },
    ]);
    pushTerminalEntries([
      { session: "s1", reset: true, data_base64: btoa("new") },
    ]);
    expect(peekTerminalBuffer("s1")).toHaveLength(1);
    expect(peekTerminalBuffer("s1")[0].data_base64).toBe(btoa("new"));

    reconcileTerminalSessions(new Set());
    expect(peekTerminalBuffer("s1")).toHaveLength(0);

    const listener = vi.fn();
    subscribeTerminal("gone", listener);
    disposeTerminalSession("gone");
    pushTerminalEntries([
      { session: "gone", reset: true, data_base64: btoa("z") },
    ]);
    expect(listener).not.toHaveBeenCalled();
  });

  it("rebuilds remount backlog only after all listeners leave", () => {
    const first = [];
    const unsubscribe = subscribeTerminal("s1", (entry) => first.push(entry));
    pushTerminalEntries([
      { session: "s1", reset: true, data_base64: btoa("live") },
    ]);
    expect(peekTerminalBuffer("s1")).toHaveLength(0);
    unsubscribe();

    pushTerminalEntries([
      { session: "s1", reset: true, data_base64: btoa("backlog") },
      { session: "s1", reset: false, data_base64: btoa("delta") },
    ]);
    expect(peekTerminalBuffer("s1")).toHaveLength(2);

    const second = [];
    subscribeTerminal("s1", (entry) => second.push(entry.data_base64));
    expect(second).toEqual([btoa("backlog"), btoa("delta")]);
    expect(peekTerminalBuffer("s1")).toHaveLength(0);
  });
});
