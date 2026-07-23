import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockXTerm } from "./test/mockXterm.js";
import { installMockWebSocket, MockWebSocket } from "./test/mockWebSocket.js";
import { resetTerminalFeeds } from "./utils/terminalFeeds.js";

vi.mock("@xterm/xterm", () => ({
  Terminal: MockXTerm,
}));

vi.mock("@xterm/addon-fit", () => {
  class MockFitAddon {
    static instances = [];
    constructor() {
      this.fitCount = 0;
      this.disposed = false;
      MockFitAddon.instances.push(this);
    }
    activate() {}
    fit() {
      this.fitCount += 1;
    }
    dispose() {
      this.disposed = true;
    }
  }
  return { FitAddon: MockFitAddon };
});

import App from "./App.jsx";
import { SUPPORTED_LOCALES } from "./i18n/index.js";

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
    vi.stubGlobal(
      "requestAnimationFrame",
      (cb) => {
        cb(0);
        return 1;
      },
    );
    vi.stubGlobal("cancelAnimationFrame", () => {});
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
    window.localStorage.clear();
    // Keep existing Chinese UI assertions stable regardless of host language.
    window.localStorage.setItem("grok-bridge-locale", "zh-CN");
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
    expect(container.textContent).toContain("未标记的 Codex 会话");
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
    expect(container.querySelector(".page.app-shell")).not.toBeNull();
    expect(container.querySelector("header.navbar")).not.toBeNull();
    expect(container.querySelectorAll(".stats-grid > .card")).toHaveLength(5);
    expect(container.querySelector("details.card.supervisor-card")).not.toBeNull();
    expect(container.querySelector(".nav.nav-tabs")).not.toBeNull();
    expect(container.querySelector('[href="#session-board"]')).not.toBeNull();
    expect(container.querySelectorAll("button")).toSatisfy((buttons) =>
      [...buttons].some((button) =>
        button.textContent.includes("关闭该 Codex 下的全部 Grok 会话"),
      ),
    );
    expect(
      [...container.querySelectorAll("details.group")]
        .find((group) => group.dataset.ownerKey === "missing-owner")
        .textContent,
    ).not.toContain("关闭该 Codex 下的全部 Grok 会话");
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
      (button) =>
        button.textContent.includes("关闭该 Codex 下的全部 Grok 会话"),
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
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      },
    ]);
    expect(container.textContent).toContain("自动关闭倒计时");
    expect(container.querySelector('[data-lifecycle-hint="orphaned"]')).not.toBeNull();
  });

  it("hides connected keep-alive/lease/grace banners from the WebUI", async () => {
    await renderAppAndConnect([
      {
        ...sessions[0],
        client_state: "connected",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      },
    ]);
    expect(container.querySelector("[data-lifecycle-hint]")).toBeNull();
    expect(container.textContent).not.toMatch(
      /正在保活|不会自动关闭|断连后|续租|宽限/,
    );
  });

  it("does not invent timeout messaging for unmanaged sessions", async () => {
    await renderAppAndConnect([sessions[1]]);
    expect(container.querySelector("[data-lifecycle-hint]")).toBeNull();
    expect(container.textContent).not.toMatch(/自动关闭|正在保活|宽限/);
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

  it("defaults interactive mode off and does not persist the switch", async () => {
    await renderAppAndConnect();
    const toggle = container.querySelector("[data-interactive-toggle]");
    expect(toggle).not.toBeNull();
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(toggle.dataset.interactive).toBe("off");
    expect(toggle.getAttribute("title")).toBe(
      "开启所有 Grok 终端的键盘输入",
    );
    expect(
      [...container.querySelectorAll("[data-readonly]")].every(
        (node) => node.dataset.readonly === "true",
      ),
    ).toBe(true);
    expect(MockXTerm.instances.every((term) => term.options.disableStdin)).toBe(
      true,
    );
    expect(window.localStorage.getItem("grok-bridge-interactive")).toBeNull();

    await act(async () => toggle.click());
    await settle();
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    expect(toggle.getAttribute("title")).toBe(
      "开启或关闭所有终端的键盘输入",
    );
    const warning = container.querySelector("[data-interactive-warning]");
    expect(warning).not.toBeNull();
    expect(warning.textContent).toContain("交互模式已开启");
    expect(MockXTerm.instances.every((term) => !term.options.disableStdin)).toBe(
      true,
    );
    expect(window.localStorage.getItem("grok-bridge-interactive")).toBeNull();
  });

  it("sends terminal_input over the events WebSocket only while interactive", async () => {
    const ws = await renderAppAndConnect([sessions[0]]);
    const toggle = container.querySelector("[data-interactive-toggle]");
    await act(async () => toggle.click());
    await settle();
    const term = MockXTerm.instances[0];
    await act(async () => term.emitData("hello"));
    await settle();
    const sent = ws.sent.map((item) => JSON.parse(String(item)));
    expect(sent.some((msg) => msg.type === "terminal_input")).toBe(true);
    const input = sent.find((msg) => msg.type === "terminal_input");
    expect(input.session).toBe("gbt-a");
    expect(input.data_base64).toBe(btoa("hello"));
    expect(input.id).toBeTruthy();

    await act(async () => toggle.click());
    await settle();
    const before = ws.sent.length;
    await act(async () => term.emitData("nope"));
    await settle();
    const after = ws.sent.map((item) => JSON.parse(String(item)));
    expect(after.filter((msg) => msg.type === "terminal_input").length).toBe(
      sent.filter((msg) => msg.type === "terminal_input").length,
    );
    expect(ws.sent.length).toBeGreaterThanOrEqual(before);
  });

  it("does not buffer keystrokes across disconnect", async () => {
    const ws = await renderAppAndConnect([sessions[0]]);
    const toggle = container.querySelector("[data-interactive-toggle]");
    await act(async () => toggle.click());
    await settle();
    await act(async () => ws.close());
    await settle();
    const term = MockXTerm.instances[0];
    const before = ws.sent.length;
    await act(async () => term.emitData("buffered?"));
    await settle();
    expect(ws.sent.length).toBe(before);
    expect(container.textContent).toMatch(/断开|断|disconnect|离线|offline|不可|fail|失败/i);
  });

  it("exposes a language switcher that localizes chrome without touching terminal bytes", async () => {
    const waitingSession = {
      ...sessions[0],
      session: "gbt-wait",
      activity: "waiting",
      waiting_reason: "ask_user:confirm_path",
      title: "WAIT_TITLE_RAW",
      hook_event: "pre_tool_use",
      tool_name: "shell_exec",
      screen: "RAW_TERMINAL_BYTES_αβ",
    };
    await renderAppAndConnect([waitingSession]);
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(document.title).toContain("Grok Bridge");
    const switcher = container.querySelector("[data-language-switcher]");
    expect(switcher).not.toBeNull();
    expect(switcher.querySelector("select")).toBeNull();

    const terminalTextBefore = MockXTerm.instances[0].written.slice();
    const trigger = switcher.querySelector("[data-language-trigger]");
    expect(trigger).not.toBeNull();

    await act(async () => trigger.click());
    await settle();
    expect(switcher.querySelectorAll('[role="option"]')).toHaveLength(
      SUPPORTED_LOCALES.length,
    );
    await act(async () => {
      switcher
        .querySelector("[data-language-menu]")
        ?.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
        );
    });
    await settle();

    async function pickLocale(code) {
      if (!switcher.querySelector("[data-language-menu]")) {
        await act(async () => trigger.click());
        await settle();
      }
      const option = switcher.querySelector(`[data-language-option="${code}"]`);
      expect(option).not.toBeNull();
      await act(async () => option.click());
      await settle();
    }

    await pickLocale("en");
    expect(document.documentElement.lang).toBe("en");
    expect(window.localStorage.getItem("grok-bridge-locale")).toBe("en");
    expect(container.textContent).toContain("Supervisor");
    expect(container.textContent).toContain("Live channel connected");
    expect(container.textContent).toContain("Close Grok");
    // Raw session fields stay as provided by the Runtime.
    expect(container.textContent).toContain("Codex A/中文");
    expect(container.textContent).toContain("WAIT_TITLE_RAW");
    expect(container.textContent).toContain("pre_tool_use");
    expect(container.textContent).toContain("shell_exec");
    expect(container.textContent).toContain("ask_user:confirm_path");
    expect(container.textContent).toContain("C:\\work\\todo-a");
    expect(MockXTerm.instances[0].written).toEqual(terminalTextBefore);

    await pickLocale("fr");
    expect(document.documentElement.lang).toBe("fr");
    expect(window.localStorage.getItem("grok-bridge-locale")).toBe("fr");
    expect(container.textContent).toMatch(/Canal en direct connecté|sous-agent|Fermer Grok/i);
    expect(container.textContent).toContain("ask_user:confirm_path");
    expect(container.textContent).toContain("WAIT_TITLE_RAW");
    expect(MockXTerm.instances[0].written).toEqual(terminalTextBefore);

    await pickLocale("de");
    expect(document.documentElement.lang).toBe("de");
    expect(container.textContent).toMatch(/Live-Kanal verbunden|Grok schließen/i);
    expect(container.textContent).toContain("ask_user:confirm_path");
    expect(MockXTerm.instances[0].written).toEqual(terminalTextBefore);
  });
});
