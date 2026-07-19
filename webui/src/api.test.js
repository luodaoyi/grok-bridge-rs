import { afterEach, describe, expect, it, vi } from "vitest";
import {
  eventsWebSocketUrl,
  getSessions,
  getVersionStatus,
  normalizeEventsMessage,
  normalizeSessions,
  normalizeTerminalEntries,
  normalizeVersionStatus,
} from "./api.js";

function jsonResponse(value, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

describe("normalizeSessions", () => {
  it("rejects non-array payloads", () => {
    expect(() => normalizeSessions({ sessions: [] })).toThrow(
      /not an array/i,
    );
  });

  it("drops invalid entries and keeps valid sessions", () => {
    expect(
      normalizeSessions([
        { session: "ok-1", phase: "running" },
        null,
        { session: "" },
        { phase: "idle" },
        { session: "ok-2" },
      ]),
    ).toEqual([
      { session: "ok-1", phase: "running" },
      { session: "ok-2" },
    ]);
  });
});

describe("getSessions", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns normalized sessions", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse([{ session: "a" }, { session: "" }, null]),
      ),
    );
    await expect(getSessions()).resolves.toEqual([{ session: "a" }]);
  });

  it("throws on invalid JSON without leaving hang state", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response("not-json", {
          status: 200,
          headers: { "Content-Type": "text/plain" },
        }),
      ),
    );
    await expect(getSessions()).rejects.toThrow(/invalid JSON/i);
  });

  it("throws on non-array JSON", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(jsonResponse({ error: "boom" })),
    );
    await expect(getSessions()).rejects.toThrow(/not an array/i);
  });
});

describe("normalizeVersionStatus", () => {
  it("normalizes update payloads", () => {
    expect(
      normalizeVersionStatus({
        current: "0.6.1",
        latest: "0.6.2",
        update_available: true,
        release_url:
          "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.6.2",
        checked_at_ms: 42,
      }),
    ).toEqual({
      current: "0.6.1",
      latest: "0.6.2",
      update_available: true,
      release_url:
        "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.6.2",
      checked_at_ms: 42,
    });
  });

  it("rejects invalid payloads", () => {
    expect(() => normalizeVersionStatus([])).toThrow(/not an object/i);
    expect(() => normalizeVersionStatus({})).toThrow(/missing current/i);
  });
});

describe("getVersionStatus", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns normalized version status", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({
          current: "0.6.1",
          latest: "0.6.2",
          update_available: true,
          release_url:
            "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.6.2",
        }),
      ),
    );
    await expect(getVersionStatus()).resolves.toMatchObject({
      current: "0.6.1",
      latest: "0.6.2",
      update_available: true,
    });
  });
});

describe("events stream helpers", () => {
  it("builds same-origin ws/wss events URL", () => {
    expect(
      eventsWebSocketUrl({ protocol: "http:", host: "127.0.0.1:47653" }),
    ).toBe("ws://127.0.0.1:47653/api/events");
    expect(
      eventsWebSocketUrl({ protocol: "https:", host: "localhost:8443" }),
    ).toBe("wss://localhost:8443/api/events");
  });

  it("normalizes sessions event frames and terminal entries", () => {
    const message = normalizeEventsMessage({
      type: "sessions",
      sessions: [{ session: "a" }, { session: "" }, null],
      terminals: [
        {
          session: "a",
          reset: true,
          cursor: 0,
          next_cursor: 3,
          data_base64: "YWI=",
        },
        { session: "", data_base64: "x" },
        null,
      ],
    });
    expect(message).toEqual({
      type: "sessions",
      sessions: [{ session: "a" }],
      terminals: [
        {
          session: "a",
          reset: true,
          cursor: 0,
          next_cursor: 3,
          data_base64: "YWI=",
        },
      ],
    });
  });

  it("rejects invalid event frames", () => {
    expect(() => normalizeEventsMessage(null)).toThrow(/not an object/i);
    expect(() => normalizeEventsMessage({ type: "other" })).toThrow(
      /unsupported events type/i,
    );
    expect(() => normalizeTerminalEntries({})).toThrow(/not an array/i);
  });
});
