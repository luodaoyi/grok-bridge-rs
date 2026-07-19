import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockXTerm } from "../test/mockXterm.js";
import {
  pushTerminalEntries,
  resetTerminalFeeds,
} from "../utils/terminalFeeds.js";
import { createTerminalWriteQueue } from "./Terminal.jsx";

vi.mock("@xterm/xterm", () => ({
  Terminal: MockXTerm,
}));

import { Terminal } from "./Terminal.jsx";

function decodeChunk(chunk) {
  return typeof chunk === "string"
    ? chunk
    : new TextDecoder().decode(chunk);
}

describe("Terminal (xterm read-only)", () => {
  let container;
  let root;

  beforeEach(() => {
    MockXTerm.reset();
    resetTerminalFeeds();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    if (root) {
      await act(async () => {
        try {
          root.unmount();
        } catch {
          /* already unmounted by a test */
        }
      });
      root = null;
    }
    container?.remove();
    container = null;
    resetTerminalFeeds();
    MockXTerm.reset();
  });

  async function renderTerminal(props = {}) {
    await act(async () => {
      root.render(
        <Terminal
          id={props.id ?? "gbt-1"}
          rows={props.rows ?? 24}
          cols={props.cols ?? 80}
          label={props.label ?? "term"}
        />,
      );
    });
    await act(async () => {
      await Promise.resolve();
    });
  }

  it("configures disableStdin and never registers input handlers", async () => {
    await renderTerminal();
    expect(MockXTerm.instances).toHaveLength(1);
    const term = MockXTerm.instances[0];
    expect(term.options.disableStdin).toBe(true);
    expect(term.options.cursorBlink).toBe(false);
    expect(term.handlers.data).toHaveLength(0);
    expect(term.handlers.key).toHaveLength(0);
    expect(term.opened).toBe(true);
  });

  it("applies initial reset snapshot then ordered appends", async () => {
    pushTerminalEntries([
      {
        session: "gbt-1",
        reset: true,
        data_base64: btoa("SNAP"),
      },
    ]);
    await renderTerminal();
    const term = MockXTerm.instances[0];
    expect(term.resetCount).toBe(1);
    expect(term.written).toHaveLength(1);
    expect(Array.from(term.written[0])).toEqual(
      Array.from(new TextEncoder().encode("SNAP")),
    );

    await act(async () => {
      pushTerminalEntries([
        {
          session: "gbt-1",
          reset: false,
          data_base64: btoa("A"),
        },
        {
          session: "gbt-1",
          reset: false,
          data_base64: btoa("B"),
        },
      ]);
    });
    expect(term.written).toHaveLength(3);
    expect(Array.from(term.written[1])).toEqual([65]);
    expect(Array.from(term.written[2])).toEqual([66]);
  });

  it("resets on reset=true without disposing the instance", async () => {
    await renderTerminal();
    const term = MockXTerm.instances[0];
    await act(async () => {
      pushTerminalEntries([
        { session: "gbt-1", reset: true, data_base64: btoa("one") },
      ]);
    });
    await act(async () => {
      pushTerminalEntries([
        { session: "gbt-1", reset: false, data_base64: btoa("two") },
      ]);
    });
    await act(async () => {
      pushTerminalEntries([
        { session: "gbt-1", reset: true, data_base64: btoa("three") },
      ]);
    });
    expect(term.resetCount).toBe(2);
    expect(term.disposed).toBe(false);
    expect(term.written).toHaveLength(1);
    expect(Array.from(term.written[0])).toEqual(
      Array.from(new TextEncoder().encode("three")),
    );
  });

  it("FIFO: reset waits for prior write callback and completes snapshot before later append", async () => {
    await renderTerminal();
    const term = MockXTerm.instances[0];
    term.holdWriteCallbacks = true;

    await act(async () => {
      pushTerminalEntries([
        { session: "gbt-1", reset: false, data_base64: btoa("A") },
        { session: "gbt-1", reset: true, data_base64: btoa("SNAP") },
        { session: "gbt-1", reset: false, data_base64: btoa("B") },
      ]);
    });

    // Only append A has entered write(); reset must not have run yet.
    expect(term.ops.map((op) => op[0])).toEqual(["write"]);
    expect(decodeChunk(term.ops[0][1])).toBe("A");
    expect(term.resetCount).toBe(0);
    expect(term.pendingCallbacks).toHaveLength(1);

    await act(async () => {
      term.flushWriteCallback();
    });
    // Now reset at exact queue position, then SNAP write is pending.
    expect(term.ops.map((op) => op[0])).toEqual(["write", "reset", "write"]);
    expect(decodeChunk(term.ops[2][1])).toBe("SNAP");
    expect(term.resetCount).toBe(1);
    expect(term.pendingCallbacks).toHaveLength(1);
    // B must not appear until SNAP callback fires.
    expect(term.ops.some((op) => op[0] === "write" && decodeChunk(op[1]) === "B")).toBe(
      false,
    );

    await act(async () => {
      term.flushWriteCallback();
    });
    expect(term.ops.map((op) => op[0])).toEqual([
      "write",
      "reset",
      "write",
      "write",
    ]);
    expect(decodeChunk(term.ops[3][1])).toBe("B");

    await act(async () => {
      term.flushWriteCallback();
    });
    expect(term.pendingCallbacks).toHaveLength(0);
  });

  it("FIFO unit: append -> reset -> append order with held callbacks", () => {
    const term = new MockXTerm();
    term.holdWriteCallbacks = true;
    const queue = createTerminalWriteQueue(term);

    queue.enqueue({ reset: false, data_base64: btoa("A") });
    queue.enqueue({ reset: true, data_base64: btoa("SNAP") });
    queue.enqueue({ reset: false, data_base64: btoa("B") });

    expect(term.ops.map((op) => op[0])).toEqual(["write"]);
    expect(decodeChunk(term.ops[0][1])).toBe("A");

    term.flushWriteCallback();
    expect(term.ops.map((op) => op[0])).toEqual(["write", "reset", "write"]);
    expect(decodeChunk(term.ops[2][1])).toBe("SNAP");

    term.flushWriteCallback();
    expect(term.ops.map((op) => op[0])).toEqual([
      "write",
      "reset",
      "write",
      "write",
    ]);
    expect(decodeChunk(term.ops[3][1])).toBe("B");

    term.flushWriteCallback();
    expect(term.pendingCallbacks).toHaveLength(0);
    queue.dispose();
  });

  it("dispose drops pending queue entries even if prior write callback later fires", async () => {
    await renderTerminal();
    const term = MockXTerm.instances[0];
    term.holdWriteCallbacks = true;

    await act(async () => {
      pushTerminalEntries([
        { session: "gbt-1", reset: false, data_base64: btoa("A") },
        { session: "gbt-1", reset: false, data_base64: btoa("B") },
        { session: "gbt-1", reset: true, data_base64: btoa("LATE") },
      ]);
    });
    expect(decodeChunk(term.ops[0][1])).toBe("A");
    expect(term.ops).toHaveLength(1);

    await act(async () => root.unmount());
    expect(term.disposed).toBe(true);

    await act(async () => {
      term.flushAllWriteCallbacks();
    });
    // No further writes or resets after dispose.
    expect(term.ops.map((op) => op[0])).toEqual(["write"]);
    expect(term.resetCount).toBe(0);
  });

  it("resizes renderer from server rows/cols and disposes on unmount", async () => {
    await renderTerminal({ rows: 20, cols: 90 });
    const term = MockXTerm.instances[0];
    expect(term.rows).toBe(20);
    expect(term.cols).toBe(90);

    await act(async () => {
      root.render(<Terminal id="gbt-1" rows={40} cols={120} label="term" />);
    });
    expect(term.rows).toBe(40);
    expect(term.cols).toBe(120);

    await act(async () => root.unmount());
    expect(term.disposed).toBe(true);
  });
});
