---
name: grok-build
description: Use Grok Build as a persistent local coding subagent for implementation, repair, testing, and follow-up work through the bundled Runtime CLI on Windows, Linux, or macOS. Use when Codex should delegate a bounded task while retaining planning and final-review ownership, when machine-readable create/read/wait/send control or the localhost session WebUI is useful, or when the user explicitly asks for Grok, grok-build, or the bundled wrapper. Supports multiple independent sessions and an optional per-session egui terminal for human takeover. Requires an authenticated Grok CLI.
---

# Grok Build Local Runtime

Resolve `<skill-dir>` as the directory containing this file, then select the bundled executable for the host:

| Host | Executable |
| --- | --- |
| Windows x86_64 | `<skill-dir>/bin/windows-x86_64/grok-bridge.exe` |
| Windows ARM64 | `<skill-dir>/bin/windows-arm64/grok-bridge.exe` |
| Linux x86_64 | `<skill-dir>/bin/linux-x86_64/grok-bridge` |
| Linux ARM64 | `<skill-dir>/bin/linux-arm64/grok-bridge` |
| macOS x86_64 | `<skill-dir>/bin/macos-x86_64/grok-bridge` |
| macOS Apple Silicon | `<skill-dir>/bin/macos-arm64/grok-bridge` |

Refer to the selected executable as `<bridge>` below. Do not download another wrapper or invoke Python.

## Subagent Model

Treat each created Grok session as a persistent subagent handle, not as a one-shot shell command. Codex owns task decomposition, integration, user communication, and final verification; Grok owns the concrete implementation task delegated to that session.

Before `create`, define a delegation contract containing:

- one bounded objective and the repository context needed to act;
- the allowed write set and any shared-resource lock;
- observable acceptance criteria and the exact checks to run;
- relevant repository constraints and prohibited external or irreversible actions.

Reuse the same session for questions, evidence, corrections, and retries on that task. Independent tasks may use separate sessions concurrently when their write sets and shared resources do not overlap; tasks with ordering dependencies stay sequential. Codex may continue non-conflicting inspection, planning, or verification while Grok works. Do not create duplicate sessions for the same task merely because a turn is slow.

For a user-authorized task in a trusted repository, use `--always-approve` when the delegation contract is sufficiently bounded for Grok to edit and run non-destructive checks autonomously. Omit it when the repository, prompt, write scope, or requested commands are not trusted. Automatic approval never expands the user's authorization or permits secrets, publication, destructive operations, or writes outside the declared scope.

## Workflow

1. Inspect the repository, current changes, constraints, and acceptance criteria.
2. Run `<bridge> hooks install` before the first session and after replacing the bundled executable. The command is idempotent and updates only the managed entries in `$GROK_HOME/hooks/grok-bridge.json`, or the default `~/.grok/hooks/grok-bridge.json` when `GROK_HOME` is unset. Release archives also include fresh-install templates under `hooks/windows` and `hooks/unix`, but `hooks install` is required for custom Skill paths and safely preserves unrelated entries. Run `<bridge> doctor` if Grok availability is uncertain. `hooks status` always returns JSON, so inspect its `installed` field instead of relying only on the exit code.
   If session creation reports that the Grok state directory is not writable, the Runtime likely inherited a filesystem sandbox. Inspect `list` first so unrelated sessions are not interrupted, then stop the affected singleton and run `<bridge> server start` from a user context that can write `GROK_HOME` or the default `~/.grok` before retrying `create`.
3. Create one focused session per delegated task. Include the delegation contract in the prompt and select automatic approval according to the Subagent Model. Serialize the contract as one line before passing it to `--prompt`: keep the labeled clauses, but replace CR/LF with spaces. This preserves the whole contract even when a Windows installation resolves Grok through a command shim, where embedded line breaks can otherwise submit only the first line or be interpreted by the shell.

```text
<bridge> create --cwd <absolute-repository-path> --owner "<short-current-Codex-conversation-title>" --prompt "<single-line-delegation-contract>" [--always-approve]
```

Before every `create`, summarize the current Codex conversation into a short, recognizable, non-secret title and pass it through `--owner`. Reuse exactly the same title for later Grok sessions created by that Codex conversation. The CLI automatically attaches `CODEX_THREAD_ID`, falling back to `CODEX_SESSION_ID`, as the stable machine identity; the WebUI groups by that identity and displays owner as the readable title. Skill-driven calls must still provide the title explicitly. Optional creation arguments are `--model <model>` and `--always-approve`. Parse the JSON response, save `result.value.session`, and associate that handle with its delegated task and write lock.

4. Treat the returned handle like a running subagent. If its result is not an immediate dependency, continue non-conflicting work and inspect it at a useful boundary. Otherwise wait for the TUI to become idle, then read the terminal state. Save `next_cursor` for incremental reads.

```text
<bridge> wait --session <session> --for tui-idle --timeout-ms 300000
<bridge> read --session <session> --cursor 0 --limit 4096 --wait-ms 5000
```

5. Inspect the Session JSON fields `activity`, `hook_event`, `tool_name`, and `waiting_reason` after `create`, `list`, or `show`. If `blocked_reason` is present, inspect `show` and send the exact answer required by the visible prompt. Grok lifecycle Hooks report `ask_user_question` and other recognized interactive waits before terminal-title polling would; routine permission notifications remain record-only, so terminal prompt detection is still authoritative. Do not treat a blocked prompt as completion.

Every identified RPC refreshes the current Codex lease. If independent inspection or testing will run longer than the configured lease without another Bridge command, issue `<bridge> heartbeat` before and after that work. Do not invent or override `CODEX_THREAD_ID`/`CODEX_SESSION_ID`; use the environment supplied by Codex.

6. When the task reaches idle, independently inspect `git status` and `git diff`, then run the repository's required checks. Runtime success, a confident terminal report, or `tui-idle` is not proof that the task passed.
7. Send focused follow-up evidence through the same PTY session, then repeat `wait`, `read`, and verification. Keep ownership with that session until its bounded task passes or is explicitly abandoned.

```text
<bridge> send --session <session> --text "Fix only the verified failures and rerun the checks."
<bridge> wait --session <session> --for tui-idle --timeout-ms 300000
```

8. Interrupt a stuck turn with `send --interrupt`. Close a session when its task is complete or it is no longer useful, releasing its write lock. At the end of the whole Codex task, run `close-codex` so every Grok created by this Codex identity is cleaned up without affecting other Codex sessions.

```text
<bridge> close --session <session>
<bridge> close-codex
```

There is no fixed session-count limit. Concurrency is determined by useful independent tasks, disjoint write sets, and available machine resources—not an arbitrary session target. Close unused sessions because every live Grok process consumes local resources.

## Delegation Prompt

Draft a compact prompt in the following shape and fill it with task-specific facts rather than generic instructions. Before calling `create`, flatten the draft to one line while retaining the labels; do not pass literal CR or LF characters through `--prompt`.

```text
Act as the implementation subagent for this bounded task.

Objective:
<one concrete outcome>

Context:
<relevant architecture, current behavior, and evidence>

Write scope and locks:
<files or directories this session may modify; resources no other worker may use concurrently>

Acceptance criteria:
1. <observable requirement>
2. <observable requirement>

Required checks:
<targeted tests, lint, type checks, or builds>

Constraints:
<repository rules, compatibility requirements, and forbidden actions>

Implement the task, run the required checks, inspect your diff, and report changed files, check results, and remaining risks. Do not commit, push, publish, or modify anything outside the write scope.
```

The final argument should resemble:

```text
Objective: <outcome>. Context: <facts>. Write scope and locks: <scope>. Acceptance criteria: (1) <requirement>; (2) <requirement>. Required checks: <checks>. Constraints: <constraints>. Implement, verify, inspect the diff, and report results and risks; do not commit, push, publish, or write outside scope.
```

## Session WebUI

Use the built-in WebUI only when the user wants a browser overview or manual cleanup:

```text
<bridge> server ui
```

The command starts the singleton Runtime if needed and opens its WebUI in the default browser. The page summarizes working, waiting, and completed activity, groups Grok sessions by stable Codex identity while displaying the owner title, and keeps each group collapsible across automatic refreshes. The top-right theme control defaults to the operating-system color scheme and can persist an explicit light or dark choice. Each session card prominently distinguishes active keepalive, disconnected-but-running protection, the exact idle cleanup deadline, and cleanup in progress; orphaned sessions show a locally ticking countdown and the absolute close time. Cards also show the configured lease and grace durations, terminal screen, most recent Hook, active tool, waiting reason, process ID, last-update age, and working directory. After checking the visible terminals, close either one Grok process or every process in that Codex identity group; other groups remain running. Closing the browser tab does nothing to sessions.

The default lease is 120 seconds with a 600-second orphan grace period. While a WebUI `/api/events` WebSocket remains attached, the Runtime refreshes the managed Codex leases every 10 seconds; after the socket disconnects, the normal lease and grace countdown resumes. Only idle or terminal sessions are auto-removed after both periods; working or waiting sessions are never automatically killed merely because Codex disconnected. `GROK_BRIDGE_CODEX_LEASE_SECONDS` and `GROK_BRIDGE_ORPHAN_GRACE_SECONDS` can adjust the policy before the singleton Server starts.

`wait --timeout-ms` controls only how long that RPC blocks before returning `timed_out`; it never closes the Grok process. Automatic process cleanup follows the lease, safe-phase, and orphan-grace rules above.

The default address is `127.0.0.1:47653`. Keep `GROK_BRIDGE_WEB_ADDR` on a loopback address because the WebUI has no user authentication. If the port cannot be bound, JSON CLI and PTY sessions continue to work but `server ui` reports that the WebUI is unavailable.

## Human Takeover

Open the egui terminal only when the user requests an interactive view or manual takeover:

```text
<bridge> terminal --session <session>
```

Use `terminal [--cwd <path>] [--prompt <text>] [--model <model>] [--owner <label>] [--always-approve]` to create a session and open it immediately. Closing the window only detaches; use the explicit close action to terminate Grok. The egui terminal is a per-session interactive client, not the session-management panel. Do not use it as the normal Codex automation path because it waits for human interaction and does not return a JSON result.

## Command Rules

- `hooks install|status|uninstall` manages the global Grok lifecycle Hook entries used to distinguish working, waiting, and completed turns. Install is idempotent; uninstall preserves unrelated hooks.
- `server start|status|stop|ui` manages the per-user singleton Runtime and opens its localhost WebUI.
- `create`, `list`, `show`, `read`, `send`, `write`, `resize`, `wait`, and `close` return JSON and start the Server when needed.
- `heartbeat` refreshes the current Codex lease; `close-codex` closes all sessions attached to the current Codex identity.
- `read` uses byte cursors; `show` includes `rows`, `cols`, and `screen_ansi_base64` for terminal restoration.
- `send --text` submits bracketed text with Enter; `write --data-base64` writes exact raw bytes.
- `wait --for tui-idle` reports recognized prompts through `blocked_reason`; `wait --for exit` waits for process termination.
- `terminal --session <handle>` attaches the GUI to an existing session. Without `--session`, it creates one first.

Prefer JSON `create/read/wait/show/send` for Codex-driven work. Use `write` and `resize` only when exact terminal bytes or dimensions are required. The Server owns every Grok PTY and in-memory session; the terminal and WebUI are clients. Hooks are a fail-open observation channel and never replace PTY control or add bytes to `read`; a missing Hook must not be treated as task failure. Do not edit the same files concurrently with Grok, expose secrets in prompts, owner labels, or raw input, or assume sessions survive a Server restart. By default the Runtime resolves `grok.exe` on Windows and `grok` on Unix. Use `GROK_BIN` only for a trusted native executable; on Windows do not point it at `.cmd`, `.bat`, or `.ps1` shims. Use `GROK_BRIDGE_ALLOWED_ROOTS` to restrict accepted working directories, and `GROK_BRIDGE_WEB_ADDR` only to select a trusted loopback listener.
