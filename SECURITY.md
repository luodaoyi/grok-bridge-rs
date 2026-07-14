# Security

The `grok-build` Skill starts Grok Build in a requested directory and keeps local session state so later CLI calls can monitor, resume, or stop it. Grok may modify files and run commands with the current user's permissions.

Recommended controls:

- Keep `auto_approve` false for untrusted repositories or prompts.
- Configure `GROK_BRIDGE_ALLOWED_ROOTS` to restrict accepted working directories.
- Set `GROK_BRIDGE_STATE_DIR` when the default per-user state directory is unsuitable. Protect that directory with normal user-only filesystem permissions.
- In Windows PowerShell 5.1, set both `$OutputEncoding` and `[Console]::OutputEncoding` to `New-Object System.Text.UTF8Encoding($false)` before piping JSON to the wrapper. The default ASCII outbound pipeline replaces Chinese and other non-ASCII characters before validation, while a legacy console code page can corrupt displayed JSON.
- Download the Skill archive only from a trusted GitHub Release and verify its published SHA-256 before use.
- Review `git diff` and run relevant tests after every invocation.
- Keep the wrapper local. Its CLI/STDIN/STDOUT protocol and local state files do not provide remote authentication or authorization.
- macOS users should remove quarantine attributes only after verifying the Release source and checksum.

Do not place credentials in prompts. A prompt is written briefly to `request.json` and deleted when the worker accepts the turn; crashes before pickup can leave that request on disk. On Unix, state directories are forced to mode `0700` and session files to `0600`; Windows uses the current user's inherited ACLs. Events omit thought text and full prompts, but answer text, diagnostics, token usage, working directory, wrapper handles, and Grok session UUIDs remain in the local state directory. Treat both handles as execution context, do not share the state directory across untrusted users, and run `remove --session <handle>` after final audit when no follow-up is needed.
