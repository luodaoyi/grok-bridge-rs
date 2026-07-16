# grok-build Local Runtime Skill

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

grok-build gives Codex a reliable local way to use Grok for real software work without creating a new unmanaged process for every request. Its shared runtime keeps sessions visible, interactive, grouped by Codex conversation, and easy to close from a browser.

## Why Use It?

When Codex hands work to Grok, you should not have to guess which process belongs to which task or whether it is still doing useful work. grok-build keeps that workflow in one place:

- **Local-first:** no Python, MCP server, cloud relay, or separate service installation is required.
- **One runtime:** sessions are created and kept by one local process, so repeated Codex requests do not leave unmanaged bridges behind.
- **Live control:** Grok remains a real interactive terminal. You can inspect output, send another instruction, attach a terminal, or stop it.
- **Clear ownership:** sessions are grouped by the Codex conversation and its short title, even when several Codex conversations run at once.
- **Browser management:** `server ui` opens a local panel with status, recent activity, terminal output, working directory, and close actions.
- **Useful cleanup:** Hooks report Grok activity, while heartbeat and lease handling help identify disconnected sessions. Close one session or everything owned by a Codex conversation.
- **Room for concurrent work:** there is no artificial 64-session cap. Practical capacity depends on your machine and Grok CLI.

## Quick Start

### 1. Install Grok CLI

Install and sign in to Grok CLI first. Confirm that it is available in your terminal:

```text
grok --version
```

### 2. Install the Skill

Download the latest ZIP from [GitHub Releases](https://github.com/luodaoyi/grok-bridge-rs/releases), then extract it into your Agent Skills directory. The extracted folder should be `grok-build/`.

The ZIP includes native binaries for Windows x86_64/ARM64, Linux x86_64/ARM64, and macOS Intel/Apple Silicon. Choose the binary that matches your system.

### 3. Check and open the panel

Windows PowerShell:

```powershell
$bridge = "$env:USERPROFILE\.agents\skills\grok-build\bin\windows-x86_64\grok-bridge.exe"
& $bridge doctor
& $bridge hooks install
& $bridge server ui
```

Linux or macOS:

```sh
bridge="$HOME/.agents/skills/grok-build/bin/linux-x86_64/grok-bridge"
"$bridge" doctor
"$bridge" hooks install
"$bridge" server ui
```

Replace the binary directory with `windows-arm64`, `linux-arm64`, `macos-x86_64`, or `macos-arm64` when needed. `server ui` starts the local runtime on demand and opens the browser panel. The default address is `http://127.0.0.1:47653`.

After the Skill is installed, Codex can use it directly. The panel is there when you want to see what every Grok session is doing or take manual control.

## Everyday Use

1. Ask Codex to delegate a coding task to Grok. The Skill records a short, readable title for the Codex conversation.
2. Open `server ui` to see all Codex groups, expand the one you need, and read the current terminal screen and activity.
3. Let Codex continue normally, or send a follow-up instruction from the CLI or attached terminal.
4. Close an individual Grok session, close all sessions for one Codex conversation with `close-codex`, or stop the complete runtime when you are done.

The WebUI is an inspection and control surface, not a replacement for your editor or Git workflow. Your files and commands remain on your machine.

## Requirements and Safety

- Grok CLI must already be installed and authenticated.
- Grok and its tools run with your current user permissions. Review prompts before enabling automatic approval.
- The WebUI has no login and is intended for the local loopback address only. Do not expose it on a public interface.
- Do not put passwords, tokens, or other secrets in prompts, session titles, or environment variables shown in the panel.
- Download releases from a trusted source and verify the provided SHA-256 file.

## Project Links

- [Source code](https://github.com/luodaoyi/grok-bridge-rs)
- [Releases](https://github.com/luodaoyi/grok-bridge-rs/releases)
- [Issues and feature requests](https://github.com/luodaoyi/grok-bridge-rs/issues)
- [Linux DO community](https://linux.do/)
- [MIT License](LICENSE)
