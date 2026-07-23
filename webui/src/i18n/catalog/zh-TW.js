/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge 工作階段管理",
  "doc.description":
    "Grok Bridge 本機工作階段主控台：Codex 監督者與 Grok 子代理狀態管理",

  "app.skipToSessions": "跳到工作階段列表",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "監督者與子代理主控台",
  "app.subtitle":
    "Codex 工作階段作為監督者，其啟動的 Grok 工作階段作為子代理。終端輸出透過 WebSocket 即時推送，關閉頁面不會終止現有工作階段。",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "在 GitHub 開啟 grok-bridge-rs",
  "app.github": "GitHub",

  "connection.initial": "正在連線至即時通道",
  "connection.connected": "即時通道已連線",
  "connection.disconnected": "即時通道已中斷",
  "connection.retrying": "即時通道重新連線中",
  "connection.reconnect": "重新連線",
  "connection.reconnectAria": "手動重新連線至即時通道",

  "header.expandAll": "全部展開",
  "header.collapseAll": "全部摺疊",

  "lang.label": "語言",
  "lang.aria": "介面語言",

  "theme.aria": "色彩主題",
  "theme.auto": "自動",
  "theme.light": "淺色",
  "theme.dark": "深色",
  "theme.title": "{label}主題",

  "stats.aria": "工作階段統計",
  "stats.owners": "監督者（Codex）",
  "stats.sessions": "子代理（Grok）",
  "stats.working": "工作中",
  "stats.waiting": "等待輸入",
  "stats.done": "完成 / 閒置",

  "stream.initial": "正在連線至即時通道…",
  "stream.retrying": "即時通道重新連線中…",
  "stream.disconnected": "即時通道已中斷，等待重新連線…",
  "stream.updated": "即時更新：{time}",
  "stream.waitingData": "即時通道已連線，等待工作階段資料…",
  "stream.pushMode": "推送：WebSocket · /api/events",
  "stream.error": "即時通道異常：{detail}（將自動重試）",

  "connecting.title": "正在連線至即時通道",
  "connecting.body":
    "正在透過 WebSocket 連線至本機 Runtime；收到第一份工作階段資料後，頁面會立即更新。",

  "empty.title": "目前沒有 Grok 工作階段",
  "empty.body":
    "Codex 啟動的 Grok 工作階段會自動以子代理形式出現在這裡，同時顯示終端輸出與生命週期狀態。",

  "board.aria": "監督者群組",

  "update.title": "發現新版本 v{version}",
  "update.body":
    "目前 Runtime 為 v{current}。請手動下載並取代本機二進位檔，重新啟動後生效。",
  "update.openRelease": "開啟最新 Release",
  "update.dismiss": "稍後提醒",

  "error.renderTitle": "頁面渲染異常",
  "error.reload": "重新載入",
  "error.unknown": "未知錯誤",
  "error.timeout": "請求逾時，正在自動重試",
  "error.brand": "GROK BRIDGE",

  "activity.working": "工作中",
  "activity.waiting": "等待輸入",
  "activity.done": "已完成",
  "activity.stopped": "已退出",
  "activity.unknown": "狀態未知",

  "client.unmanaged": "未追蹤",
  "client.connected": "Codex 已連線",
  "client.disconnected": "Codex 連線中斷",
  "client.orphaned": "等待自動關閉",
  "client.closing": "正在關閉",
  "client.unknown": "未知",

  "lifecycle.unmanaged": "未託管",
  "lifecycle.connected": "監督者已連線",
  "lifecycle.disconnected": "監督者連線中斷",
  "lifecycle.orphaned": "自動關閉倒數",
  "lifecycle.closing": "關閉中",
  "lifecycle.unknown": "狀態未知",

  "badge.phase": "PTY 階段：{phase}",

  "group.supervisor": "監督者 · Codex",
  "group.unowned": "未標記的 Codex 工作階段",
  "group.subagentCount": "{n} 個子代理",
  "group.closeAll": "關閉該 Codex 下的全部 Grok 工作階段",
  "group.closeAllAria": "關閉監督者 {owner} 下的全部 Grok 工作階段",
  "group.summary.working": "工作中 {n}",
  "group.summary.waiting": "等待輸入 {n}",
  "group.summary.done": "完成或閒置 {n}",
  "group.summary.sep": " · ",
  "group.summary.none": "目前沒有活動狀態",
  "group.idPrefix": "id {id}",

  "session.subagent": "子代理",
  "session.close": "關閉 Grok",
  "session.closeAria": "關閉子代理 {id}",
  "session.waitingCollapsed": "等待：{reason}",
  "session.waitingNote": "等待 Codex：{reason}",
  "session.meta.id": "工作階段 ID",
  "session.meta.pid": "處理程序",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "最近更新",
  "session.meta.client": "Codex 連線",
  "session.meta.autoClose": "自動關閉倒數",
  "session.meta.hook": "最近 Hook",
  "session.meta.tool": "目前工具",
  "session.meta.cwd": "工作目錄",
  "session.terminalAria": "子代理 {title} 的終端畫面",

  "session.lifecycle.disconnectedTitle": "監督者連線已中斷",
  "session.lifecycle.disconnectedBody":
    "工作階段處於工作中或等待輸入狀態時不會自動關閉；進入閒置或結束狀態後，才會開始自動關閉倒數。",
  "session.lifecycle.orphanedTitle": "自動關閉倒數",
  "session.lifecycle.orphanedCountdown": "預計 {remaining} 後自動關閉",
  "session.lifecycle.orphanedCountdownDue":
    "已到自動關閉時間，等待 Runtime 處理",
  "session.lifecycle.orphanedAt":
    "預計自動關閉時間：{at}；Runtime 會隨後關閉工作階段",
  "session.lifecycle.orphanedNoDeadline":
    "工作階段正在等待自動關閉，但 Runtime 未提供預計時間。",
  "session.lifecycle.closingTitle": "正在關閉工作階段",
  "session.lifecycle.closingBody": "Runtime 正在關閉此工作階段。",
  "session.lifecycle.collapsedOrphaned": "預計 {remaining} 後自動關閉",
  "session.lifecycle.collapsedOrphanedDue": "等待 Runtime 關閉",
  "session.lifecycle.collapsedOrphanedUnknown": "等待自動關閉",
  "session.lifecycle.collapsedClosing": "正在關閉",

  "terminal.header": "終端 · 即時唯讀",
  "terminal.headerInteractive": "終端 · 可互動",
  "terminal.aria": "{id} 的終端畫面",
  "terminal.resizeAria": "調整終端高度",
  "terminal.resizeTitle": "拖曳以調整終端高度",
  "terminal.resizeHint": "使用方向鍵調整高度；Enter 還原預設高度",
  "terminal.resizeValue": "{height} 像素",

  "interactive.label": "鍵盤輸入",
  "interactive.on": "開",
  "interactive.off": "關",
  "interactive.aria": "開啟或關閉所有終端的鍵盤輸入",
  "interactive.offHint": "開啟所有 Grok 終端的鍵盤輸入",
  "interactive.warning": "互動模式已開啟。按鍵操作與貼上內容會傳送到對應的 Grok 工作階段；終端尺寸會隨顯示區域自動調整。重新整理頁面後，互動模式會自動關閉。",
  "interactive.disconnected": "即時通道已中斷；輸入不會傳送，也不會暫存。",
  "interactive.invalidPayload": "終端命令參數無效",
  "interactive.sendFailed": "無法透過即時通道傳送終端命令",
  "interactive.error": "終端輸入失敗：{detail}",
  "interactive.unavailable": "終端輸入不可用",
  "interactive.unavailableShort": "輸入不可用",

  "action.confirmCloseSession": "確定要關閉工作階段 {id} 及其 Grok 處理程序嗎？",
  "action.closedSession": "已關閉 Grok 工作階段 {id}，狀態將透過即時通道更新。",
  "action.closeFailed": "關閉失敗：{detail}",
  "action.confirmCloseGroup":
    "確定要關閉 Codex「{owner}」下的全部 {count} 個 Grok 工作階段嗎？",
  "action.groupEmpty": "該 Codex 群組中已無可關閉的 Grok 工作階段。",
  "action.groupPartial":
    "已關閉 {closed}/{matched} 個工作階段；失敗：{failures}",
  "action.groupClosed":
    "已關閉 Codex「{owner}」下的全部 {count} 個 Grok 工作階段，狀態將透過即時通道更新。",
  "action.failureJoin": "、",
};
