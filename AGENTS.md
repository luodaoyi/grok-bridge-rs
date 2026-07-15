# Repository Guidelines

## 项目结构与定位

本仓库构建跨平台 `grok-build` Agent Skill。`src/main.rs` 提供 CLI；`transport.rs` 负责自动启动和本地 IPC；`server.rs` 分发 RPC；`session.rs` 持有 Grok PTY 与有界终端状态；`protocol.rs` 定义 JSON 请求和响应；`terminal_gui.rs` 实现 egui 真实终端；`gui_fonts.rs` 发现并验证 CJK fallback 字体。根 `SKILL.md` 是 Agent 工作流，`.github/workflows/` 负责 CI 和 Release。

保持 Orca 风格本地 Runtime，不扩展为网络服务、数据库、Git/worktree 管理器、通用 Agent 平台或多 provider 抽象。

## 开发与编码规则

- 保持每用户单例 Server；CLI 通过本地 Named Pipe 调用 `create/list/show/read/send/write/resize/wait/close`。
- 所有 Grok 进程和 ConPTY 均由 Server 创建并持有。egui terminal 只附着会话，不直接启动或拥有 Grok。
- RPC 命令向 STDOUT 只写一行 JSON；诊断写 STDERR。`terminal` 是交互式例外，关窗口只 detach，显式 `close` 才终止会话。
- `send --text` 保持括号粘贴并追加 Enter 的高层语义；`write --data-base64` 必须逐字节写入，禁止隐式转码、追加回车或修改控制序列。
- `resize` 必须同时更新 ConPTY 和服务端 vt100 屏幕。`show` 保持 `rows`、`cols`、`screen_ansi_base64`，供 GUI 恢复当前状态。
- Named Pipe 使用一请求一响应 NDJSON，单帧保持 1 MiB 上限；保留 raw write 大小、终端行列、cursor 和参数验证。
- 启动外部程序必须使用独立参数，禁止 shell 拼接。`cwd` 必须 canonicalize，并保留 `GROK_BRIDGE_ALLOWED_ROOTS`。
- GUI reader 和 writer 不得阻塞 egui 线程；后台结果到达后调用 `request_repaint`。窗口销毁只停止本地 reader，不发送 Close。
- cell renderer 必须保留颜色、样式、宽字符、光标、selection、copy 和 scrollback；输入需覆盖 IME、粘贴、控制键、Alt、导航键和 resize。
- Windows 英文必须优先使用 Consolas，中文必须由验证过字形覆盖的微软雅黑 fallback 显示；保留 `GROK_BRIDGE_CJK_FONT`、`GROK_BRIDGE_CJK_FONT_INDEX` 的显式中文字体覆盖和跨平台候选发现。
- Rust 使用 Rustfmt 默认风格：函数 `snake_case`、类型 `PascalCase`、常量 `SCREAMING_SNAKE_CASE`。

## 构建与测试

默认测试不得调用真实 Grok、消耗额度或修改外部仓库。优先覆盖 CLI attach/create 互斥、协议帧、raw write、resize、ANSI 快照、cursor、PTY 生命周期、键盘映射、IME/paste、selection/copy、宽字符、scrollback、颜色和 CJK face 验证。

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

人工验证需覆盖真实 Grok TUI、中文 IME、控制键、调整窗口尺寸、关闭窗口后重新 attach，以及显式 Close 确实终止进程。

## Commit、PR 与发布

提交沿用 `feat:`、`fix:`、`docs:`、`chore:` 等前缀，并说明修改、原因、行为变化和排查线索。PR 应列出范围、风险、验证命令及 Named Pipe/ConPTY/JSON/GUI 兼容性影响。

Release 在 Windows x86_64/ARM64、Linux x86_64/ARM64 和 macOS Intel/Apple Silicon 原生 runner 上构建。ZIP 顶层必须是 `grok-build/`，且包含：

```text
SKILL.md
agents/openai.yaml
bin/windows-x86_64/grok-bridge.exe
bin/windows-arm64/grok-bridge.exe
bin/linux-x86_64/grok-bridge
bin/linux-arm64/grok-bridge
bin/macos-x86_64/grok-bridge
bin/macos-arm64/grok-bridge
```

不要自动 commit、push、创建 Tag 或发布 Release。
