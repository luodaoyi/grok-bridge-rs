# grok-build Local Runtime Skill

[Linux DO 社区](https://linux.do/)

`grok-build` v0.6.0 是一个可直接解压使用的跨平台 Agent Skill。Codex 通过随包发布的原生 `grok-bridge` 调用本机 Grok Runtime；不需要 Python、MCP、安装脚本或额外服务。

Runtime 维护每用户单例 Server 和持久 PTY 会话。CLI 通过本地 IPC 交换有界 NDJSON：Windows 使用 Named Pipe，Linux 使用抽象 Unix Socket，macOS 使用 `/tmp` 下的 Unix Socket。Server 持有 Grok CLI、会话状态和终端输出；RPC 命令向 STDOUT 返回一行 JSON，`terminal` 打开单会话 egui 终端，编译进二进制的 React + Tailwind localhost WebUI 则集中查看和关闭会话。

```text
Codex / JSON CLI       egui terminal
          \               /
             local IPC       localhost WebUI
                 \             /
             singleton Runtime Server
                         │
                        PTY
                         │
                     Grok CLI
```

## 支持平台

| 平台 | Release 目录 | Rust target |
| --- | --- | --- |
| Windows x86_64 | `bin/windows-x86_64` | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `bin/windows-arm64` | `aarch64-pc-windows-msvc` |
| Linux x86_64 | `bin/linux-x86_64` | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `bin/linux-arm64` | `aarch64-unknown-linux-gnu` |
| macOS Intel | `bin/macos-x86_64` | `x86_64-apple-darwin` |
| macOS Apple Silicon | `bin/macos-arm64` | `aarch64-apple-darwin` |

所有平台都要求已安装并登录 Grok CLI，且 `grok --version` 可执行。默认从 `PATH` 查找 Windows 上的 `grok.exe` 或 Unix 上的 `grok`；`GROK_BIN` 可指定可信的原生可执行文件，`GROK_BRIDGE_ALLOWED_ROOTS` 可限制允许创建会话的仓库根目录。

Linux GUI 使用 X11 后端。构建机需要 eframe 所需的 XCB/XKB 开发库；运行机需要可用的 X11 会话。无图形桌面时仍可使用 JSON CLI，不要调用 `terminal`。

## 安装

从 GitHub Releases 下载 `grok-build-skill-v0.6.0.zip` 和对应 `.sha256`。ZIP 同时包含六个平台的原生二进制、README 和 Windows/Unix Hook 模板，解压一次即可保留统一 Skill 目录。

Windows PowerShell：

```powershell
Expand-Archive .\grok-build-skill-v0.6.0.zip "$env:USERPROFILE\.agents\skills" -Force
$bridge = "$env:USERPROFILE\.agents\skills\grok-build\bin\windows-x86_64\grok-bridge.exe"
```

Linux/macOS：

```sh
unzip grok-build-skill-v0.6.0.zip -d "$HOME/.agents/skills"
bridge="$HOME/.agents/skills/grok-build/bin/linux-x86_64/grok-bridge"
```

按实际系统和架构替换路径；例如 Apple Silicon 使用 `bin/macos-arm64/grok-bridge`。安装后目录应为：

```text
~/.agents/skills/grok-build/
├── README.md
├── SKILL.md
├── agents/openai.yaml
├── hooks/
│   ├── windows/grok-bridge.json
│   └── unix/grok-bridge.json
└── bin/
    ├── windows-x86_64/grok-bridge.exe
    ├── windows-arm64/grok-bridge.exe
    ├── linux-x86_64/grok-bridge
    ├── linux-arm64/grok-bridge
    ├── macos-x86_64/grok-bridge
    └── macos-arm64/grok-bridge
```

Release workflow 会恢复 Linux/macOS 二进制的执行位。若解压工具丢失权限，手工执行 `chmod +x <bridge>`。当前 Release 未做 Windows 代码签名或 macOS notarization；系统出现来源警告时，应先核对 SHA-256 再决定是否放行。

## Codex 自动化工作流

先幂等安装 Grok 生命周期 Hook，再运行 `doctor`、创建会话并保存 JSON 中的 `result.value.session`：

```text
<bridge> hooks install
<bridge> doctor
<bridge> create --cwd <absolute-repository-path> --owner "<当前 Codex 对话的简短标题>" --prompt "实现当前任务并运行相关测试。"
<bridge> show --session <session>
<bridge> read --session <session> --cursor 0 --limit 4096 --wait-ms 5000
<bridge> wait --session <session> --for tui-idle --timeout-ms 300000
```

验收后可在同一 PTY 中继续输入，或显式终止会话：

```text
<bridge> send --session <session> --text "只修复验收发现的问题，并重新运行测试。"
<bridge> wait --session <session> --for tui-idle --timeout-ms 300000
<bridge> close --session <session>
```

Codex 每次创建 Grok 前都应把当前对话概括成简短、易辨认且不含秘密的标题，并通过 `--owner` 显式传入。同一个 Codex 对话创建后续 Grok 时复用完全相同的标题。Bridge 同时自动读取 `CODEX_THREAD_ID`，缺失时回退到 `CODEX_SESSION_ID`，把它作为不可见于 prompt 的稳定客户端标识；WebUI 优先按该标识分组，owner 仅作为人类可读标题。owner 和客户端标识最长 128 bytes，不能包含控制字符。

Codex 自动化应优先使用 `create/read/wait/show/send` 的 JSON 接口。每个带 Codex 标识的 RPC 都会刷新共享租约；长时间没有其他 RPC 时可显式运行 `<bridge> heartbeat`。完成整个 Codex 对话时运行 `<bridge> close-codex`，会一次关闭只属于当前 Codex 标识的全部 Grok。每轮后仍需独立检查 `git status`、`git diff` 并运行仓库要求的测试；`tui-idle` 只表示终端状态，不代表实现通过验收。Runtime 不再设置固定的 64 会话上限，但每个活跃会话都会占用 Grok 进程、PTY 和内存。

## Grok 生命周期 Hook

`hooks install` 只管理 Grok 全局 Hook 目录中的 `grok-bridge.json`，保留同一文件中的非本程序条目；重复运行是幂等的。安装状态和卸载命令为：

```text
<bridge> hooks status
<bridge> hooks uninstall
```

Release 中的模板可直接放入官方个人 Hook 目录。全新安装且目标文件不存在时，可手工复制：

```powershell
New-Item "$env:USERPROFILE\.grok\hooks" -ItemType Directory -Force | Out-Null
Copy-Item "$env:USERPROFILE\.agents\skills\grok-build\hooks\windows\grok-bridge.json" "$env:USERPROFILE\.grok\hooks\grok-bridge.json"
```

```sh
mkdir -p "$HOME/.grok/hooks"
cp "$HOME/.agents/skills/grok-build/hooks/unix/grok-bridge.json" "$HOME/.grok/hooks/grok-bridge.json"
```

模板按默认 `~/.agents/skills/grok-build` 路径自动选择本机架构。若 Skill 位于自定义目录、设置了 `GROK_HOME`，或目标 JSON 已有其他 Hook，不要直接覆盖；运行 `<bridge> hooks install`，它会合并原配置并写入当前二进制的精确路径。

配置遵循 [Grok Build 官方 Hooks 协议](https://docs.x.ai/build/features/hooks)，覆盖当前 14 个事件：`SessionStart`、`SessionEnd`、`UserPromptSubmit`、`PreToolUse`、`PostToolUse`、`PostToolUseFailure`、`PermissionDenied`、`Stop`、`StopFailure`、`Notification`、`SubagentStart`、`SubagentStop`、`PreCompact` 和 `PostCompact`。Grok 把事件 JSON 写入隐藏的 `<bridge> __hook` stdin；该进程完整读取输入、提取有界状态字段，再通过现有本地 IPC 更新对应 Session。`create/list/show/send/write/resize` 返回的 Session JSON 会携带 `activity`、`hook_event`、`hook_at_ms`、`tool_name` 和 `waiting_reason`。Stop 之后迟到的工具事件不会把已完成回合误改回工作中；只有下一次 `UserPromptSubmit` 会开始新回合。PTY 仍是输入、终端输出和进程终止的唯一控制通道，Hook 不进入 `read` 的 byte cursor。

Bridge 为每个 Grok 子进程生成随机 UUID，并通过官方 `--session-id` 参数固定 provider session ID；Hook runner 提供的 `GROK_SESSION_ID` 用于在 Runtime 内关联会话，但不会显示在 JSON 或 WebUI 中。普通终端直接启动的 Grok 也会触发全局 Hook，不过 Runtime 找不到对应 Bridge 会话时会在内部返回 `accepted: false`，隐藏入口仍静默退出 0。Runtime 未运行、事件格式变化或 IPC 失败时同样 fail-open，不会阻塞 Grok 工具调用。Hook command 默认超时 2 秒，工具事件 matcher 使用合法正则 `.*`。`hooks status` 始终返回 JSON；自动化必须读取其中的 `installed`，不能只检查进程退出码。

## localhost 会话面板

执行以下命令会按需启动单例 Runtime Server，并在默认浏览器打开 WebUI：

```text
<bridge> server ui
```

页面顶部汇总 Codex 分组、Grok 总数以及工作中、等待输入、完成/空闲数量；右上角可选择自动、浅色或深色主题，默认自动跟随系统色调，手工选择会保存在浏览器中。下方按稳定 Codex 客户端标识分组，并用 owner 显示人类可读标题；即使两个 Codex 使用相同标题，也不会混在同一组。每个会话卡片展示 Codex 连接状态、自动清理倒计时、最近 Hook、当前工具、等待原因、PID、工作目录和终端当前屏幕。页面每 2 秒刷新，也可一键全部展开或折叠。确认终端内容后，可以关闭单个 Grok，也可以点击“关闭该 Codex 全部 Grok”终止该客户端标识下的全部会话；其他 Codex 分组不受影响。

Runtime 不依赖不可靠的 CLI 父进程 PID，而使用 Codex 标识租约：默认 120 秒无 RPC 后标记断开；只有已经 `Idle`、`Exited`、`Failed` 或 `Stopped` 的安全会话才进入 600 秒宽限倒计时并自动移除。仍在工作或等待输入的 Grok 只显示“Codex 已断开”，不会被自动杀死，必须由 WebUI、`close`、`close-codex` 或 `server stop` 明确处理。可在 Server 第一次启动前用 `GROK_BRIDGE_CODEX_LEASE_SECONDS`（30–86400）和 `GROK_BRIDGE_ORPHAN_GRACE_SECONDS`（60–604800）调整秒数；非法值会阻止 Runtime 启动。状态只保存在内存中，不跨 Server 重启。

默认监听 `127.0.0.1:47653`。可在 Server 第一次启动前通过 `GROK_BRIDGE_WEB_ADDR` 更换地址；WebUI 没有用户认证，因此必须保持在可信回环地址，不要绑定 `0.0.0.0` 或对外网卡。若端口绑定失败，JSON CLI 和 PTY 会话仍可使用，但 `server ui` 会报告 WebUI 不可用。

## 交互式终端

附着到 Server 已持有的会话：

```text
<bridge> terminal --session <session>
```

也可用 `terminal [--cwd <path>] [--prompt <text>] [--model <model>] [--owner <label>] [--always-approve]` 创建会话后立即打开。关闭窗口只会 detach；只有终端工具栏的“关闭会话”、WebUI 的“关闭”、`close --session` 或 `server stop` 才会终止 Grok。egui 终端只负责单会话交互，不是会话管理面板。

终端支持 ANSI 样式、宽字符、光标、中文 IME、括号粘贴、常用控制键、文本选择、剪贴板、scrollback 和 PTY resize。启动时会发现系统字体；可用 `GROK_BRIDGE_CJK_FONT` 指定中文 `.ttf`、`.otf` 或 `.ttc`，字体集合可用 `GROK_BRIDGE_CJK_FONT_INDEX` 指定 face index。

## 命令与协议

- `server start|status|stop|ui`：管理单例 Runtime，并打开 localhost 会话面板；
- `hooks install|status|uninstall`：管理 Grok 生命周期 Hook 配置；
- `create`、`list`、`show`、`read`：创建或查询会话与有界输出；
- `heartbeat`、`close-codex`：刷新当前 Codex 租约，或关闭当前 Codex 标识下的全部会话；
- `send --text`、`send --interrupt`：提交文本或 Ctrl+C；
- `write --data-base64`、`resize`：发送原始字节或调整 PTY；
- `wait --for tui-idle|exit`：等待 TUI 空闲或进程退出；
- `close`：终止并移除会话；
- `terminal`：附着现有会话，或创建后打开 GUI；
- `doctor`：检查 Grok CLI 和 Server 状态。

每个 RPC 命令向 STDOUT 写一个 `ResponseEnvelope` JSON 对象并以换行结束。成功响应包含 `result`，失败响应包含结构化 `error`；`terminal` 是交互式例外。`read` 返回 byte cursor、Base64 原始增量、当前 screen、`truncated` 和 `eof`。`wait --for tui-idle` 遇到可识别提示时返回 `satisfied: false` 和 `blocked_reason`，调用方应先 `show` 再明确回复。

## 安全边界

- Grok 及其工具继承当前用户权限，Runtime 和 GUI 都不是沙箱；
- 只在可信仓库和可信 prompt 上使用 `--always-approve`；
- prompt 和原始输入可能出现在进程参数或本机内存中，不要传递秘密；
- Hook 的 provider session ID 只用于同一用户 Runtime 内的会话关联，不是操作系统级沙箱或对当前用户恶意进程的安全边界；
- owner 和客户端标识会显示在 JSON 与 WebUI 中，不要把用户名密码或其他秘密放入相应环境变量或 owner；
- WebUI 没有用户认证，只允许绑定可信回环地址；
- 不要让 Codex 与人工终端同时修改相同文件；
- Server 退出时会关闭全部会话，状态不跨 Server 重启持久化；
- 只从可信 Release 下载 ZIP，并验证 SHA-256。

## 开发与发布

本地完成前运行：

```text
cd webui
npm ci
npm test
npm run build
cd ..
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

`webui/` 保存 React、Tailwind 和 Vitest 源码。Rust 编译通过 `include_bytes!` 嵌入 `webui/dist/`，因此源码构建必须先生成前端产物；Release ZIP 中不包含 Node.js、依赖目录或独立网页文件。

CI 先在 Node.js 24 上执行一次前端测试和构建，再把相同的 `webui-dist` 提供给六个固定原生 runner，分别执行 Rust 测试、Clippy 和 release 构建：`windows-2025`、`windows-11-arm`、`ubuntu-24.04`、`ubuntu-24.04-arm`、`macos-15-intel` 和 `macos-15`。这既确保六个二进制嵌入同一份页面，也避免交叉编译只能验证链接、不能执行目标架构测试的问题。

`v*` Tag 触发 Release workflow，将六个二进制、`README.md`、`SKILL.md`、`agents/openai.yaml` 和两份跨架构 Hook 模板组装为一个通用 Skill ZIP，校验模板均含 14 个协议 v4 事件，并生成 SHA-256 文件。本地 Agent 不自动 commit、push、创建 Tag 或发布 Release。
