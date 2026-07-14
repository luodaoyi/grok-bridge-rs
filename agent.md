# Codex 使用 grok-build Skill 的编排规则

将本文件的规则合并到业务项目 `AGENTS.md`，或在 Codex 会话开始时发送。`grok-build` 负责启动并监控 Grok；Codex 始终负责规划、审计和最终验收。

## 工作流程

1. 理解需求，检查现有工作树，并列出可验证的验收标准。
2. 把目标、约束、相关文件和验收标准整理成完整 prompt，通过 `$grok-build` 的 `start` 启动任务。只有可信仓库才设置 `auto_approve: true`。
3. 保存 wrapper `handle`。用 `read --cursor` 增量读取事件，用 `wait --for tui-idle` 等待成功空闲；长任务使用有限超时的滚动等待，不要盲目重复启动。
4. Grok 执行时，Codex 不得并发修改相同文件。需要中止时调用 `stop`。
5. 返回 `idle` 后，独立执行 `git status --short`、`git diff --check`、`git diff` 以及项目要求的测试、lint、格式化和构建。
6. 审计失败时，先读取剩余事件，再用同一 handle 的 `send` 提交只针对证据和根因的返工 prompt。wrapper 自动恢复 Grok UUID。
7. 最多自动返工五轮；仍未通过则停止并报告根因、改动和未完成项。
8. 最终审计完成且不再续轮后，调用 `remove --session <handle>` 清理本地会话状态和事件。
9. 不得删除测试、降低安全检查、吞掉异常，也不得自动 commit、push、merge、创建 PR 或执行不可逆操作。

Windows PowerShell 5.1 调用 `start` 或 `send` 前必须设置：

```powershell
$utf8 = New-Object System.Text.UTF8Encoding($false)
$OutputEncoding = $utf8
[Console]::OutputEncoding = $utf8
```

否则中文会在进入 wrapper 前变成问号。

## Prompt 模板

```text
你是本任务的具体实现者，请直接检查并修改当前项目。

目标：
<用户目标>

验收标准：
1. <可验证标准>
2. <可验证标准>

约束：
- 只修改与任务直接相关的文件，保留现有文件编码。
- 保持公开接口兼容，除非目标明确要求改变。
- 增加或更新必要测试，并运行相关检查。
- 不要 commit、push 或创建 PR。
- 完成时报告修改文件、测试结果和未解决风险。
```
