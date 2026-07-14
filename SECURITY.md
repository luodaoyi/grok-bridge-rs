# Security

The `grok-build` Skill starts Grok Build in a requested directory. Grok may modify files and run commands with the current user's permissions.

Recommended controls:

- Keep `auto_approve` false for untrusted repositories or prompts.
- Configure `GROK_BRIDGE_ALLOWED_ROOTS` to restrict accepted working directories.
- Download the Skill archive only from a trusted GitHub Release and verify its published SHA-256 before use.
- Review `git diff` and run relevant tests after every invocation.
- Keep the wrapper local. Its STDIN/STDOUT JSON protocol does not provide remote authentication or authorization.
- macOS users should remove quarantine attributes only after verifying the Release source and checksum.

Do not place credentials in prompts. The wrapper redacts the prompt from its returned command field, but Grok and Codex may retain their own session or execution logs. A returned session UUID should be treated as execution context: pass it only to follow-up requests for the same task.
