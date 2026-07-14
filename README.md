# grok-build Skill

这是一个通过预编译 CLI wrapper 调用 Grok Build 的 Codex/Agent Skill。Codex 负责规划、监控、审计和验收；wrapper 负责后台运行 Grok、保存会话状态并通过 JSON STDIN/STDOUT 提供实时查询。无需 MCP、HTTP 服务、Rust 工具链或安装脚本。

## 安装

先安装并登录 Grok Build CLI，确认 `grok --version`。然后从 GitHub Releases 下载 `grok-build-skill-<版本>.zip` 及 `.sha256`，校验后解压到用户 Skills 目录：

```powershell
Expand-Archive .\grok-build-skill-v0.2.0.zip "$env:USERPROFILE\.agents\skills"
```

```sh
unzip grok-build-skill-v0.2.0.zip -d "$HOME/.agents/skills"
```

完成后应存在 `$HOME/.agents/skills/grok-build/SKILL.md`。重启 Codex，在项目中调用 `$grok-build`。

## Release 包结构

```text
grok-build/
├── SKILL.md
├── agents/openai.yaml
└── bin/
    ├── windows-arm64/grok-bridge.exe
    ├── windows-x86_64/grok-bridge.exe
    ├── macos-arm64/grok-bridge
    ├── linux-arm64/grok-bridge
    └── linux-x86_64/grok-bridge
```

Windows “x86” 按 x86_64 发布，不提供 32 位构建；macOS 仅支持 Apple Silicon。

## 实时会话协议

协议借鉴 Orca CLI 的终端工作流，但不引入 PTY、守护服务或编排平台：每次 CLI 调用只完成一个动作，后台 worker 通过本地状态目录衔接。

```text
start  → status/read(cursor) → wait(tui-idle)
                                  ↓
                              send → read/wait
                                  ↓
                                 stop
```

`start` 从 STDIN 读取：

```json
{"prompt":"实现登录限流并运行测试。","cwd":"D:\\Projects\\my-app","timeout_seconds":1800,"auto_approve":true,"model":null}
```

它立即返回 wrapper `handle`。后续命令：

```text
grok-bridge status --session <handle>
grok-bridge read --session <handle> --cursor 0 --limit 200 --wait-ms 5000
grok-bridge wait --session <handle> --for tui-idle --timeout-ms 300000
grok-bridge send --session <handle>
grok-bridge stop --session <handle>
grok-bridge remove --session <handle>
grok-bridge list
```

`read` 返回 `oldest_cursor`、`next_cursor`、`latest_cursor` 和 `limited`。事件包括 heartbeat、activity、文本增量、结束原因、token usage 和失败信息；Grok 的 thought 文本不落盘。成功状态为 `idle`，续轮由 wrapper 自动使用 Grok UUID 执行 `--resume`。

## Windows 中文编码

Windows PowerShell 5.1 的原生管道默认使用 ASCII，中文会在进入 wrapper 前变成 `?`。必须显式设置无 BOM UTF-8：

```powershell
$utf8 = New-Object System.Text.UTF8Encoding($false)
$OutputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$request | & $wrapper start
```

`$OutputEncoding` 保护写给 wrapper 的请求，`[Console]::OutputEncoding` 保护 wrapper 返回的中文 JSON。PowerShell 7 也可以使用相同设置。wrapper 接受 UTF-8 BOM，但无法恢复已经被 PowerShell 替换成问号的内容。

## 状态与安全

- 默认状态目录：Windows `%LOCALAPPDATA%\grok-bridge\sessions`；macOS `~/Library/Application Support/grok-bridge/sessions`；Linux `$XDG_STATE_HOME/grok-bridge/sessions` 或 `~/.local/state/grok-bridge/sessions`。
- 用 `GROK_BRIDGE_STATE_DIR` 覆盖状态目录，用 `GROK_BRIDGE_ALLOWED_ROOTS` 限制允许的 `cwd`，用 `GROK_BIN` 指定 Grok 路径。
- Unix 状态目录和会话目录使用 `0700`，prompt、状态、事件和锁文件使用 `0600`。完成审计且不再续轮后，用 `remove` 删除非活动会话数据。
- prompt 只短暂写入待处理请求文件，worker 读取后立即删除；状态和事件不回显完整 prompt。
- 默认保持 `auto_approve: false`。每轮后由 Codex 检查 diff 并运行测试；不要让 Codex 与 Grok 并发修改相同文件。
- Release 下载后校验 SHA-256。macOS 隔离属性仅应在确认来源后由用户自行解除。

## 开发与发布

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

推送 `v*` Tag 后，`.github/workflows/release.yml` 会检查代码、原生构建五个平台、组装完整 Skill ZIP、生成 SHA-256 并发布 Release。本地 Agent 不自动 commit、push、建 Tag 或发布。

`main` 分支和 Pull Request 由 `.github/workflows/ci.yml` 在 Ubuntu 上运行同样的 Rust 检查；发布包仍只由 `v*` Tag 触发。
