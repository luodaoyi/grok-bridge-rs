# grok-build Skill

这是一个通过预编译 CLI wrapper 调用 Grok Build 的 Codex/Agent Skill。Codex 负责规划、审计和验收；`SKILL.md` 选择当前平台二进制，通过 JSON STDIN/STDOUT 同步调用 Grok。无需 MCP、Rust 工具链或安装脚本。

## 安装

先安装并登录 Grok Build CLI，确认：

```text
grok --version
```

然后从 GitHub Releases 下载 `grok-build-skill-<版本>.zip` 和对应 `.sha256`，校验后直接解压到用户 Skills 目录。

Windows：

```powershell
Expand-Archive .\grok-build-skill-v0.1.0.zip "$env:USERPROFILE\.agents\skills"
```

macOS/Linux：

```sh
unzip grok-build-skill-v0.1.0.zip -d "$HOME/.agents/skills"
```

完成后应存在：

```text
$HOME/.agents/skills/grok-build/SKILL.md
```

重启 Codex，在任意项目中明确调用 `$grok-build`。

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

Windows “x86” 按 x86_64 发布，不提供 32 位构建。macOS 仅支持 Apple Silicon。

## JSON 调用协议

wrapper 从 STDIN 读取一个 JSON 对象，等待 Grok 退出，再向 STDOUT 写入一个 JSON 结果。协议模式下日志只能写 STDERR。

首轮请求：

```json
{
  "prompt": "实现登录限流，增加测试并运行检查。",
  "cwd": "D:\\Projects\\my-app",
  "session_id": null,
  "timeout_seconds": 1800,
  "auto_approve": false,
  "model": null
}
```

结果包含 `success`、`exit_code`、`timed_out`、`session_id`、`command`、`cwd`、`stdout`、`stderr`、`output_truncated` 和 `error`。首轮成功后返回 Grok 会话 UUID；返工时原样传回该 UUID，wrapper 使用 `grok --resume <UUID>` 恢复上下文。

## 安全与配置

- 默认保持 `auto_approve: false`；只在可信仓库中启用自动批准。
- 用 `GROK_BRIDGE_ALLOWED_ROOTS` 限制允许的 `cwd`。
- Grok 不在 `PATH` 时，用 `GROK_BIN` 指定完整路径。
- 不要在 prompt 中放密钥；每轮调用后由 Codex 独立检查 diff 并运行测试。
- 从 Release 下载后校验随附 SHA-256；macOS 若保留下载隔离属性，需要由用户确认来源后自行解除。

## 开发与发布

本地代码检查：

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

推送 `v*` Tag 后，[release.yml](.github/workflows/release.yml) 会：

1. 运行完整 Rust 检查。
2. 在对应架构的 GitHub-hosted runner 上构建五个平台二进制。
3. 组装包含 `SKILL.md`、`agents/` 和 `bin/` 的单一 ZIP。
4. 生成 SHA-256。
5. 自动创建或更新同名 GitHub Release 并上传两个文件。

本地 Agent 不自动 commit、push 或创建 Tag；Release 仅由维护者推送的版本 Tag 触发。
