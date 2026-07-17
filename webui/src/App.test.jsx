import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
    screen: "done",
  },
];

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
    if (handlers.sessions instanceof Response) return handlers.sessions;
    if (typeof handlers.sessions === "function") return handlers.sessions(path);
    return jsonResponse(handlers.sessions ?? sessions);
  });
}

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("App", () => {
  let container;
  let root;

  beforeEach(() => {
    vi.useFakeTimers();
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
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  async function renderApp() {
    await act(async () => root.render(<App />));
    await settle();
  }

  it("renders owner groups, stats, terminal context and no unowned batch close", async () => {
    await renderApp();
    expect(container.textContent).toContain("Codex A/中文");
    expect(container.textContent).toContain("正在修改 app.js");
    expect(container.textContent).toContain("未标记的 Codex 对话");
    expect(container.textContent).toContain("GitHub");
    expect(container.textContent).toContain("Runtime v0.6.1");
    expect(
      container.querySelector('a[href="https://github.com/luodaoyi/grok-bridge-rs"]'),
    ).not.toBeNull();
    expect(container.querySelectorAll("details.group")).toHaveLength(2);
    expect(container.querySelectorAll("details.session")).toHaveLength(2);
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
  });

  it("collapses and expands all owner groups and session cards", async () => {
    await renderApp();
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
    expect(container.querySelectorAll("pre")).toHaveLength(0);
    await act(async () => button("全部展开").click());
    expect(
      [...container.querySelectorAll("details.group")].every(
        (group) => group.open,
      ),
    ).toBe(true);
    expect(
      [...container.querySelectorAll("details.session")].every(
        (session) => session.open,
      ),
    ).toBe(true);
    expect(container.querySelectorAll("pre")).toHaveLength(2);
  });

  it("can collapse a single Grok session without collapsing its Codex group", async () => {
    await renderApp();
    const session = container.querySelector('details.session[data-session="gbt-a"]');
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
    expect(container.querySelectorAll("pre")).toHaveLength(1);
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
    await renderApp();
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

  it("pauses polling while a close request is pending", async () => {
    let resolveClose;
    const pendingClose = new Promise((resolve) => {
      resolveClose = resolve;
    });
    const baseFetch = mockFetch();
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url, options) => {
        if (String(url).includes("/close")) return pendingClose;
        return baseFetch(url, options);
      }),
    );
    await renderApp();
    const initialCloseCalls = fetch.mock.calls.filter((call) =>
      String(call[0]).includes("/close"),
    ).length;

    const sessionClose = [...container.querySelectorAll("button")].find(
      (button) => button.textContent.trim() === "关闭 Grok",
    );
    await act(async () => sessionClose.click());
    expect(
      fetch.mock.calls.filter((call) => String(call[0]).includes("/close")),
    ).toHaveLength(initialCloseCalls + 1);

    const callsDuringPending = fetch.mock.calls.length;
    await act(async () => vi.advanceTimersByTimeAsync(4000));
    expect(fetch.mock.calls.length).toBe(callsDuringPending);

    await act(async () => resolveClose(new Response("", { status: 200 })));
    await settle();
    expect(fetch.mock.calls.length).toBeGreaterThan(callsDuringPending);
  });

  it("posts session and owner close requests with the bridge header", async () => {
    await renderApp();

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
      "已关闭 Codex“Codex A/中文”下的全部 1 个 Grok 会话。",
    );
  });

  it("keeps polling after request failures and bad payloads", async () => {
    let sessionCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url) => {
        if (String(url).includes("/api/version")) {
          return jsonResponse({
            current: "0.6.1",
            update_available: false,
            release_url:
              "https://github.com/luodaoyi/grok-bridge-rs/releases/latest",
          });
        }
        sessionCalls += 1;
        if (sessionCalls === 1) return jsonResponse(sessions);
        if (sessionCalls === 2) throw new Error("network down");
        if (sessionCalls === 3) return jsonResponse({ broken: true });
        return jsonResponse(sessions);
      }),
    );
    await renderApp();
    expect(container.textContent).toContain("正在修改 app.js");

    await act(async () => vi.advanceTimersByTimeAsync(2000));
    await settle();
    expect(container.textContent).toContain("读取 Runtime 状态失败");
    expect(container.textContent).toContain("将自动重试");
    expect(container.textContent).toContain("正在修改 app.js");

    await act(async () => vi.advanceTimersByTimeAsync(2000));
    await settle();
    expect(container.textContent).toContain("将自动重试");

    await act(async () => vi.advanceTimersByTimeAsync(2000));
    await settle();
    expect(container.textContent).toContain("本机服务已连接");
    expect(container.textContent).toContain("正在修改 app.js");
  });

  it("refreshes sessions every two seconds", async () => {
    await renderApp();
    const initialCalls = fetch.mock.calls.length;
    await act(async () => vi.advanceTimersByTimeAsync(2000));
    await settle();
    expect(fetch.mock.calls.length).toBeGreaterThan(initialCalls);
  });
});
