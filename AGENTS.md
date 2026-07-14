# Repository Guidelines

## 项目定位与结构

本仓库是 `grok-build` Agent Skill 的源码。根 `SKILL.md` 定义 Codex 工作流，`agents/openai.yaml` 提供 UI 元数据，`src/main.rs` 实现单一 Rust wrapper，`.github/workflows/release.yml` 构建并发布完整 Skill ZIP。发布包中的 `bin/<platform>/` 由 CI 生成，不在源码仓库维护预编译文件。不要加入 GUI、网络服务、数据库、Git/worktree 管理、内置规划器或多 provider 抽象。

## 开发规则

- `start`/`send` 从 STDIN 读取一个 UTF-8 JSON 请求；所有协议命令向 STDOUT 写一个 JSON 结果，诊断写 STDERR。
- 使用 `tokio::process::Command` 和独立参数启动 Grok，禁止 shell 拼接。
- wrapper 句柄用于 `status/read/wait/send/stop`；Grok UUID 仅由 worker 保存，续轮通过 `--resume <UUID>` 恢复。
- 后台 worker 直接消费 `streaming-json`。保留 activity、heartbeat、文本、usage 和结束状态，不持久化 thought 文本或完整 prompt。
- `read` 必须保持 cursor 增量语义；`wait --for tui-idle` 只有成功进入 `idle` 才算达成。
- `remove` 只能删除非活动会话，并且删除目标必须是状态根目录的直接子目录。
- Unix 状态目录必须保持 `0700`，状态、事件、请求、停止标记和 worker 锁文件必须保持 `0600`。
- Windows PowerShell 5.1 调用示例必须先把 `$OutputEncoding` 和 `[Console]::OutputEncoding` 设置为无 BOM UTF-8，不能使用默认 ASCII/旧代码页处理协议。
- 保留目录 canonicalize、`GROK_BRIDGE_ALLOWED_ROOTS`、超时、输出截断、prompt 脱敏和参数验证。
- 协议字段变化必须同步更新测试、README 和 `SKILL.md`。
- Rust 使用 Rustfmt 默认风格；函数用 `snake_case`，类型用 `PascalCase`，常量用 `SCREAMING_SNAKE_CASE`。

## 测试与完成定义

默认测试不得调用真实 Grok 或消耗额度。优先覆盖 JSON/UTF-8 解析、UUID、参数生成、状态迁移、cursor、会话删除边界、Unix 权限、路径限制、输出截断和 thought 脱敏。任何代码修改完成前必须通过：

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

同时确认 Release workflow 覆盖 Windows ARM64/x86_64、macOS ARM64、Linux ARM64/x86_64，并将根 `SKILL.md`、`agents/` 与五个二进制组装为 `grok-build/` 顶层目录的 ZIP。

`.github/workflows/ci.yml` 必须在 `main` 与 Pull Request 上运行上述四项检查；`.github/workflows/release.yml` 仅由 `v*` Tag 构建并发布五平台 ZIP。

## Commit、Tag 与 Release

提交信息应说明改了什么、为何修改、行为变化和排查线索。PR 必须描述范围、风险、验证命令及受影响平台；协议变化需附 JSON 示例。本地 Agent 不自动 commit、push 或创建 Tag。维护者推送 `v*` Tag 后，GitHub Actions 可以按预期自动创建或更新 Release。
