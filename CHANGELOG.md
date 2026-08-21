# Changelog

## Unreleased

## 0.8.11 - 2026-08-21

- Windows session spawn now starts the PTY writer/reader before the Grok PID handshake so ConPTY inherit-cursor `CSI 6n` is answered and the launcher can report its process ID. Handshake errors include bounded pre-handshake PTY output, terminate the Job Object, and drop the PTY master so a failed `create` cannot leak the helper tree.

## 0.8.10 - 2026-08-21

- Fixed Windows named-pipe `PIPE_NOWAIT` writes of modest RPC frames stalling until the I/O deadline when `WriteFile` returned `Ok(0)` instead of a partial write (Issue #27).
- When the Windows Grok launcher exits before reporting a PID, `create` now includes the launcher exit code instead of waiting out the 10s handshake timeout (Issue #27).

## 0.8.9 - 2026-08-21

- Fixed Windows server named-pipe empty `PIPE_NOWAIT` reads being treated as EOF, which caused larger RPC write requests to fail with `os error 232` (Issue #27).

## 0.8.8 - 2026-08-20

- Fixed Windows named-pipe `PIPE_NOWAIT` empty reads being treated as EOF, which made RPC clients report `protocol peer closed before sending a frame` even after the runtime had executed the request (Issue #24).

## 0.8.7 - 2026-08-18

- Hardened the release workflow so hung apt mirrors time out and artifact uploads retry after transient network errors.

## 0.8.6 - 2026-08-17

- global package TUI and hardening.

## 0.8.5 - 2026-08-06

- Refined WebUI terminal typography with a compact line height for continuous box-drawing glyphs while preserving the concrete Consolas-first xterm font stack on Windows.
- Aligned DOM monospace content with the Source Code Pro-derived fallback stack and added regression coverage for the terminal font and line-height settings.

## 0.8.4 - 2026-07-27

- Fixed exaggerated WebUI terminal character spacing on Windows by replacing the unresolved CSS-variable font with a concrete Consolas-first monospace stack for xterm.js canvas measurement and rendering.

## 0.8.3 - 2026-07-23

- Added Brazilian Portuguese and European Portuguese WebUI catalogs, locale detection, and localized page metadata.
- Corrected Portuguese session lifecycle and update-banner wording following CodeRabbit review.

## 0.8.2 - 2026-07-22

- Detached Unix Runtime startup now creates a new process session so `server start` and automatic startup survive the invoking CLI process exiting.
- Session creation now resolves relative `GROK_HOME` against the session working directory, passes the same absolute path to Grok, and verifies the effective state directory is writable before opening a PTY, returning actionable sandbox guidance instead of leaving Grok indefinitely at `Starting session...`.
- Fixed the embedded-WebUI HTTP test helper to read and write concurrently with socket timeouts, preventing macOS Intel CI deadlocks on large static assets.
- Rebuilt the session dashboard with Tabler and Bootstrap styling while preserving the live WebSocket terminal, 13-language catalog, RTL layout, read-only-by-default terminal input, responsive grouping, and light/dark/automatic themes.
- Added complete keyboard navigation and focus restoration for theme selection, plus legacy Safari navbar blur compatibility.
- Added repository-level CodeRabbit configuration for assertive Chinese reviews of pull requests targeting `main`, with Rust, WebUI, workflow, and bilingual documentation guidance alongside the existing six-platform GitHub Actions CI.

## 0.8.1 - 2026-07-20

- Moved the WebUI “close all Grok for this Codex” control into the supervisor group header so it no longer occupies a standalone body row, without toggling the details group when clicked.
- Doubled the default xterm.js terminal height from 280px to 560px (still clamped by the existing min/max and viewport cap) and made height a single Codex-group-scoped setting that syncs across every Grok terminal in that group and persists across reload without affecting other groups.
- Removed connected keep-alive and post-disconnect lease/grace explanatory banners from the WebUI in every locale, while retaining disconnected offline notices and orphaned/closing cleanup risk messaging.

## 0.8.0 - 2026-07-19

- Localized the WebUI into 13 languages with persisted browser detection, an accessible themed language picker, locale-aware formatting, and right-to-left Arabic layout.
- Replaced vertically stacked Grok cards with per-supervisor session tabs and condensed fixed lease guidance so large Codex groups remain quick to navigate.
- Added adjustable xterm.js terminal height with viewport fitting, plus a global keyboard-input switch that remains read-only by default and sends raw input and resize commands over the existing WebSocket only when enabled.
- Kept managed Codex leases alive while a WebUI event socket is attached, exposed lease and orphan-grace timing in session state, and added live cleanup deadlines and countdowns to the UI.
- Eliminated the orphan-cleanup deadline race by atomically rechecking the lease and safe phase before shutdown becomes irreversible, while allowing timely heartbeats to cancel uncommitted cleanup.

## 0.7.0 - 2026-07-19

- Replaced the WebUI's two-second session polling with a same-origin, read-only WebSocket event stream that pushes lifecycle changes and bounded terminal deltas as they happen.
- Replaced terminal text snapshots with an input-disabled xterm.js terminal that restores ANSI state, applies ordered incremental output, supports selection and copy, and keeps live buffers bounded.
- Redesigned the WebUI in light and dark themes, added reconnect state and controls, and documented both themes with screenshots in the README.
- Updated the Grok Build Skill workflow so Codex can keep persistent Grok sessions as delegated workers, inspect their evidence, and send focused follow-up tasks.

## 0.6.2 - 2026-07-17

- Added Runtime self-check for newer GitHub releases on a background interval, exposed through `GET /api/version` for the WebUI.
- WebUI shows the current Runtime version and an update banner with the latest Release link when a newer version is available; users download and replace the binary themselves.
- Version checks fail open on network errors and can be disabled with `GROK_BRIDGE_DISABLE_UPDATE_CHECK=1`.

## 0.6.1 - 2026-07-17

- Made each Grok session card collapsible inside Codex groups so multi-session dashboards stay compact without collapsing sibling sessions.
- Hardened WebUI polling against hung Runtime, invalid JSON, and non-array payloads with request timeouts, payload normalization, continuous retry, and a render error boundary.
- Added a GitHub repository link in the WebUI header: https://github.com/luodaoyi/grok-bridge-rs

## 0.6.0 - 2026-07-15

- Expanded managed Grok Build Hooks to all 14 currently documented events, including permission denial, subagent lifecycle, and compaction, and added completion guards so late tool events cannot revive a finished turn.
- Added stable Codex identity propagation from `CODEX_THREAD_ID`/`CODEX_SESSION_ID`, shared lease refresh through normal RPCs and `heartbeat`, and targeted `close-codex` cleanup.
- Added conservative orphan handling: disconnected running/waiting sessions remain available for inspection, while idle and terminal sessions are automatically removed only after the configurable lease and grace periods.
- Updated the WebUI to group by stable Codex identity, retain readable owner titles, show connection/cleanup state, and close exact Codex groups without collisions between duplicate titles.
- Added Windows and Unix Hook templates plus README to the universal Release ZIP, with CI validation for all protocol v4 events and manual `~/.grok/hooks` installation guidance.

## 0.5.1 - 2026-07-15

- Rebuilt the embedded session WebUI with React and Tailwind CSS; CI now produces one deterministic static bundle before every native Rust target embeds it into the CLI binary.
- Added persistent automatic, light, and dark themes with a top-right switcher; automatic mode follows live operating-system color-scheme changes before the first page paint.
- Applied complete light and dark color tokens across session groups, terminals, status badges, notices, and destructive controls while improving small-text contrast in both themes.

## 0.5.0 - 2026-07-15

- Redesigned the localhost WebUI as a denser session-management view with Codex owner summaries, persistent collapsible groups, live counts, stable terminal scroll positions, and responsive controls.
- Added managed Grok lifecycle Hooks that report working, waiting, tool, failure, and turn-complete events through the existing local Runtime IPC and public Session JSON without changing PTY byte cursors.
- Added `hooks install|status|uninstall`; the managed global Hook config is idempotent, preserves unrelated entries, drains stdin on every path, and fails open when the Runtime is unavailable.
- Fixed Windows Hook launch reliability with direct slash-normalized commands and an encoded PowerShell fallback for paths containing spaces, Unicode, or shell-sensitive characters.
- Assigned every Grok process a provider UUID through `--session-id`, routed Hook events to the exact Bridge session, and removed the mapping when that session closes.

## 0.4.1 - 2026-07-15

- Added one-click WebUI cleanup for all Grok sessions grouped under one Codex conversation title, while leaving every other Codex group untouched.
- Added exact UTF-8 owner routing, confirmation, result counts, and per-session failure reporting for batch close operations.

## 0.4.0 - 2026-07-15

- Extended the Runtime, persistent PTY sessions, JSON CLI, and optional egui terminal from Windows x86_64 to Windows ARM64, Linux x86_64/ARM64, and macOS Intel/Apple Silicon.
- Replaced Windows-only Grok process termination with the portable PTY child-killer interface and resolved `grok.exe` on Windows or `grok` on Unix by default.
- Added platform-specific detached Server startup and per-user local IPC identities. The generic local socket maps to Windows Named Pipes, Linux abstract sockets, and filesystem Unix sockets on macOS.
- Removed the fixed 64-session ceiling; live sessions are now limited by host resources and remain explicitly closeable through the CLI or WebUI.
- Added optional session owner labels to `create`, session state, and new `terminal` sessions, with automatic fallback to `CODEX_THREAD_ID` or `CODEX_SESSION_ID`.
- Added a Server-owned localhost WebUI and `server ui` command. It groups Grok sessions by a short Codex conversation title, live-refreshes each terminal screen and session state, and explicitly terminates selected sessions. The default listener is `127.0.0.1:47653` and can be changed with `GROK_BRIDGE_WEB_ADDR`.
- Enabled native Linux GUI builds with the eframe X11 backend while retaining platform-specific CJK font discovery and explicit font overrides.
- Added fixed native CI runners for all six release targets so tests and Clippy execute on the target architecture instead of relying on compile-only cross builds.
- Expanded the tag Release workflow to publish one portable Skill ZIP containing `SKILL.md`, `agents/openai.yaml`, and all six native binaries under consistently named `bin/<platform>-<arch>` directories.
- Restored Linux and macOS executable permissions during packaging and retained the release ZIP SHA-256 sidecar.

## 0.3.0 - 2026-07-15

- Replaced v0.2 state-file workers with one per-user Windows x86_64 Runtime Server backed by a local Windows Named Pipe and bounded NDJSON frames.
- Moved Grok process ownership into the Server, with each session running in a persistent ConPTY and retaining bounded in-memory terminal output.
- Added Orca-style `create`, `list`, `show`, `read`, `send`, `write`, `resize`, `wait`, and `close` JSON commands with automatic detached Server startup.
- Added `terminal --session <handle>` to attach an egui terminal to an existing session, plus `terminal [--cwd --prompt --model --always-approve]` to create and open a session.
- Added a cell-based terminal renderer with ANSI colors and styles, wide characters, cursor rendering, Chinese IME, bracketed paste, terminal key mappings, selection, clipboard copy, bounded scrollback, and live ConPTY resize.
- Added raw Base64 terminal writes and synchronized ConPTY/vt100 resize. `show` now reports `rows`, `cols`, and `screen_ansi_base64` so the GUI can restore the current screen before consuming cursor-based output.
- Defined terminal window closure as detach-only. Grok continues running until the user explicitly closes the session or stops the Runtime Server.
- Added TUI-idle and process-exit waits, blocked interactive prompt detection, follow-up text input, and Ctrl+C interruption.
- Added cross-platform CJK font discovery and verified face selection for the egui terminal, with `GROK_BRIDGE_CJK_FONT` and `GROK_BRIDGE_CJK_FONT_INDEX` overrides.
- Prevented the detached Server from inheriting CLI pipeline handles, normalized Windows verbatim working directories, synchronized PTY EOF with process exit, and made Server Stop reliably reap active Grok processes.
- Retained `GROK_BIN` for trusted native Grok executable selection and `GROK_BRIDGE_ALLOWED_ROOTS` for canonical working-directory restrictions.
- Limited CI and Release builds to Windows x86_64. The Skill ZIP contains only `SKILL.md`, `agents/openai.yaml`, and `bin/windows-x86_64/grok-bridge.exe`.

## 0.2.0 - 2026-07-14

- Replaced the blocking one-request protocol with stateful `start`, `status`, `read`, `wait`, `send`, `stop`, and `list` CLI commands.
- Added detached background workers that consume Grok `streaming-json`, persist bounded result fields, expose cursor-based events, and resume follow-ups with the provider session UUID.
- Added observable heartbeat, activity, answer text, usage, timeout, failure, and stop events while excluding Grok thought text and full prompts.
- Added explicit UTF-8 guidance for Windows PowerShell 5.1, whose default native-pipeline encoding corrupts Chinese text into question marks.
- Added session-state, cursor, Unicode-boundary, streaming-event, UUID, and thought-redaction tests.

## 0.1.0 - 2026-07-14

- Converted the repository root into a directly installable `grok-build` Agent Skill.
- Added a single-request STDIN/STDOUT JSON wrapper with timeout, output capture, prompt redaction, allowed-root checks, and Grok session continuation.
- Added Tag-triggered GitHub Actions builds for Windows ARM64/x86_64, macOS ARM64, and Linux ARM64/x86_64, packaged as one Skill ZIP with SHA-256.
