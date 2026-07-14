# Repository Guidelines

## 项目定位与结构

本仓库是 `grok-build` Agent Skill 的源码。根 `SKILL.md` 定义 Codex 工作流，`agents/openai.yaml` 提供 UI 元数据，`src/main.rs` 实现单一 Rust wrapper，`.github/workflows/release.yml` 构建并发布完整 Skill ZIP。发布包中的 `bin/<platform>/` 由 CI 生成，不在源码仓库维护预编译文件。不要加入 GUI、网络服务、数据库、任务队列、Git/worktree 管理、内置规划器或多 provider 抽象。

## 开发规则

- wrapper 只从 STDIN 读取一个 JSON 请求，只向 STDOUT 写一个 JSON 结果；诊断写 STDERR。
- 使用 `tokio::process::Command` 和独立参数启动 Grok，禁止 shell 拼接。
- 首轮 `session_id` 为 `null`；从 Grok 输出提取 UUID，续轮通过 `--resume <UUID>` 恢复。
- 保留目录 canonicalize、`GROK_BRIDGE_ALLOWED_ROOTS`、超时、输出截断、prompt 脱敏和参数验证。
- 协议字段变化必须同步更新测试、README 和 `SKILL.md`。
- Rust 使用 Rustfmt 默认风格；函数用 `snake_case`，类型用 `PascalCase`，常量用 `SCREAMING_SNAKE_CASE`。

## 测试与完成定义

默认测试不得调用真实 Grok或消耗额度。优先覆盖 JSON 解析、UUID、参数生成、路径限制、输出截断和脱敏。任何代码修改完成前必须通过：

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

同时确认 Release workflow 覆盖 Windows ARM64/x86_64、macOS ARM64、Linux ARM64/x86_64，并将根 `SKILL.md`、`agents/` 与五个二进制组装为 `grok-build/` 顶层目录的 ZIP。

## Commit、Tag 与 Release

提交信息应说明改了什么、为何修改、行为变化和排查线索。PR 必须描述范围、风险、验证命令及受影响平台；协议变化需附 JSON 示例。本地 Agent 不自动 commit、push 或创建 Tag。维护者推送 `v*` Tag 后，GitHub Actions 可以按预期自动创建或更新 Release。
