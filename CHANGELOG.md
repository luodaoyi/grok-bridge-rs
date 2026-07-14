# Changelog

## Unreleased

- Added `remove --session <handle>` for explicit cleanup of non-active local session data.
- Restricted Unix state directories to mode `0700` and session files to `0600`.
- Documented `GROK_BRIDGE_STATE_DIR` in the Skill, upgraded GitHub artifact Actions to their Node.js 24-native major versions, and added Ubuntu CI for `main` and pull requests.

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
