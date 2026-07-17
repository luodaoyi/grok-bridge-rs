import { afterEach, describe, expect, it, vi } from "vitest";
import { getSessions, normalizeSessions } from "./api.js";

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
