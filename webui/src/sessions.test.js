import { describe, expect, it } from "vitest";
import { createTranslator, formatCountdown, formatDurationMs } from "./i18n/index.js";
import {
  activityOf,
  activityLabel,
  ageLabel,
  clientLifecycle,
  clientLifecycleLabel,
  clientStateLabel,
  countdownLabel,
  dominantClientState,
  durationMsLabel,
  groupSessions,
  groupSummary,
  lifecycleCollapsedSummary,
  lifecycleHintModel,
  optionalDurationMs,
  ownerKey,
  remainingLabel,
  sessionGroupKey,
  sessionStats,
} from "./sessions.js";

describe("activityOf", () => {
  it("lets terminal PTY phases override stale hook activity", () => {
    for (const phase of ["exited", "failed", "stopped"]) {
      expect(activityOf({ phase, activity: "working" })).toBe("stopped");
      expect(activityOf({ phase, activity: "waiting" })).toBe("stopped");
    }
  });

  it("uses hooks before live phase fallbacks", () => {
    expect(activityOf({ phase: "running", activity: "waiting" })).toBe(
      "waiting",
    );
    expect(activityOf({ phase: "idle", activity: "unknown" })).toBe("done");
    expect(activityOf({ phase: "starting" })).toBe("working");
  });
});

describe("session grouping", () => {
  const sessions = [
    {
      session: "b",
      owner: "Codex B",
      client_session_id: "thread-b",
      phase: "idle",
      activity: "done",
    },
    { session: "missing", owner: null, phase: "running", activity: "working" },
    {
      session: "a",
      owner: "Codex A",
      client_session_id: "thread-a",
      phase: "running",
      activity: "waiting",
    },
    {
      session: "a2",
      owner: "Codex A",
      client_session_id: "thread-a",
      phase: "exited",
      activity: "done",
    },
  ];

  it("sorts owner groups and keeps a distinct missing-owner key", () => {
    const groups = groupSessions(sessions);
    expect(groups.map(([key]) => key)).toEqual([
      "missing-owner",
      "client:thread-a",
      "client:thread-b",
    ]);
    expect(groups[1][1]).toHaveLength(2);
    expect(ownerKey(null)).not.toBe(ownerKey("missing-owner"));
    expect(sessionGroupKey(sessions[2])).toBe("client:thread-a");
  });

  it("computes headline and per-group status counts", () => {
    expect(sessionStats(sessions)).toEqual({
      owners: 3,
      sessions: 4,
      working: 1,
      waiting: 1,
      done: 1,
    });
    const tZh = createTranslator("zh-CN");
    expect(groupSummary(groupSessions(sessions)[1][1], tZh, "zh-CN")).toBe(
      "1 个等待输入",
    );
    const tEn = createTranslator("en");
    expect(groupSummary(groupSessions(sessions)[1][1], tEn, "en")).toBe(
      "1 waiting",
    );
  });
});

describe("localized labels and times", () => {
  it("localizes activity, client, and lifecycle labels", () => {
    const tZh = createTranslator("zh-CN");
    const tEn = createTranslator("en");
    expect(activityLabel("working", tZh)).toBe("工作中");
    expect(activityLabel("working", tEn)).toBe("Working");
    expect(clientStateLabel("connected", tZh)).toBe("Codex 在线");
    expect(clientLifecycleLabel("orphaned", tZh)).toBe("清理倒计时");
    expect(clientLifecycleLabel("orphaned", tEn)).toBe("Cleanup countdown");
  });

  it("formats age and remaining with Intl", () => {
    const now = Date.now();
    expect(ageLabel(now - 5_000, now, "en")).toMatch(/second|sec|秒/i);
    expect(remainingLabel(now + 120_000, now, "en")).toMatch(
      /minute|min|分/i,
    );
  });
});

describe("client lifecycle", () => {
  it("maps lease states into connected/disconnected/cleanup visuals", () => {
    expect(clientLifecycle("connected")).toBe("connected");
    expect(clientLifecycle("disconnected")).toBe("disconnected");
    expect(clientLifecycle("orphaned")).toBe("cleanup");
    expect(clientLifecycle("closing")).toBe("cleanup");
    expect(clientLifecycle("unmanaged")).toBe("unmanaged");
  });

  it("picks the dominant client state for a supervisor group", () => {
    expect(
      dominantClientState([
        { client_state: "connected" },
        { client_state: "disconnected" },
        { client_state: "orphaned" },
      ]),
    ).toBe("orphaned");
    expect(
      dominantClientState([
        { client_state: "connected" },
        { client_state: "unmanaged" },
      ]),
    ).toBe("connected");
  });
});

describe("lifecycle hint model", () => {
  const t = createTranslator("en");

  it("returns none for unmanaged and never fabricates durations", () => {
    expect(lifecycleHintModel({ client_state: "unmanaged" })).toEqual({
      kind: "none",
    });
    expect(optionalDurationMs(undefined)).toBeNull();
    expect(optionalDurationMs(null)).toBeNull();
    expect(optionalDurationMs(-1)).toBeNull();
    expect(optionalDurationMs(120_000)).toBe(120_000);
    expect(durationMsLabel(undefined)).toBeNull();
    expect(formatDurationMs(120_000, "en")).toMatch(/2|minute|min/i);
  });

  it("hides connected keep-alive/lease/grace from the lifecycle model", () => {
    expect(
      lifecycleHintModel({
        client_state: "connected",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      }),
    ).toEqual({ kind: "none" });
    expect(lifecycleHintModel({ client_state: "connected" })).toEqual({
      kind: "none",
    });
  });

  it("models disconnected without lease/grace policy footnotes", () => {
    expect(
      lifecycleHintModel({
        client_state: "disconnected",
        client_lease_ms: 120_000,
        orphan_grace_ms: 600_000,
      }),
    ).toEqual({ kind: "disconnected" });
  });

  it("models orphaned deadline and second-precise countdown", () => {
    const deadline = Date.now() + 90_500;
    expect(
      lifecycleHintModel({
        client_state: "orphaned",
        auto_close_at_ms: deadline,
      }),
    ).toEqual({ kind: "orphaned", deadlineMs: deadline });
    const label = countdownLabel(deadline, Date.now(), "en");
    expect(label).toMatch(/second/i);
    expect(formatCountdown(deadline, Date.now() + 30_000, "en")).toMatch(
      /second|minute/i,
    );
    expect(
      lifecycleCollapsedSummary(
        { client_state: "orphaned", auto_close_at_ms: deadline },
        t,
        "en",
        Date.now(),
      ),
    ).toMatch(/Cleanup eligible in/i);
    expect(
      lifecycleCollapsedSummary(
        { client_state: "orphaned", auto_close_at_ms: Date.now() - 1_000 },
        t,
        "en",
        Date.now(),
      ),
    ).toMatch(/Waiting for Runtime cleanup/i);
  });

  it("models closing and collapsed risk summaries", () => {
    expect(lifecycleHintModel({ client_state: "closing" })).toEqual({
      kind: "closing",
    });
    expect(
      lifecycleCollapsedSummary({ client_state: "closing" }, t, "en"),
    ).toBe("Closing now");
    expect(
      lifecycleCollapsedSummary(
        { client_state: "orphaned" },
        t,
        "en",
      ),
    ).toMatch(/Orphaned/i);
    expect(
      lifecycleCollapsedSummary({ client_state: "connected" }, t, "en"),
    ).toBeNull();
  });
});
