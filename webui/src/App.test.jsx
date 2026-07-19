import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockXTerm } from "./test/mockXterm.js";
import { installMockWebSocket, MockWebSocket } from "./test/mockWebSocket.js";
import { resetTerminalFeeds } from "./utils/terminalFeeds.js";

vi.mock("@xterm/xterm", () => ({
  Terminal: MockXTerm,
}));

import App from "./App.jsx";

const sessions = [
  {
    session: "gbt-a",
    owner: "Codex A/中文",
    client_session_id: "codex-a",
    client_state: "connected",
    phase: "running",
    title: "实现 TodoList",
    cwd: "C:\\work\\todo-a",
    process_id: 101,
    updated_at_ms: Date.now(),
    activity: "working",
    hook_event: "pre_tool_use",
    hook_at_ms: Date.now(),
    tool_name: "edit",
    waiting_reason: null,
    rows: 24,
    cols: 80,
    screen: "正在修改 app.js",
  },
  {
    session: "gbt-unowned",
    owner: null,
    client_session_id: null,
    client_state: "unmanaged",
    phase: "idle",
    title: null,
    cwd: "C:\\work\\other",
    process_id: 102,
    updated_at_ms: Date.now(),
    activity: "done",
    hook_event: null,
    hook_at_ms: null,
    tool_name: null,
    waiting_reason: null,
    rows: 24,
    cols: 80,
    screen: "done",
  },
];

function utf8ToBase64(text) {
  const bytes = new TextEncoder().encode(String(text ?? ""));
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function jsonResponse(value) {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockFetch(handlers = {}) {
  return vi.fn(async (url) => {
    const path = String(url);
    if (path.includes("/api/version")) {
      return jsonResponse(
        handlers.version ?? {
          current: "0.6.1",
          latest: null,
          update_available: false,
          release_url:
            "https://github.com/luodaoyi/grok-bridge-rs/releases/latest",
        },
      );
    }
    if (path.includes("/close")) {
      if (handlers.close instanceof Response) return handlers.close;
      if (typeof handlers.close === "function") return handlers.close(path);
      if (path.includes("/api/sessions/")) {
        return new Response("", { status: 200 });
      }
      return jsonResponse(
        handlers.closeResult ?? { matched: 1, closed: 1, failures: [] },
      );
    }
    if (path.includes("/api/sessions")) {
      throw new Error("GET /api/sessions must not be used by WebUI stream");
    }
    return jsonResponse({});
  });
}

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

function pushSessions(ws, nextSessions, terminals = []) {
  ws.emitMessage({
    type: "sessions",
    sessions: nextSessions,
    terminals,
  });
}

describe("App", () => {
  let container;
  let root;

  beforeEach(() => {
    vi.useFakeTimers();
    MockXTerm.reset();
    resetTerminalFeeds();
    installMockWebSocket();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.spyOn(window, "matchMedia").mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    window.localStorage.clear();
    vi.stubGlobal("fetch", mockFetch());
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    resetTerminalFeeds();
    MockXTerm.reset();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  async function renderAppAndConnect(messageSessions = sessions, terminals) {
    await act(async () => root.render(<App />));
    await settle();
    const ws = MockWebSocket.instances[0];
    await act(async () => ws.open());
    const defaultTerminals =
      terminals ??
      messageSessions.map((session) => ({
        session: session.session,
        reset: true,
        cursor: 0,
        next_cursor: 4,
        data_base64: utf8ToBase64(session.screen || session.session),
      }));
    await act(async () => pushSessions(ws, messageSessions, defaultTerminals));
    await settle();
    return ws;
  }

  it("renders supervisor/subagent groups, stats, terminal shells and no unowned batch close", async () => {
    await renderAppAndConnect();
    expect(container.textContent).toContain("Codex A/中文");
    expect(container.textContent).toContain("未标记的 Codex 对话");
    expect(container.textContent).toContain("监督者");
    expect(container.textContent).toContain("子代理");
    expect(container.textContent).toContain("GitHub");
    expect(container.textContent).toContain("Runtime v0.6.1");
    expect(container.textContent).toContain("实时通道已连接");
    expect(container.textContent).toContain("WebSocket");
    expect(
      container.querySelector('a[href="https://github.com/luodaoyi/grok-bridge-rs"]'),
    ).not.toBeNull();
    expect(container.querySelectorAll("details.group")).toHaveLength(2);
    expect(container.querySelectorAll("details.session")).toHaveLength(2);
    expect(container.querySelectorAll("[data-terminal]")).toHaveLength(2);
    expect(container.querySelector('[href="#session-board"]')).not.toBeNull();
    expect(container.querySelectorAll("button")).toSatisfy((buttons) =>
      [...buttons].some((button) =>
        button.textContent.includes("关闭该 Codex 全部 Grok"),
      ),
    );
    expect(
      [...container.querySelectorAll("details.group")]
        .find((group) => group.dataset.ownerKey === "missing-owner")
        .textContent,
    ).not.toContain("关闭该 Codex 全部 Grok");
    expect(MockXTerm.instances.every((term) => term.options.disableStdin)).toBe(
      true,
    );
  });

  it("keeps xterm instances mounted across collapse so continuous output is preserved", async () => {
    await renderAppAndConnect();
    expect(MockXTerm.instances).toHaveLength(2);
    const before = MockXTerm.instances.slice();

    const button = (text) =>
      [...container.querySelectorAll("button")].find((item) =>
        item.textContent.includes(text),
      );
    await act(async () => button("全部折叠").click());
    expect(
      [...container.querySelectorAll("details.group")].every(
        (group) => !group.open,
      ),
    ).toBe(true);
    // Terminal hosts remain mounted while sessions exist.
    expect(container.querySelectorAll("[data-terminal]")).toHaveLength(2);
    expect(MockXTerm.instances).toHaveLength(2);
    expect(MockXTerm.instances[0]).toBe(before[0]);
    expect(MockXTerm.instances[1]).toBe(before[1]);
    expect(before.every((term) => !term.disposed)).toBe(true);

    await act(async () => button("全部展开").click());
    expect(
      [...container.querySelectorAll("details.session")].every(
        (session) => session.open,
      ),
    ).toBe(true);
  });

  it("can collapse a single Grok session without collapsing its Codex group", async () => {
    await renderAppAndConnect();
    const session = container.querySelector(
      'details.session[data-session="gbt-a"]',
    );
    expect(session.open).toBe(true);
    await act(async () => {
      session.open = false;
      session.dispatchEvent(new Event("toggle", { bubbles: true }));
    });
    await settle();
    expect(
      container.querySelector('details.session[data-session="gbt-a"]').open,
    ).toBe(false);
    expect(
      container.querySelector('details.group[data-owner-key="client:codex-a"]')
        .open,
    ).toBe(true);
    expect(container.querySelectorAll("[data-terminal]")).toHaveLength(2);
  });

  it("shows update banner with release link and allows dismiss", async () => {
    vi.stubGlobal(
      "fetch",
      mockFetch({
        version: {
          current: "0.6.1",
          latest: "0.6.2",
          update_available: true,
          release_url:
            "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.6.2",
        },
      }),
    );
    await renderAppAndConnect();
    expect(container.textContent).toContain("发现新版本 v0.6.2");
    expect(
      container.querySelector(
        'a[href="https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.6.2"]',
      ),
    ).not.toBeNull();
    const dismiss = [...container.querySelectorAll("button")].find((button) =>
      button.textContent.includes("稍后提醒"),
    );
    await act(async () => dismiss.click());
    await settle();
    expect(container.querySelector("[data-update-banner]")).toBeNull();
    expect(window.localStorage.getItem("grok-bridge-dismissed-update")).toBe(
      "0.6.2",
    );
  });

  it("posts session and owner close without forcing GET /api/sessions", async () => {
    const ws = await renderAppAndConnect();
    const sessionCallsBefore = fetch.mock.calls.filter((call) =>
      String(call[0]).includes("/api/sessions"),
    ).length;

    const ownedGroup = [...container.querySelectorAll("details.group")].find(
      (group) => group.dataset.ownerKey === "client:codex-a",
    );
    const sessionClose = [...ownedGroup.querySelectorAll("button")].find(
      (button) => button.textContent.trim() === "关闭 Grok",
    );
    await act(async () => sessionClose.click());
    await settle();
    expect(fetch).toHaveBeenCalledWith(
      "/api/sessions/gbt-a/close",
      expect.objectContaining({
        method: "POST",
        headers: { "X-Grok-Bridge-WebUI": "1" },
      }),
    );

    const ownerClose = [...container.querySelectorAll("button")].find(
      (button) => button.textContent.includes("关闭该 Codex 全部 Grok"),
    );
    await act(async () => ownerClose.click());
    await settle();
    expect(fetch).toHaveBeenCalledWith(
      "/api/clients/codex-a/close",
      expect.objectContaining({
        method: "POST",
        headers: { "X-Grok-Bridge-WebUI": "1" },
      }),
    );
    expect(container.textContent).toContain(
      "已关闭 Codex“Codex A/中文”下的全部 1 个 Grok 会话",
    );

    const sessionGets = fetch.mock.calls.filter(
      (call) =>
        String(call[0]).includes("/api/sessions") &&
        !String(call[0]).includes("/close") &&
        (!call[1] || call[1].method == null || call[1].method === "GET"),
    );
    expect(sessionGets).toHaveLength(sessionCallsBefore);

    // State updates come from the pushed WebSocket stream.
    await act(async () => {
      pushSessions(ws, [sessions[1]], [
        {
          session: "gbt-unowned",
          reset: true,
          data_base64: utf8ToBase64("done"),
        },
      ]);
    });
    await settle();
    expect(container.textContent).not.toContain("实现 TodoList");
  });

  it("disposes xterm when a session is removed from the stream", async () => {
    const ws = await renderAppAndConnect();
    expect(MockXTerm.instances).toHaveLength(2);
    const hosts = [...container.querySelectorAll("[data-terminal]")];
    const removedIndex = hosts.findIndex(
      (host) => host.dataset.terminal === "gbt-a",
    );
    const removed = MockXTerm.instances[removedIndex];

    await act(async () => {
      pushSessions(ws, [sessions[1]], [
        {
          session: "gbt-unowned",
          reset: false,
          data_base64: utf8ToBase64("+"),
        },
      ]);
    });
    await settle();
    expect(container.querySelectorAll("[data-terminal]")).toHaveLength(1);
    expect(
      MockXTerm.instances.filter((term) => !term.disposed),
    ).toHaveLength(1);
    if (removed) expect(removed.disposed).toBe(true);
  });

  it("never issues two-second session polling fetches", async () => {
    await renderAppAndConnect();
    const before = fetch.mock.calls.length;
    await act(async () => vi.advanceTimersByTimeAsync(6000));
    await settle();
    const sessionPolls = fetch.mock.calls.slice(before).filter((call) => {
      const url = String(call[0]);
      return url.includes("/api/sessions") && !url.includes("/close");
    });
    expect(sessionPolls).toHaveLength(0);
  });

  it("shows reconnecting status after socket close and recovers on open", async () => {
    const ws = await renderAppAndConnect();
    await act(async () => ws.close());
    await settle();
    expect(container.textContent).toMatch(/重连|断开/);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    const next = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    await act(async () => next.open());
    await act(async () => {
      pushSessions(next, sessions, [
        {
          session: "gbt-a",
          reset: true,
          data_base64: utf8ToBase64("again"),
        },
        {
          session: "gbt-unowned",
          reset: true,
          data_base64: utf8ToBase64("done"),
        },
      ]);
    });
    await settle();
    expect(container.textContent).toContain("实时通道已连接");
  });

  it("surfaces disconnected and cleanup lifecycle on supervisor cards", async () => {
    await renderAppAndConnect([
      {
        ...sessions[0],
        client_state: "orphaned",
        auto_close_at_ms: Date.now() + 60_000,
      },
    ]);
    expect(container.textContent).toContain("清理倒计时");
    expect(container.textContent).toContain("自动清理倒计时");
  });

  it("applies ordered terminal appends after the initial snapshot", async () => {
    const ws = await renderAppAndConnect([sessions[0]], [
      {
        session: "gbt-a",
        reset: true,
        data_base64: utf8ToBase64("SNAP"),
      },
    ]);
    const term = MockXTerm.instances[0];
    expect(term.resetCount).toBeGreaterThanOrEqual(1);

    await act(async () => {
      pushSessions(ws, [sessions[0]], [
        {
          session: "gbt-a",
          reset: false,
          data_base64: utf8ToBase64("1"),
        },
        {
          session: "gbt-a",
          reset: false,
          data_base64: utf8ToBase64("2"),
        },
      ]);
    });
    await settle();
    const decoded = term.written.map((chunk) =>
      typeof chunk === "string"
        ? chunk
        : new TextDecoder().decode(chunk),
    );
    expect(decoded.join("")).toContain("1");
    expect(decoded.join("")).toContain("2");
  });
});
