/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge 会话管理",
  "doc.description":
    "Grok Bridge 本机会话控制台：Codex 监督者与 Grok 子代理状态管理",

  "app.skipToSessions": "跳到会话列表",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "监督者与子代理控制台",
  "app.subtitle":
    "每个 Codex 对话是监督者，其下的 Grok 会话是可折叠的持久子代理。终端通过 WebSocket 实时推送只读输出；关闭窗口不会影响 Runtime 中已持有的会话。",
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

  "connecting.title": "实时通道连接中",
  "connecting.body":
    "已建立到本机 Runtime 的 WebSocket 连接请求，首帧会话快照会立即渲染。",

  "empty.title": "暂无 Grok 会话",
  "empty.body":
    "新的 Codex 监督者调用会自动出现在这里；每个 Grok 会话作为持久子代理显示其终端与生命周期状态。",

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
  "client.orphaned": "等待自动清理",
  "client.closing": "正在清理",
  "client.unknown": "未知",

  "lifecycle.unmanaged": "未托管",
  "lifecycle.connected": "监督者在线",
  "lifecycle.disconnected": "监督者断开",
  "lifecycle.orphaned": "清理倒计时",
  "lifecycle.closing": "清理中",
  "lifecycle.unknown": "状态未知",

  "badge.phase": "PTY 阶段：{phase}",

  "group.supervisor": "监督者 · Codex",
  "group.unowned": "未标记的 Codex 对话",
  "group.subagentCount": "{n} 个子代理",
  "group.closeAll": "关闭该 Codex 全部 Grok",
  "group.closeAllAria": "关闭监督者 {owner} 下的全部 Grok 子代理",
  "group.summary.working": "{n} 个工作中",
  "group.summary.waiting": "{n} 个等待输入",
  "group.summary.done": "{n} 个完成/空闲",
  "group.summary.sep": " · ",
  "group.summary.none": "无可用状态",
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
  "session.meta.autoClose": "自动清理倒计时",
  "session.meta.hook": "最近 Hook",
  "session.meta.tool": "当前工具",
  "session.meta.cwd": "工作目录",
  "session.terminalAria": "子代理 {title} 的终端画面",

  "session.lifecycle.disconnectedTitle": "监督者已断开 — 尚未开始关闭",
  "session.lifecycle.disconnectedBody":
    "Running / Waiting 阶段不会自动关闭；进入安全的 Idle 或终态后才开始宽限期。",
  "session.lifecycle.orphanedTitle": "自动关闭倒计时",
  "session.lifecycle.orphanedCountdown": "将在 {remaining} 后具备清理资格",
  "session.lifecycle.orphanedCountdownDue":
    "已过清理资格截止时间 — 等待 Runtime 清理",
  "session.lifecycle.orphanedAt":
    "本地清理资格截止时间 {at}；Runtime 会在截止后短时间内执行清理",
  "session.lifecycle.orphanedNoDeadline":
    "已进入孤儿状态；Runtime 未上报清理资格截止时间。",
  "session.lifecycle.closingTitle": "正在关闭会话",
  "session.lifecycle.closingBody": "此受管会话正在关闭。",
  "session.lifecycle.collapsedOrphaned": "{remaining} 后具备清理资格",
  "session.lifecycle.collapsedOrphanedDue": "等待 Runtime 清理",
  "session.lifecycle.collapsedOrphanedUnknown": "孤儿状态 — 等待自动关闭",
  "session.lifecycle.collapsedClosing": "正在关闭",

  "terminal.header": "终端 · 只读实时",
  "terminal.headerInteractive": "终端 · 可交互",
  "terminal.aria": "{id} 的终端画面",
  "terminal.resizeAria": "调整终端高度",
  "terminal.resizeTitle": "拖动以调整终端高度",
  "terminal.resizeHint": "使用方向键调整高度；Enter 恢复默认高度",
  "terminal.resizeValue": "{height} 像素",

  "interactive.label": "键盘输入",
  "interactive.on": "开",
  "interactive.off": "关",
  "interactive.aria": "切换全部终端的交互键盘输入",
  "interactive.offHint": "开启后可向所有 Grok 终端发送键盘输入",
  "interactive.warning": "交互模式已开启：按键和粘贴会发送到实时 Grok 会话。终端尺寸始终跟随可见视口同步。刷新页面会恢复键盘只读。",
  "interactive.disconnected": "实时通道已断开；输入不会发送，也不会缓存。",
  "interactive.invalidPayload": "终端命令参数无效",
  "interactive.sendFailed": "无法通过实时通道发送终端命令",
  "interactive.error": "终端输入失败：{detail}",
  "interactive.unavailable": "终端输入不可用",
  "interactive.unavailableShort": "输入离线",

  "action.confirmCloseSession": "确认关闭 {id} 及其 Grok 进程？",
  "action.closedSession": "已关闭 Grok 会话 {id}。实时通道将推送更新。",
  "action.closeFailed": "关闭失败：{detail}",
  "action.confirmCloseGroup":
    "确认关闭 Codex“{owner}”下的全部 {count} 个 Grok 会话？",
  "action.groupEmpty": "该 Codex 分组已没有活跃 Grok 会话。",
  "action.groupPartial":
    "已关闭 {closed}/{matched} 个会话；失败：{failures}",
  "action.groupClosed":
    "已关闭 Codex“{owner}”下的全部 {count} 个 Grok 会话。实时通道将推送更新。",
  "action.failureJoin": "、",
};
