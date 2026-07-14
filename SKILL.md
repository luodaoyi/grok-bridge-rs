---
name: grok-build
description: Delegate concrete coding, repair, and follow-up tasks to Grok Build through the bundled stateful CLI wrapper. Use when Codex should plan and audit while Grok implements, when live Grok status is useful, or when the user explicitly asks for Grok, grok-build, or the wrapper. Do not use for questions Codex can answer without delegation.
---

# Grok Build

Use the platform binary bundled beside this file. Resolve `<skill-dir>` as the directory containing this `SKILL.md`.

## Select the wrapper

- Windows ARM64: `<skill-dir>/bin/windows-arm64/grok-bridge.exe`
- Windows x86_64/AMD64: `<skill-dir>/bin/windows-x86_64/grok-bridge.exe`
- macOS Apple Silicon: `<skill-dir>/bin/macos-arm64/grok-bridge`
- Linux ARM64/aarch64: `<skill-dir>/bin/linux-arm64/grok-bridge`
- Linux x86_64/AMD64: `<skill-dir>/bin/linux-x86_64/grok-bridge`

Run `<wrapper> doctor` when Grok availability is uncertain. Set `GROK_BRIDGE_STATE_DIR` before the first command when session data must live somewhere other than the platform default. Every protocol command writes one JSON object with `ok` and either `result` or `error`.

## Session workflow

1. Inspect the repository and define acceptance criteria.
2. Send `start` a UTF-8 JSON object containing `prompt`, absolute `cwd`, optional `timeout_seconds`, `auto_approve`, and optional `model`. Save `result.handle`; this wrapper handle is distinct from Grok's UUID.
3. Use `read --session <handle> --cursor <n>` for incremental events. Save `result.next_cursor`. Thought text is intentionally hidden; activity, heartbeat, answer text, completion, errors, and usage remain observable.
4. Use `wait --session <handle> --for tui-idle --timeout-ms <n>` with an explicit timeout. `idle` means the turn completed successfully and can receive a follow-up.
5. Inspect the diff and run repository checks independently.
6. Before a follow-up, read remaining events. Send a focused JSON prompt through `send --session <handle>`, then repeat `read`/`wait`. The wrapper owns Grok's resume UUID.
7. Use `stop --session <handle>` to terminate active work. Stop after five total implementation rounds unless the user requests more.
8. After final audit and when no follow-up is needed, use `remove --session <handle>` to delete the non-active session's local state and events.

Useful commands:

```text
<wrapper> list
<wrapper> status --session <handle>
<wrapper> read --session <handle> --cursor 0 --limit 200 --wait-ms 5000
<wrapper> wait --session <handle> --for tui-idle --timeout-ms 300000
<wrapper> stop --session <handle>
<wrapper> remove --session <handle>
```

## Windows UTF-8 invocation

Windows PowerShell 5.1 defaults native pipelines to ASCII and will turn Chinese text into `?`. Set UTF-8 before every `start` or `send`; this is safe in PowerShell 7 too.

```powershell
$wrapper = '<wrapper-path>'
$utf8 = New-Object System.Text.UTF8Encoding($false)
$OutputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$request = @{
    prompt = '实现修改，保留中文编码，并运行测试。'
    cwd = (Get-Location).Path
    timeout_seconds = 1800
    auto_approve = $true
    model = $null
} | ConvertTo-Json -Compress
$started = $request | & $wrapper start | ConvertFrom-Json
$handle = $started.result.handle
& $wrapper wait --session $handle --for tui-idle --timeout-ms 300000
```

Follow-up:

```powershell
$utf8 = New-Object System.Text.UTF8Encoding($false)
$OutputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$request = @{ prompt = '只修复验收发现的问题。'; timeout_seconds = 1800 } | ConvertTo-Json -Compress
$request | & $wrapper send --session $handle
```

## macOS and Linux invocation

```sh
<wrapper-path> start <<'JSON'
{"prompt":"Implement the change and run relevant tests.","cwd":"/absolute/project/path","timeout_seconds":1800,"auto_approve":true,"model":null}
JSON
```

Use a returned handle with the same `status`, `read`, `wait`, `send`, and `stop` commands shown above. Never put secrets in prompts. Keep `auto_approve` false for untrusted repositories, and never edit the same files concurrently with Grok.
