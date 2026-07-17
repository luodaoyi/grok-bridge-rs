import { afterEach, describe, expect, it, vi } from "vitest";
import {
  getSessions,
  getVersionStatus,
  normalizeSessions,
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
