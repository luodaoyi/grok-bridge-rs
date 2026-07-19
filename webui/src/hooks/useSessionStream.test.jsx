import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSessionStream } from "./useSessionStream.js";
import { installMockWebSocket, MockWebSocket } from "../test/mockWebSocket.js";
import {
  peekTerminalBuffer,
  resetTerminalFeeds,
} from "../utils/terminalFeeds.js";
import { WS_BACKOFF_MS } from "../utils/constants.js";

function Probe({ onState }) {
  const state = useSessionStream();
  onState(state);
  return null;
}

describe("useSessionStream", () => {
  let container;
  let root;
  let latest;

  beforeEach(() => {
    vi.useFakeTimers();
    installMockWebSocket();
    resetTerminalFeeds();
    latest = null;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    resetTerminalFeeds();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  async function mount() {
    await act(async () => {
      root.render(
        <Probe
          onState={(state) => {
            latest = state;
          }}
        />,
      );
    });
    await act(async () => {
      await Promise.resolve();
    });
  }

  it("connects immediately to same-origin /api/events without GET /api/sessions", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    await mount();
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(MockWebSocket.instances[0].url).toMatch(/\/api\/events$/);
    expect(latest.connectionState).toBe("initial");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("applies initial sessions snapshot and terminal reset entries", async () => {
    await mount();
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());
    expect(latest.connectionState).toBe("connected");

    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [
          {
            session: "gbt-1",
            phase: "running",
            activity: "working",
            rows: 30,
            cols: 100,
            updated_at_ms: 1,
          },
        ],
        terminals: [
          {
            session: "gbt-1",
            reset: true,
            cursor: 0,
            next_cursor: 5,
            data_base64: btoa("hello"),
          },
        ],
      });
    });

    expect(latest.sessions).toHaveLength(1);
    expect(latest.sessions[0].session).toBe("gbt-1");
    expect(peekTerminalBuffer("gbt-1")).toHaveLength(1);
    expect(peekTerminalBuffer("gbt-1")[0].reset).toBe(true);
  });

  it("applies ordered appends and later reset", async () => {
    await mount();
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());

    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [{ session: "gbt-1", phase: "running", rows: 24, cols: 80 }],
        terminals: [
          {
            session: "gbt-1",
            reset: true,
            data_base64: btoa("A"),
          },
        ],
      });
    });
    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [{ session: "gbt-1", phase: "running", rows: 24, cols: 80 }],
        terminals: [
          {
            session: "gbt-1",
            reset: false,
            data_base64: btoa("B"),
          },
          {
            session: "gbt-1",
            reset: false,
            data_base64: btoa("C"),
          },
        ],
      });
    });
    expect(peekTerminalBuffer("gbt-1").map((e) => e.data_base64)).toEqual([
      btoa("A"),
      btoa("B"),
      btoa("C"),
    ]);

    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [{ session: "gbt-1", phase: "idle", rows: 24, cols: 80 }],
        terminals: [
          {
            session: "gbt-1",
            reset: true,
            data_base64: btoa("RESET"),
          },
        ],
      });
    });
    expect(peekTerminalBuffer("gbt-1")).toHaveLength(1);
    expect(peekTerminalBuffer("gbt-1")[0].data_base64).toBe(btoa("RESET"));
  });

  it("disposes terminal feeds when sessions disappear from push", async () => {
    await mount();
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());
    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [{ session: "gbt-1", phase: "running" }],
        terminals: [
          { session: "gbt-1", reset: true, data_base64: btoa("x") },
        ],
      });
    });
    expect(peekTerminalBuffer("gbt-1")).toHaveLength(1);

    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [],
        terminals: [],
      });
    });
    expect(latest.sessions).toHaveLength(0);
    expect(peekTerminalBuffer("gbt-1")).toHaveLength(0);
  });

  it("reconnects with bounded exponential backoff and supports manual reconnect", async () => {
    await mount();
    const first = MockWebSocket.instances[0];
    await act(async () => first.open());
    await act(async () => first.close());
    expect(latest.connectionState).toBe("retrying");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(WS_BACKOFF_MS[0] - 1);
    });
    expect(MockWebSocket.instances).toHaveLength(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(MockWebSocket.instances).toHaveLength(2);

    await act(async () => {
      latest.reconnect();
    });
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(3);
    const manual = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    await act(async () => manual.open());
    expect(latest.connectionState).toBe("connected");
  });

  it("does not poll GET /api/sessions on a two-second interval", async () => {
    const fetchSpy = vi.fn(async () => new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchSpy);
    await mount();
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(
      fetchSpy.mock.calls.some((call) => String(call[0]).includes("/api/sessions")),
    ).toBe(false);
  });
});
