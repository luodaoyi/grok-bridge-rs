# Changelog

## Unreleased

- Converted the repository root into a directly clonable `grok-build` Agent Skill.
- Replaced server registration with a single-request STDIN/STDOUT JSON wrapper flow.
- Added Grok session UUID extraction and follow-up continuation through `--resume`.
- Added a five-platform Release package layout for Windows ARM64/x86_64, macOS ARM64, and Linux ARM64/x86_64.
- Added Tag-triggered GitHub Actions builds that assemble one installable Skill ZIP and publish it with a SHA-256 checksum.
- Removed installation scripts and repository-managed precompiled binaries; users install by extracting the Release ZIP.

## 0.1.0 - 2026-07-14

- Initial Rust bridge with timeout, output capture, prompt redaction, and optional allowed roots.
