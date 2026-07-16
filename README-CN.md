# grok-build 本地 Runtime Skill

<p>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/actions/workflows/ci.yml?query=branch%3Amain"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/luodaoyi/grok-bridge-rs/ci.yml?branch=main&label=build&logo=githubactions&logoColor=white" /></a>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/actions/workflows/release.yml"><img alt="Release Workflow" src="https://img.shields.io/github/actions/workflow/status/luodaoyi/grok-bridge-rs/release.yml?label=release&logo=githubactions&logoColor=white" /></a>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/releases/latest"><img alt="Latest Release" src="https://img.shields.io/github/v/release/luodaoyi/grok-bridge-rs?display_name=tag&sort=semver" /></a>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/releases"><img alt="Release Downloads" src="https://img.shields.io/github/downloads/luodaoyi/grok-bridge-rs/total?label=release%20downloads" /></a>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/luodaoyi/grok-bridge-rs?style=flat" /></a>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/issues"><img alt="Issues" src="https://img.shields.io/github/issues/luodaoyi/grok-bridge-rs" /></a>
  <a href="https://github.com/luodaoyi/grok-bridge-rs/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/luodaoyi/grok-bridge-rs" /></a>
</p>

[English](README.md) | [简体中文](README-CN.md)

grok-build 让 Codex 可以稳定地在本机调用 Grok 完成真实的软件开发，不会为每次请求都留下难以管理的独立进程。它通过共享 Runtime 保持会话可见、可交互、按 Codex 对话归类，并能在浏览器中随时查看和关闭。

## 为什么使用它？

当 Codex 把任务交给 Grok 时，你不应该还要猜测某个进程属于哪个任务、现在是否仍在工作。grok-build 把这套流程集中到一个地方：

- **本地优先：** 不需要 Python、MCP Server、云端中转或额外安装后台服务。
- **一个 Runtime：** 会话由同一个本地进程创建和持有，重复调用不会留下难以管理的桥接进程。
- **实时交互：** Grok 保持真实的交互式终端，可以查看输出、继续发送指令、附加终端或停止会话。
- **归属清晰：** 按 Codex 对话和简短标题分组，即使多个 Codex 同时工作也不会混淆。
- **浏览器管理：** 执行 `server ui` 即可打开本地面板，查看状态、最近活动、终端内容、工作目录并执行关闭操作。
- **自动感知与清理：** Hook 可以报告 Grok 活动，heartbeat 和租约机制可以帮助发现断开的会话；既可以关闭单个会话，也可以关闭某个 Codex 对话的全部会话。
- **支持并发工作：** 不设置人为的 64 个会话上限，实际数量取决于机器资源和 Grok CLI。

## 快速开始

### 1. 安装 Grok CLI

先安装并登录 Grok CLI，在终端确认它可以运行：

```text
grok --version
```

### 2. 安装 Skill

从 [GitHub Releases](https://github.com/luodaoyi/grok-bridge-rs/releases) 下载最新 ZIP，解压到 Agent Skills 目录。解压后应当得到 `grok-build/` 文件夹。

压缩包包含 Windows x86_64/ARM64、Linux x86_64/ARM64、macOS Intel/Apple Silicon 的原生二进制，请根据自己的系统选择对应版本。

### 3. 检查并打开面板

Windows PowerShell：

```powershell
$bridge = "$env:USERPROFILE\.agents\skills\grok-build\bin\windows-x86_64\grok-bridge.exe"
& $bridge doctor
& $bridge hooks install
& $bridge server ui
```

Linux 或 macOS：

```sh
bridge="$HOME/.agents/skills/grok-build/bin/linux-x86_64/grok-bridge"
"$bridge" doctor
"$bridge" hooks install
"$bridge" server ui
```

需要时，将二进制目录替换为 `windows-arm64`、`linux-arm64`、`macos-x86_64` 或 `macos-arm64`。`server ui` 会按需启动本地 Runtime 并打开浏览器面板，默认地址是 `http://127.0.0.1:47653`。

安装 Skill 后，Codex 就可以直接使用它。需要了解每个 Grok 会话在做什么，或想手工接管时，再打开这个面板即可。

## 日常使用

1. 让 Codex 把开发任务交给 Grok，Skill 会为当前 Codex 对话记录简短易读的标题。
2. 执行 `server ui`，查看所有 Codex 分组，展开目标分组并阅读当前终端画面和活动状态。
3. 让 Codex 继续工作，也可以从 CLI 或附加终端发送后续指令。
4. 可以关闭单个 Grok，会话结束时使用 `close-codex` 关闭该 Codex 对话创建的全部 Grok，也可以在完成后停止整个 Runtime。

WebUI 是查看和管理会话的面板，不会替代编辑器或 Git 工作流；代码、命令和文件始终保留在你的本机。

## 要求与安全

- 必须先安装并登录 Grok CLI。
- Grok 及其工具使用当前用户权限运行。启用自动批准前，请确认任务和提示词可信。
- WebUI 没有登录认证，只适合绑定本机回环地址，不要暴露到公网网卡。
- 不要把密码、Token 或其他秘密放入提示词、会话标题或会显示在面板中的环境变量。
- 请从可信来源下载 Release，并使用提供的 SHA-256 文件校验完整性。

## 项目链接

- [源代码](https://github.com/luodaoyi/grok-bridge-rs)
- [Releases](https://github.com/luodaoyi/grok-bridge-rs/releases)
- [Issues 和功能建议](https://github.com/luodaoyi/grok-bridge-rs/issues)
- [Linux DO 社区](https://linux.do/)
- [MIT License](LICENSE)
