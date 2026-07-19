export const STOPPED_PHASES = new Set(["exited", "failed", "stopped"]);

export function activityOf(session) {
  if (STOPPED_PHASES.has(session.phase)) return "stopped";
  if (session.activity && session.activity !== "unknown") {
    return session.activity;
  }
  if (session.phase === "idle") return "done";
  if (["starting", "running"].includes(session.phase)) return "working";
  return "unknown";
}

export function activityLabel(activity) {
  return (
    {
      working: "工作中",
      waiting: "等待输入",
      done: "已完成",
      stopped: "已退出",
      unknown: "状态未知",
    }[activity] ?? activity
  );
}

export function ownerKey(owner) {
  return owner == null ? "missing-owner" : `owner:${owner}`;
}

export function sessionGroupKey(session) {
  return session.client_session_id
    ? `client:${session.client_session_id}`
    : ownerKey(session.owner ?? null);
}

export function groupSessions(sessions) {
  const grouped = new Map();
  for (const session of sessions) {
    const key = sessionGroupKey(session);
    if (!grouped.has(key)) grouped.set(key, []);
    grouped.get(key).push(session);
  }
  return [...grouped.entries()].sort(([, left], [, right]) =>
    String(left[0]?.owner ?? "").localeCompare(
      String(right[0]?.owner ?? ""),
      "zh-CN",
    ),
  );
}

export function sessionStats(sessions) {
  const activities = sessions.map(activityOf);
  return {
    owners: new Set(sessions.map(sessionGroupKey)).size,
    sessions: sessions.length,
    working: activities.filter((activity) => activity === "working").length,
    waiting: activities.filter((activity) => activity === "waiting").length,
    done: activities.filter((activity) => activity === "done").length,
  };
}

export function clientStateLabel(state) {
  return (
    {
      unmanaged: "未跟踪",
      connected: "Codex 在线",
      disconnected: "Codex 已断开",
      orphaned: "等待自动清理",
      closing: "正在清理",
    }[state] ?? "未知"
  );
}

/** Visual lifecycle for Codex lease / cleanup pipeline. */
export function clientLifecycle(state) {
  return (
    {
      unmanaged: "unmanaged",
      connected: "connected",
      disconnected: "disconnected",
      orphaned: "cleanup",
      closing: "cleanup",
    }[state] ?? "unknown"
  );
}

export function clientLifecycleLabel(state) {
  return (
    {
      unmanaged: "未托管",
      connected: "监督者在线",
      disconnected: "监督者断开",
      orphaned: "清理倒计时",
      closing: "清理中",
    }[state] ?? "状态未知"
  );
}

export function dominantClientState(sessions) {
  const priority = [
    "closing",
    "orphaned",
    "disconnected",
    "connected",
    "unmanaged",
  ];
  for (const state of priority) {
    if (sessions.some((session) => session.client_state === state)) {
      return state;
    }
  }
  return sessions[0]?.client_state ?? "unmanaged";
}

export function groupSummary(sessions) {
  const counts = { working: 0, waiting: 0, done: 0 };
  for (const session of sessions) {
    const activity = activityOf(session);
    if (activity in counts) counts[activity] += 1;
  }
  return [
    counts.working && `${counts.working} 个工作中`,
    counts.waiting && `${counts.waiting} 个等待输入`,
    counts.done && `${counts.done} 个完成/空闲`,
  ]
    .filter(Boolean)
    .join(" · ") || "无可用状态";
}

export function ageLabel(updatedAt, now = Date.now()) {
  const seconds = Math.max(0, Math.floor((now - updatedAt) / 1000));
  if (seconds < 60) return `${seconds} 秒前`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟前`;
  return `${Math.floor(seconds / 3600)} 小时前`;
}

export function remainingLabel(deadline, now = Date.now()) {
  const seconds = Math.max(0, Math.ceil((deadline - now) / 1000));
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600) return `${Math.ceil(seconds / 60)} 分钟`;
  return `${Math.ceil(seconds / 3600)} 小时`;
}

export function sessionsSignature(sessions) {
  // Terminal bytes stream separately; signature tracks metadata only.
  return JSON.stringify(
    sessions.map((session) => [
      session.session,
      session.owner,
      session.client_session_id,
      session.client_state,
      session.client_last_seen_at_ms,
      session.orphaned_at_ms,
      session.auto_close_at_ms,
      session.phase,
      session.title,
      session.cwd,
      session.process_id,
      session.updated_at_ms,
      session.activity,
      session.hook_event,
      session.hook_at_ms,
      session.tool_name,
      session.waiting_reason,
      session.rows,
      session.cols,
    ]),
  );
}
