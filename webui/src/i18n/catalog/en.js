/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge Sessions",
  "doc.description":
    "Grok Bridge local session console: Codex supervisors and Grok subagent management",

  "app.skipToSessions": "Skip to session list",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Supervisor & Subagent Console",
  "app.subtitle":
    "Each Codex session serves as a supervisor; the Grok sessions it launches are its sub-agents. Terminal output is pushed in real time via WebSocket. Closing the page does not terminate existing sessions.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Open grok-bridge-rs on GitHub",
  "app.github": "GitHub",

  "connection.initial": "Connecting live channel",
  "connection.connected": "Live channel connected",
  "connection.disconnected": "Live channel disconnected",
  "connection.retrying": "Reconnecting live channel",
  "connection.reconnect": "Reconnect",
  "connection.reconnectAria": "Manually reconnect live channel",

  "header.expandAll": "Expand all",
  "header.collapseAll": "Collapse all",

  "lang.label": "Language",
  "lang.aria": "Interface language",

  "theme.aria": "Color theme",
  "theme.auto": "Auto",
  "theme.light": "Light",
  "theme.dark": "Dark",
  "theme.title": "{label} theme",

  "stats.aria": "Session statistics",
  "stats.owners": "Supervisors (Codex)",
  "stats.sessions": "Subagents (Grok)",
  "stats.working": "Working",
  "stats.waiting": "Waiting",
  "stats.done": "Done / Idle",

  "stream.initial": "Connecting live channel…",
  "stream.retrying": "Reconnecting live channel…",
  "stream.disconnected": "Live channel disconnected, waiting to reconnect…",
  "stream.updated": "Live update: {time}",
  "stream.waitingData": "Live channel connected, waiting for session data…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error": "Live channel error: {detail} (will retry automatically)",

  "connecting.title": "Connecting live channel",
  "connecting.body":
    "A WebSocket connection to the local Runtime is being established; the first session snapshot will render immediately.",

  "empty.title": "No Grok sessions",
  "empty.body":
    "Grok sessions started by Codex appear here automatically as subagents, with terminal output and lifecycle status.",

  "board.aria": "Supervisor groups",

  "update.title": "Update available: v{version}",
  "update.body":
    "Current Runtime is v{current}. Download and replace the local binary manually, then restart to apply.",
  "update.openRelease": "Open latest Release",
  "update.dismiss": "Remind me later",

  "error.renderTitle": "Page render error",
  "error.reload": "Reload",
  "error.unknown": "unknown error",
  "error.timeout": "Request timed out; retrying automatically",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Working",
  "activity.waiting": "Waiting",
  "activity.done": "Done",
  "activity.stopped": "Exited",
  "activity.unknown": "Unknown",

  "client.unmanaged": "Untracked",
  "client.connected": "Codex online",
  "client.disconnected": "Codex disconnected",
  "client.orphaned": "Awaiting auto-cleanup",
  "client.closing": "Cleaning up",
  "client.unknown": "Unknown",

  "lifecycle.unmanaged": "Unmanaged",
  "lifecycle.connected": "Supervisor online",
  "lifecycle.disconnected": "Supervisor offline",
  "lifecycle.orphaned": "Cleanup countdown",
  "lifecycle.closing": "Cleaning up",
  "lifecycle.unknown": "Unknown",

  "badge.phase": "PTY phase: {phase}",

  "group.supervisor": "Supervisor · Codex",
  "group.unowned": "Unlabeled Codex conversation",
  "group.subagentCount": "{n} subagents",
  "group.closeAll": "Close all Grok for this Codex",
  "group.closeAllAria":
    "Close all Grok subagents under supervisor {owner}",
  "group.summary.working": "{n} working",
  "group.summary.waiting": "{n} waiting",
  "group.summary.done": "{n} done/idle",
  "group.summary.sep": " · ",
  "group.summary.none": "No status",
  "group.idPrefix": "id {id}",

  "session.subagent": "Subagent",
  "session.close": "Close Grok",
  "session.closeAria": "Close subagent {id}",
  "session.waitingCollapsed": "Waiting: {reason}",
  "session.waitingNote": "Waiting for Codex: {reason}",
  "session.meta.id": "Session ID",
  "session.meta.pid": "Process",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Last updated",
  "session.meta.client": "Codex connection",
  "session.meta.autoClose": "Auto-cleanup countdown",
  "session.meta.hook": "Latest Hook",
  "session.meta.tool": "Current tool",
  "session.meta.cwd": "Working directory",
  "session.terminalAria": "Terminal for subagent {title}",

  "session.lifecycle.disconnectedTitle": "Supervisor offline — not closing yet",
  "session.lifecycle.disconnectedBody":
    "Running or Waiting stages are never auto-closed. Grace starts only after a safe Idle or terminal stage.",
  "session.lifecycle.orphanedTitle": "Auto-close countdown",
  "session.lifecycle.orphanedCountdown": "Eligible for cleanup in {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Past eligibility deadline — waiting for Runtime cleanup",
  "session.lifecycle.orphanedAt":
    "Local cleanup eligibility deadline {at}; Runtime cleans up shortly after",
  "session.lifecycle.orphanedNoDeadline":
    "Orphaned; Runtime did not report a cleanup eligibility deadline.",
  "session.lifecycle.closingTitle": "Closing session",
  "session.lifecycle.closingBody":
    "This managed session is being closed now.",
  "session.lifecycle.collapsedOrphaned": "Cleanup eligible in {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "Waiting for Runtime cleanup",
  "session.lifecycle.collapsedOrphanedUnknown": "Orphaned — auto-close pending",
  "session.lifecycle.collapsedClosing": "Closing now",

  "terminal.header": "Terminal · read-only live",
  "terminal.headerInteractive": "Terminal · interactive",
  "terminal.aria": "Terminal for {id}",
  "terminal.resizeAria": "Resize terminal height",
  "terminal.resizeTitle": "Drag to resize terminal height",
  "terminal.resizeHint": "Use arrow keys to resize; Enter resets to default height",
  "terminal.resizeValue": "{height} pixels",

  "interactive.label": "Keyboard",
  "interactive.on": "On",
  "interactive.off": "Off",
  "interactive.aria": "Toggle interactive keyboard input for all terminals",
  "interactive.offHint": "Enable keyboard input to all Grok terminals",
  "interactive.warning": "Interactive mode is on: keystrokes and paste are sent to live Grok sessions. Terminal resize always follows the visible viewport. Reload resets keyboard input to read-only.",
  "interactive.disconnected": "Live channel disconnected; input is not sent and is not buffered.",
  "interactive.invalidPayload": "Invalid terminal command payload",
  "interactive.sendFailed": "Failed to send the terminal command over the live channel",
  "interactive.error": "Terminal input failed: {detail}",
  "interactive.unavailable": "Terminal input unavailable",
  "interactive.unavailableShort": "input offline",

  "action.confirmCloseSession": "Close {id} and its Grok process?",
  "action.closedSession":
    "Closed Grok session {id}. Live channel will push updates.",
  "action.closeFailed": "Close failed: {detail}",
  "action.confirmCloseGroup":
    "Close all {count} Grok sessions under Codex “{owner}”?",
  "action.groupEmpty": "This Codex group has no active Grok sessions.",
  "action.groupPartial":
    "Closed {closed}/{matched} sessions; failures: {failures}",
  "action.groupClosed":
    "Closed all {count} Grok sessions under Codex “{owner}”. Live channel will push updates.",
  "action.failureJoin": ", ",
};
