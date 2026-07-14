---
name: grok-build
description: Delegate concrete coding, repair, and follow-up fix tasks to Grok Build through the bundled headless CLI wrapper. Use when Codex should plan and audit the work while Grok implements changes, or when the user explicitly asks to use Grok, grok-build, or the bundled wrapper. Do not use for questions that Codex can answer without delegating implementation.
---

# Grok Build

Use the platform binary bundled beside this file. Resolve `<skill-dir>` as the directory containing this `SKILL.md`; never assume the user's project is the skill directory.

## Select the wrapper

Choose exactly one binary for the current host:

- Windows ARM64: `<skill-dir>/bin/windows-arm64/grok-bridge.exe`
- Windows x86_64/AMD64: `<skill-dir>/bin/windows-x86_64/grok-bridge.exe`
- macOS Apple Silicon: `<skill-dir>/bin/macos-arm64/grok-bridge`
- Linux ARM64/aarch64: `<skill-dir>/bin/linux-arm64/grok-bridge`
- Linux x86_64/AMD64: `<skill-dir>/bin/linux-x86_64/grok-bridge`

Stop and report an unsupported platform when no mapping exists. Run `<wrapper> doctor` before the first delegation when Grok availability is uncertain.

## Delegate work

1. Inspect the user's repository and define concrete acceptance criteria.
2. Build one JSON request with these fields:
   - `prompt`: complete implementation or repair instructions.
   - `cwd`: the absolute target repository directory.
   - `session_id`: `null` for the first round; use the UUID returned by the previous round for a follow-up.
   - `timeout_seconds`: normally `1800`; allowed range is 10 through 7200.
   - `auto_approve`: use `true` only for a trusted repository when Grok must edit files or run commands.
   - `model`: `null` unless the user requests a specific Grok model.
3. Write the compact JSON request to the wrapper's STDIN. Read exactly one JSON result from STDOUT. Treat STDERR as diagnostics only.
4. Check `success`, `exit_code`, `timed_out`, `session_id`, `stdout`, `stderr`, `output_truncated`, and `error`.
5. Independently inspect the working tree and run the repository's required format, test, lint, and build commands.
6. If verification fails, send a focused repair request using the returned `session_id`. Stop after five total rounds unless the user explicitly requests more.

The wrapper invokes Grok with argument arrays, not shell command concatenation. Never put secrets in `prompt`.

## Windows invocation

Use PowerShell's JSON serializer and native pipeline:

```powershell
$request = @{
    prompt = "Implement the requested change and run the relevant tests."
    cwd = (Get-Location).Path
    session_id = $null
    timeout_seconds = 1800
    auto_approve = $true
    model = $null
} | ConvertTo-Json -Compress
$request | & <wrapper-path>
```

## macOS and Linux invocation

Pipe a compact, correctly JSON-escaped request with a quoted heredoc:

```sh
<wrapper-path> <<'JSON'
{"prompt":"Implement the requested change and run the relevant tests.","cwd":"/absolute/project/path","session_id":null,"timeout_seconds":1800,"auto_approve":true,"model":null}
JSON
```

Do not call Grok directly when this wrapper is available; the wrapper enforces path validation, timeout limits, output clipping, and prompt redaction.
