/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge 会话管理",
  "doc.description":
    "Grok Bridge 本机会话控制台：Codex 监督者与 Grok 子代理状态管理",

  "app.skipToSessions": "跳到会话列表",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "监督者与子代理控制台",
  "app.subtitle":
    "Codex 会话作为监督者，其启动的 Grok 会话作为子代理。终端输出通过 WebSocket 实时推送，关闭页面不会终止已有会话。",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "在 GitHub 打开 grok-bridge-rs",
  "app.github": "GitHub",

  "connection.initial": "实时通道连接中",
  "connection.connected": "实时通道已连接",
  "connection.disconnected": "实时通道已断开",
  "connection.retrying": "实时通道重连中",
  "connection.reconnect": "重新连接",
  "connection.reconnectAria": "手动重连实时通道",

  "header.expandAll": "全部展开",
  "header.collapseAll": "全部折叠",

  "lang.label": "语言",
  "lang.aria": "界面语言",

  "theme.aria": "颜色主题",
  "theme.auto": "自动",
  "theme.light": "浅色",
  "theme.dark": "深色",
  "theme.title": "{label}主题",

  "stats.aria": "会话统计",
  "stats.owners": "监督者（Codex）",
  "stats.sessions": "子代理（Grok）",
  "stats.working": "工作中",
  "stats.waiting": "等待输入",
  "stats.done": "完成 / 空闲",

  "stream.initial": "实时通道连接中…",
  "stream.retrying": "实时通道重连中…",
  "stream.disconnected": "实时通道已断开，等待重连…",
  "stream.updated": "实时更新：{time}",
  "stream.waitingData": "实时通道已连接，等待会话数据…",
  "stream.pushMode": "推送：WebSocket · /api/events",
  "stream.error": "实时通道异常：{detail}（将自动重试）",

  "connecting.title": "正在连接实时通道",
  "connecting.body":
    "正在通过 WebSocket 连接本机 Runtime；收到首份会话数据后，页面会立即更新。",

  "empty.title": "暂无 Grok 会话",
  "empty.body":
    "Codex 启动的 Grok 会话会自动以子代理形式出现在这里，同时显示终端输出和生命周期状态。",

  "board.aria": "监督者分组",

  "update.title": "发现新版本 v{version}",
  "update.body":
    "当前 Runtime 为 v{current}。请手动下载并替换本地二进制，重启后生效。",
  "update.openRelease": "打开最新 Release",
  "update.dismiss": "稍后提醒",

  "error.renderTitle": "页面渲染异常",
  "error.reload": "重新加载",
  "error.unknown": "未知错误",
  "error.timeout": "请求超时，正在自动重试",
  "error.brand": "GROK BRIDGE",

  "activity.working": "工作中",
  "activity.waiting": "等待输入",
  "activity.done": "已完成",
  "activity.stopped": "已退出",
  "activity.unknown": "状态未知",

  "client.unmanaged": "未跟踪",
  "client.connected": "Codex 在线",
  "client.disconnected": "Codex 已断开",
  "client.orphaned": "等待自动关闭",
  "client.closing": "正在关闭",
  "client.unknown": "未知",

  "lifecycle.unmanaged": "未托管",
  "lifecycle.connected": "监督者在线",
  "lifecycle.disconnected": "监督者已断开",
  "lifecycle.orphaned": "自动关闭倒计时",
  "lifecycle.closing": "关闭中",
  "lifecycle.unknown": "状态未知",

  "badge.phase": "PTY 阶段：{phase}",

  "group.supervisor": "监督者 · Codex",
  "group.unowned": "未标记的 Codex 会话",
  "group.subagentCount": "{n} 个子代理",
  "group.closeAll": "关闭该 Codex 下的全部 Grok 会话",
  "group.closeAllAria": "关闭监督者 {owner} 下的全部 Grok 会话",
  "group.summary.working": "工作中 {n}",
  "group.summary.waiting": "等待输入 {n}",
  "group.summary.done": "完成或空闲 {n}",
  "group.summary.sep": " · ",
  "group.summary.none": "暂无活动状态",
  "group.idPrefix": "id {id}",

  "session.subagent": "子代理",
  "session.close": "关闭 Grok",
  "session.closeAria": "关闭子代理 {id}",
  "session.waitingCollapsed": "等待：{reason}",
  "session.waitingNote": "等待 Codex：{reason}",
  "session.meta.id": "会话 ID",
  "session.meta.pid": "进程",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "最近更新",
  "session.meta.client": "Codex 连接",
  "session.meta.autoClose": "自动关闭倒计时",
  "session.meta.hook": "最近 Hook",
  "session.meta.tool": "当前工具",
  "session.meta.cwd": "工作目录",
  "session.terminalAria": "子代理 {title} 的终端画面",

  "session.lifecycle.disconnectedTitle": "监督者连接已断开",
  "session.lifecycle.disconnectedBody":
    "会话处于工作中或等待输入状态时不会自动关闭；进入空闲或结束状态后，才会开始自动关闭倒计时。",
  "session.lifecycle.orphanedTitle": "自动关闭倒计时",
  "session.lifecycle.orphanedCountdown": "预计 {remaining} 后自动关闭",
  "session.lifecycle.orphanedCountdownDue":
    "已到自动关闭时间，等待 Runtime 处理",
  "session.lifecycle.orphanedAt":
    "预计自动关闭时间：{at}；Runtime 会随后关闭会话",
  "session.lifecycle.orphanedNoDeadline":
    "会话正在等待自动关闭，但 Runtime 未提供预计时间。",
  "session.lifecycle.closingTitle": "正在关闭会话",
  "session.lifecycle.closingBody": "Runtime 正在关闭此会话。",
  "session.lifecycle.collapsedOrphaned": "预计 {remaining} 后自动关闭",
  "session.lifecycle.collapsedOrphanedDue": "等待 Runtime 关闭",
  "session.lifecycle.collapsedOrphanedUnknown": "等待自动关闭",
  "session.lifecycle.collapsedClosing": "正在关闭",

  "terminal.header": "终端 · 实时只读",
  "terminal.headerInteractive": "终端 · 可交互",
  "terminal.aria": "{id} 的终端画面",
  "terminal.resizeAria": "调整终端高度",
  "terminal.resizeTitle": "拖动以调整终端高度",
  "terminal.resizeHint": "使用方向键调整高度；Enter 恢复默认高度",
  "terminal.resizeValue": "{height} 像素",

  "interactive.label": "键盘输入",
  "interactive.on": "开",
  "interactive.off": "关",
  "interactive.aria": "开启或关闭所有终端的键盘输入",
  "interactive.offHint": "开启所有 Grok 终端的键盘输入",
  "interactive.warning": "交互模式已开启。按键操作和粘贴内容会发送到对应的 Grok 会话；终端尺寸会随显示区域自动调整。刷新页面后，交互模式会自动关闭。",
  "interactive.disconnected": "实时通道已断开；输入不会发送，也不会暂存。",
  "interactive.invalidPayload": "终端命令参数无效",
  "interactive.sendFailed": "无法通过实时通道发送终端命令",
  "interactive.error": "终端输入失败：{detail}",
  "interactive.unavailable": "终端输入不可用",
  "interactive.unavailableShort": "输入不可用",

  "action.confirmCloseSession": "确定要关闭会话 {id} 及其 Grok 进程吗？",
  "action.closedSession": "已关闭 Grok 会话 {id}，状态将通过实时通道更新。",
  "action.closeFailed": "关闭失败：{detail}",
  "action.confirmCloseGroup":
    "确定要关闭 Codex“{owner}”下的全部 {count} 个 Grok 会话吗？",
  "action.groupEmpty": "该 Codex 分组中已无可关闭的 Grok 会话。",
  "action.groupPartial":
    "已关闭 {closed}/{matched} 个会话；失败：{failures}",
  "action.groupClosed":
    "已关闭 Codex“{owner}”下的全部 {count} 个 Grok 会话，状态将通过实时通道更新。",
  "action.failureJoin": "、",
};
