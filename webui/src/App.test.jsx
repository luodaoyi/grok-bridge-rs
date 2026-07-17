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
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(jsonResponse(sessions)));
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

  it("pauses polling while a close request is pending", async () => {
    let resolveClose;
    const pendingClose = new Promise((resolve) => {
      resolveClose = resolve;
    });
    fetch
      .mockResolvedValueOnce(jsonResponse(sessions))
      .mockReturnValueOnce(pendingClose)
      .mockResolvedValue(jsonResponse(sessions));
    await renderApp();
    const initialCalls = fetch.mock.calls.length;

    const sessionClose = [...container.querySelectorAll("button")].find(
      (button) => button.textContent.trim() === "关闭 Grok",
    );
    await act(async () => sessionClose.click());
    expect(fetch).toHaveBeenCalledTimes(initialCalls + 1);

    await act(async () => vi.advanceTimersByTimeAsync(4000));
    expect(fetch).toHaveBeenCalledTimes(initialCalls + 1);

    await act(async () => resolveClose(new Response("", { status: 200 })));
    await settle();
    expect(fetch.mock.calls.length).toBeGreaterThan(initialCalls + 1);
  });

  it("posts session and owner close requests with the bridge header", async () => {
    fetch
      .mockResolvedValueOnce(jsonResponse(sessions))
      .mockResolvedValueOnce(new Response("", { status: 200 }))
      .mockResolvedValueOnce(jsonResponse(sessions))
      .mockResolvedValueOnce(
        jsonResponse({ matched: 1, closed: 1, failures: [] }),
      )
      .mockResolvedValue(jsonResponse(sessions));
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
    fetch
      .mockResolvedValueOnce(jsonResponse(sessions))
      .mockRejectedValueOnce(new Error("network down"))
      .mockResolvedValueOnce(jsonResponse({ broken: true }))
      .mockResolvedValueOnce(jsonResponse(sessions));
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
