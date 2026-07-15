---
name: grok-build
description: Delegate concrete coding, repair, testing, and follow-up tasks to Grok Build through the bundled local Runtime CLI on Windows, Linux, or macOS. Use when Codex should plan and audit while Grok works in persistent PTY sessions, when machine-readable create/read/wait/send control or the localhost session WebUI is useful, or when the user explicitly asks for Grok, grok-build, or the bundled wrapper. Supports an optional per-session egui terminal for human takeover. Requires an authenticated Grok CLI.
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

## Workflow

1. Inspect the repository, current changes, constraints, and acceptance criteria.
2. Run `<bridge> hooks install` before the first session and after replacing the bundled executable. The command is idempotent and updates only the managed entries in `$GROK_HOME/hooks/grok-bridge.json`, or the default `~/.grok/hooks/grok-bridge.json` when `GROK_HOME` is unset. Run `<bridge> doctor` if Grok availability is uncertain. `hooks status` always returns JSON, so inspect its `installed` field instead of relying only on the exit code.
3. Create one focused session. Keep automatic approval disabled unless the repository and prompt are trusted.

```text
<bridge> create --cwd <absolute-repository-path> --owner "<short-current-Codex-conversation-title>" --prompt "Implement the requested change, run relevant checks, and report the result."
```

Before every `create`, summarize the current Codex conversation into a short, recognizable, non-secret title and pass it through `--owner`. Reuse exactly the same title for later Grok sessions created by that Codex conversation so the WebUI groups them together. Environment fallback through `CODEX_THREAD_ID` or `CODEX_SESSION_ID` exists for compatibility, but Skill-driven calls must provide the human-readable title explicitly. Optional creation arguments are `--model <model>` and `--always-approve`. Parse the JSON response and save `result.value.session`.

4. Wait for the TUI to become idle, then read the terminal state. Save `next_cursor` for incremental reads.

```text
<bridge> wait --session <session> --for tui-idle --timeout-ms 300000
<bridge> read --session <session> --cursor 0 --limit 4096 --wait-ms 5000
```

Inspect the Session JSON fields `activity`, `hook_event`, `tool_name`, and `waiting_reason` after `create`, `list`, or `show`. If `blocked_reason` is present, inspect `show` and send the exact answer required by the visible prompt. Grok lifecycle Hooks report `ask_user_question` and other recognized interactive waits before terminal-title polling would; routine permission notifications remain record-only, so terminal prompt detection is still authoritative. Do not treat a blocked prompt as completion.

5. Independently inspect `git status` and `git diff`, then run the repository's required checks. Runtime success or `tui-idle` is not proof that the task passed.
6. Send focused follow-up evidence through the same PTY session, then repeat `wait`, `read`, and verification.

```text
<bridge> send --session <session> --text "Fix only the verified failures and rerun the checks."
<bridge> wait --session <session> --for tui-idle --timeout-ms 300000
```

7. Interrupt a stuck turn with `send --interrupt`. Close the session after the final audit.

```text
<bridge> close --session <session>
```

There is no fixed session-count limit. Create only the sessions needed for the task and close unused sessions because every live Grok process consumes local resources.

## Session WebUI

Use the built-in WebUI only when the user wants a browser overview or manual cleanup:

```text
<bridge> server ui
```

The command starts the singleton Runtime if needed and opens its WebUI in the default browser. The page summarizes working, waiting, and completed activity, groups Grok sessions by Codex conversation title, and keeps each group collapsible across automatic refreshes; use the expand-all and collapse-all controls when many Codex conversations are active. Each session card shows its terminal screen, most recent Hook, active tool, waiting reason, process ID, last-update age, and working directory. After checking the visible terminals, close either one Grok process or every process in that Codex title group with its batch-close button; other groups remain running. Closing the browser tab does nothing to sessions.

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
- `read` uses byte cursors; `show` includes `rows`, `cols`, and `screen_ansi_base64` for terminal restoration.
- `send --text` submits bracketed text with Enter; `write --data-base64` writes exact raw bytes.
- `wait --for tui-idle` reports recognized prompts through `blocked_reason`; `wait --for exit` waits for process termination.
- `terminal --session <handle>` attaches the GUI to an existing session. Without `--session`, it creates one first.

Prefer JSON `create/read/wait/show/send` for Codex-driven work. Use `write` and `resize` only when exact terminal bytes or dimensions are required. The Server owns every Grok PTY and in-memory session; the terminal and WebUI are clients. Hooks are a fail-open observation channel and never replace PTY control or add bytes to `read`; a missing Hook must not be treated as task failure. Do not edit the same files concurrently with Grok, expose secrets in prompts, owner labels, or raw input, or assume sessions survive a Server restart. By default the Runtime resolves `grok.exe` on Windows and `grok` on Unix. Use `GROK_BIN` only for a trusted native executable, `GROK_BRIDGE_ALLOWED_ROOTS` to restrict accepted working directories, and `GROK_BRIDGE_WEB_ADDR` only to select a trusted loopback listener.
