import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { catalogs, I18nProvider } from "../i18n/index.js";
import { installMockWebSocket, MockWebSocket } from "../test/mockWebSocket.js";
import { WS_BACKOFF_MS } from "../utils/constants.js";
import {
  peekTerminalBuffer,
  resetTerminalFeeds,
} from "../utils/terminalFeeds.js";
import { CLIENT_IO_ERROR, useSessionStream } from "./useSessionStream.js";

function Probe({ onState, setNotice }) {
  const state = useSessionStream({ setNotice });
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

  async function mount({ locale = "en", setNotice } = {}) {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale={locale}>
          <Probe
            setNotice={setNotice}
            onState={(state) => {
              latest = state;
            }}
          />
        </I18nProvider>,
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

  it("sends terminal_input/resize without buffering when disconnected", async () => {
    await mount();
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());
    const ok = latest.sendTerminalInput("gbt-1", btoa("x"));
    expect(ok.ok).toBe(true);
    expect(ws.sent.some((item) => String(item).includes("terminal_input"))).toBe(
      true,
    );
    const resize = latest.sendTerminalResize("gbt-1", 80, 24);
    expect(resize.ok).toBe(true);
    expect(ws.sent.some((item) => String(item).includes("terminal_resize"))).toBe(
      true,
    );

    await act(async () => ws.close());
    const fail = latest.sendTerminalInput("gbt-1", btoa("y"));
    expect(fail.ok).toBe(false);
    expect(fail.error).toBe(CLIENT_IO_ERROR.DISCONNECTED);
  });

  it("localizes client send failures without leaking English homemade strings", async () => {
    let notice = null;
    await mount({
      locale: "zh-CN",
      setNotice: (value) => {
        notice = typeof value === "function" ? value(notice) : value;
      },
    });
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());

    await act(async () => {
      latest.sendTerminalInput("", "");
    });
    expect(notice?.text).toBe(catalogs["zh-CN"]["interactive.invalidPayload"]);
    expect(notice?.text).not.toMatch(/invalid payload|Invalid terminal/i);

    await act(async () => ws.close());
    await act(async () => {
      latest.sendTerminalInput("gbt-1", btoa("y"));
    });
    expect(notice?.text).toBe(catalogs["zh-CN"]["interactive.disconnected"]);
    expect(notice?.text).not.toMatch(/disconnected|Live channel/i);

    // Force a send exception path.
    await mount({
      locale: "zh-CN",
      setNotice: (value) => {
        notice = typeof value === "function" ? value(notice) : value;
      },
    });
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    await act(async () => ws2.open());
    ws2.send = () => {
      throw new Error("WebSocket is already in CLOSING or CLOSED state");
    };
    await act(async () => {
      latest.sendTerminalResize("gbt-1", 80, 24);
    });
    expect(notice?.text).toBe(catalogs["zh-CN"]["interactive.sendFailed"]);
    expect(notice?.text).not.toMatch(
      /WebSocket is already|Failed to send|CLOSING/i,
    );
  });

  it("keeps backend input_result detail after a localized prefix", async () => {
    let notice = null;
    await mount({
      locale: "zh-CN",
      setNotice: (value) => {
        notice = typeof value === "function" ? value(notice) : value;
      },
    });
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());
    await act(async () => {
      ws.emitMessage({
        type: "sessions",
        sessions: [{ session: "gbt-1", phase: "running", rows: 24, cols: 80 }],
        terminals: [],
      });
    });
    await act(async () => {
      ws.emitMessage({
        type: "input_result",
        ok: false,
        id: "r1",
        session: "gbt-1",
        error: "session not found",
      });
    });
    expect(latest.sessions[0].session).toBe("gbt-1");
    expect(notice?.text).toContain("session not found");
    expect(notice?.text).toContain("终端输入失败");
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
