import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n/index.js";
import { MockXTerm } from "../test/mockXterm.js";
import { SubagentCard } from "./SubagentCard.jsx";
import { SessionLifecycleHint } from "./SessionLifecycleHint.jsx";

vi.mock("@xterm/xterm", () => ({
  Terminal: MockXTerm,
}));

vi.mock("@xterm/addon-fit", () => {
  class MockFitAddon {
    activate() {}
    fit() {}
    dispose() {}
  }
  return { FitAddon: MockFitAddon };
});

function baseSession(overrides = {}) {
  return {
    session: "gbt-test",
    owner: "Codex Test",
    client_session_id: "thread-1",
    client_state: "connected",
    phase: "idle",
    title: "Task",
    cwd: "C:\\work",
    process_id: 1,
    updated_at_ms: Date.now(),
    activity: "done",
    hook_event: null,
    tool_name: null,
    waiting_reason: null,
    rows: 24,
    cols: 80,
    ...overrides,
  };
}

describe("SessionLifecycleHint", () => {
  let container;
  let root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    vi.useFakeTimers({ shouldAdvanceTime: false });
    vi.setSystemTime(new Date("2026-07-19T12:00:00.000Z"));
  });

  afterEach(async () => {
    await act(async () => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  async function renderHint(session) {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale="en">
          <SessionLifecycleHint session={session} />
        </I18nProvider>,
      );
    });
  }

  async function renderCard(session, collapsed = false) {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale="en">
          <SubagentCard
            session={session}
            collapsed={collapsed}
            onToggle={() => {}}
            onClose={() => {}}
            busy={false}
          />
        </I18nProvider>,
      );
    });
  }

  it("shows keep-alive for connected managed sessions with policy durations", async () => {
    await renderHint(
      baseSession({
        client_state: "connected",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      }),
    );
    const el = container.querySelector('[data-lifecycle-hint="connected"]');
    expect(el).not.toBeNull();
    expect(el.getAttribute("data-density")).toBe("compact");
    expect(el.querySelectorAll("p")).toHaveLength(0);
    expect(el.textContent).toMatch(/Keep-alive active/i);
    expect(el.textContent).toMatch(/will not auto-close/i);
    expect(el.textContent).toMatch(/lease/i);
    expect(el.textContent).toMatch(/grace/i);
    // Should surface real wire durations, not invent other numbers.
    expect(el.textContent).toMatch(/2/);
    expect(el.textContent).toMatch(/10/);
  });

  it("does not invent lease/grace when wire fields are missing", async () => {
    await renderHint(baseSession({ client_state: "connected" }));
    const el = container.querySelector('[data-lifecycle-hint="connected"]');
    expect(el).not.toBeNull();
    expect(el.getAttribute("data-density")).toBe("compact");
    expect(el.textContent).toMatch(/Keep-alive active/i);
    expect(el.textContent).not.toMatch(/After disconnect/i);
    expect(el.textContent).not.toMatch(/120/);
    expect(el.textContent).not.toMatch(/600/);
  });

  it("explains disconnected policy without a fixed close deadline", async () => {
    await renderHint(
      baseSession({
        client_state: "disconnected",
        phase: "running",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      }),
    );
    const el = container.querySelector('[data-lifecycle-hint="disconnected"]');
    expect(el).not.toBeNull();
    expect(el.getAttribute("data-density")).toBe("compact");
    expect(el.querySelectorAll("p")).toHaveLength(0);
    expect(el.textContent).toMatch(/not closing yet|offline/i);
    expect(el.textContent).toMatch(/Running or Waiting/i);
    expect(el.textContent).toMatch(/Idle/i);
    expect(el.querySelector("[data-lifecycle-countdown]")).toBeNull();
  });

  it("ticks orphaned countdown on the parent-owned local clock each second", async () => {
    const deadline = Date.now() + 90_000;
    await renderCard(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: deadline,
      }),
      false,
    );
    const el = container.querySelector('[data-lifecycle-hint="orphaned"]');
    expect(el).not.toBeNull();
    expect(el.getAttribute("data-density")).toBe("prominent");
    expect(el.getAttribute("role")).toBe("alert");
    const first = container.querySelector(
      "[data-lifecycle-countdown]",
    ).textContent;
    expect(first).toMatch(/1\s*minute|90\s*second/i);

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    const second = container.querySelector(
      "[data-lifecycle-countdown]",
    ).textContent;
    // After one second the remaining value must change.
    expect(second).not.toBe(first);
    expect(el.textContent).toMatch(
      /Local cleanup eligibility deadline|cleans up shortly after/i,
    );
    expect(el.getAttribute("data-cleanup-due")).toBe("false");
  });

  it("shows waiting for Runtime cleanup after eligibility deadline, not closed", async () => {
    const deadline = Date.now() - 1_000;
    await renderCard(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: deadline,
      }),
      false,
    );
    const el = container.querySelector('[data-lifecycle-hint="orphaned"]');
    expect(el).not.toBeNull();
    expect(el.getAttribute("data-density")).toBe("prominent");
    expect(el.getAttribute("data-cleanup-due")).toBe("true");
    const countdown = container.querySelector(
      "[data-lifecycle-countdown]",
    ).textContent;
    expect(countdown).toMatch(/waiting for Runtime cleanup/i);
    expect(countdown).not.toMatch(/closes in|closed|0 second/i);
    expect(el.textContent).toMatch(
      /Local cleanup eligibility deadline|cleans up shortly after/i,
    );
  });

  it("shows closing state clearly", async () => {
    await renderHint(baseSession({ client_state: "closing" }));
    const el = container.querySelector('[data-lifecycle-hint="closing"]');
    expect(el).not.toBeNull();
    expect(el.getAttribute("data-density")).toBe("prominent");
    expect(el.textContent).toMatch(/Closing session/i);
    expect(el.textContent).toMatch(/being closed/i);
  });

  it("uses compact density for normal states and prominent density for cleanup risk", async () => {
    await renderHint(
      baseSession({
        client_state: "connected",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      }),
    );
    const connected = container.querySelector(
      '[data-lifecycle-hint="connected"]',
    );
    expect(connected.getAttribute("data-density")).toBe("compact");
    expect(connected.getAttribute("role")).toBe("status");
    // Compact tips stay single-block (no multi-paragraph card structure).
    expect(connected.querySelectorAll("p")).toHaveLength(0);

    await renderHint(baseSession({ client_state: "disconnected" }));
    const disconnected = container.querySelector(
      '[data-lifecycle-hint="disconnected"]',
    );
    expect(disconnected.getAttribute("data-density")).toBe("compact");
    expect(disconnected.getAttribute("role")).toBe("status");
    expect(disconnected.querySelectorAll("p")).toHaveLength(0);

    await renderHint(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: Date.now() + 30_000,
      }),
    );
    const orphaned = container.querySelector('[data-lifecycle-hint="orphaned"]');
    expect(orphaned.getAttribute("data-density")).toBe("prominent");
    expect(orphaned.getAttribute("role")).toBe("alert");
    expect(orphaned.querySelectorAll("p").length).toBeGreaterThan(0);

    await renderHint(baseSession({ client_state: "closing" }));
    const closing = container.querySelector('[data-lifecycle-hint="closing"]');
    expect(closing.getAttribute("data-density")).toBe("prominent");
    expect(closing.querySelectorAll("p").length).toBeGreaterThan(0);
  });

  it("renders nothing for unmanaged sessions", async () => {
    await renderHint(baseSession({ client_state: "unmanaged" }));
    expect(container.querySelector("[data-lifecycle-hint]")).toBeNull();
    expect(container.textContent).not.toMatch(/auto-close|countdown|lease/i);
  });

  it("surfaces orphaned risk in the collapsed summary", async () => {
    await renderCard(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: Date.now() + 45_000,
      }),
      true,
    );
    const collapsed = container.querySelector(
      "[data-lifecycle-collapsed='orphaned']",
    );
    expect(collapsed).not.toBeNull();
    expect(collapsed.textContent).toMatch(/Cleanup eligible in/i);
    // Banner stays mounted under collapsed details for xterm survival.
    expect(
      container.querySelector('[data-lifecycle-hint="orphaned"]'),
    ).not.toBeNull();
  });

  it("surfaces waiting-for-Runtime risk in the collapsed summary when due", async () => {
    await renderCard(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: Date.now() - 500,
      }),
      true,
    );
    const collapsed = container.querySelector(
      "[data-lifecycle-collapsed='orphaned']",
    );
    expect(collapsed).not.toBeNull();
    expect(collapsed.textContent).toMatch(/Waiting for Runtime cleanup/i);
    expect(collapsed.textContent).not.toMatch(/closes|closed|0 second/i);
  });

  it("surfaces closing risk in the collapsed summary", async () => {
    await renderCard(baseSession({ client_state: "closing" }), true);
    const collapsed = container.querySelector(
      "[data-lifecycle-collapsed='closing']",
    );
    expect(collapsed).not.toBeNull();
    expect(collapsed.textContent).toMatch(/Closing now/i);
  });

  it("creates at most one interval for collapsed orphaned card (summary + banner share clock)", async () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const session = baseSession({
      client_state: "orphaned",
      auto_close_at_ms: Date.now() + 60_000,
    });
    await renderCard(session, true);
    const oneSecondIntervals = () =>
      setIntervalSpy.mock.calls.filter((call) => call[1] === 1000);
    expect(oneSecondIntervals()).toHaveLength(1);
    expect(
      container.querySelector("[data-lifecycle-collapsed='orphaned']"),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-lifecycle-hint="orphaned"]'),
    ).not.toBeNull();

    // Expand: needsClock stays true, so parent must not open a second interval.
    await renderCard(session, false);
    expect(oneSecondIntervals()).toHaveLength(1);
    setIntervalSpy.mockRestore();
  });

  it("does not start an interval for connected or unmanaged cards", async () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    await renderCard(
      baseSession({
        client_state: "connected",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      }),
      false,
    );
    expect(
      setIntervalSpy.mock.calls.filter((call) => call[1] === 1000),
    ).toHaveLength(0);
    await renderCard(baseSession({ client_state: "unmanaged" }), false);
    expect(
      setIntervalSpy.mock.calls.filter((call) => call[1] === 1000),
    ).toHaveLength(0);
    setIntervalSpy.mockRestore();
  });

  it("clears the local countdown timer on unmount", async () => {
    const clearSpy = vi.spyOn(window, "clearInterval");
    await renderCard(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: Date.now() + 30_000,
      }),
      true,
    );
    await act(async () => {
      root.unmount();
    });
    expect(clearSpy).toHaveBeenCalled();
    clearSpy.mockRestore();
    // Recreate root for afterEach unmount safety.
    root = createRoot(container);
  });

  it("ticks collapsed summary and expanded banner from the same parent clock", async () => {
    const deadline = Date.now() + 45_000;
    await renderCard(
      baseSession({
        client_state: "orphaned",
        auto_close_at_ms: deadline,
      }),
      true,
    );
    const summaryBefore = container.querySelector(
      "[data-lifecycle-collapsed='orphaned']",
    ).textContent;
    const bannerBefore = container.querySelector(
      "[data-lifecycle-countdown]",
    ).textContent;

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    const summaryAfter = container.querySelector(
      "[data-lifecycle-collapsed='orphaned']",
    ).textContent;
    const bannerAfter = container.querySelector(
      "[data-lifecycle-countdown]",
    ).textContent;
    expect(summaryAfter).not.toBe(summaryBefore);
    expect(bannerAfter).not.toBe(bannerBefore);
  });
});
