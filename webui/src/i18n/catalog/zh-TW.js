/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge 工作階段管理",
  "doc.description":
    "Grok Bridge 本機工作階段主控台：Codex 監督者與 Grok 子代理狀態管理",

  "app.skipToSessions": "跳到工作階段列表",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "監督者與子代理主控台",
  "app.subtitle":
    "每個 Codex 對話是監督者，其下的 Grok 工作階段是可摺疊的持久子代理。終端透過 WebSocket 即時推送唯讀輸出；關閉視窗不會影響 Runtime 中已持有的工作階段。",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "在 GitHub 開啟 grok-bridge-rs",
  "app.github": "GitHub",

  "connection.initial": "正在連線即時通道",
  "connection.connected": "即時通道已連線",
  "connection.disconnected": "即時通道已中斷",
  "connection.retrying": "即時通道重新連線中",
  "connection.reconnect": "重新連線",
  "connection.reconnectAria": "手動重新連線即時通道",

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

  "stream.initial": "正在連線即時通道…",
  "stream.retrying": "即時通道重新連線中…",
  "stream.disconnected": "即時通道已中斷，等待重新連線…",
  "stream.updated": "即時更新：{time}",
  "stream.waitingData": "即時通道已連線，等待工作階段資料…",
  "stream.pushMode": "推送：WebSocket · /api/events",
  "stream.error": "即時通道異常：{detail}（將自動重試）",

  "connecting.title": "正在連線即時通道",
  "connecting.body":
    "已建立到本機 Runtime 的 WebSocket 連線請求，首幀工作階段快照會立即渲染。",

  "empty.title": "暫無 Grok 工作階段",
  "empty.body":
    "新的 Codex 監督者呼叫會自動出現在這裡；每個 Grok 工作階段作為持久子代理顯示其終端與生命週期狀態。",

  "board.aria": "監督者分組",

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
  "client.connected": "Codex 在線",
  "client.disconnected": "Codex 已中斷",
  "client.orphaned": "等待自動清理",
  "client.closing": "正在清理",
  "client.unknown": "未知",

  "lifecycle.unmanaged": "未託管",
  "lifecycle.connected": "監督者在線",
  "lifecycle.disconnected": "監督者中斷",
  "lifecycle.orphaned": "清理倒數",
  "lifecycle.closing": "清理中",
  "lifecycle.unknown": "狀態未知",

  "badge.phase": "PTY 階段：{phase}",

  "group.supervisor": "監督者 · Codex",
  "group.unowned": "未標記的 Codex 對話",
  "group.subagentCount": "{n} 個子代理",
  "group.closeAll": "關閉該 Codex 全部 Grok",
  "group.closeAllAria": "關閉監督者 {owner} 下的全部 Grok 子代理",
  "group.summary.working": "{n} 個工作中",
  "group.summary.waiting": "{n} 個等待輸入",
  "group.summary.done": "{n} 個完成/閒置",
  "group.summary.sep": " · ",
  "group.summary.none": "無可用狀態",
  "group.idPrefix": "id {id}",

  "session.subagent": "子代理",
  "session.close": "關閉 Grok",
  "session.closeAria": "關閉子代理 {id}",
  "session.waitingCollapsed": "等待：{reason}",
  "session.waitingNote": "等待 Codex：{reason}",
  "session.meta.id": "工作階段 ID",
  "session.meta.pid": "程序",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "最近更新",
  "session.meta.client": "Codex 連線",
  "session.meta.autoClose": "自動清理倒數",
  "session.meta.hook": "最近 Hook",
  "session.meta.tool": "目前工具",
  "session.meta.cwd": "工作目錄",
  "session.terminalAria": "子代理 {title} 的終端畫面",

  "session.lifecycle.disconnectedTitle": "監督者已中斷 — 尚未開始關閉",
  "session.lifecycle.disconnectedBody":
    "Running / Waiting 階段不會自動關閉；進入安全的 Idle 或終態後才開始寬限期。",
  "session.lifecycle.orphanedTitle": "自動關閉倒數",
  "session.lifecycle.orphanedCountdown": "將在 {remaining} 後具備清理資格",
  "session.lifecycle.orphanedCountdownDue":
    "已過清理資格截止時間 — 等待 Runtime 清理",
  "session.lifecycle.orphanedAt":
    "本機清理資格截止時間 {at}；Runtime 會在截止後短時間內執行清理",
  "session.lifecycle.orphanedNoDeadline":
    "已進入孤兒狀態；Runtime 未回報清理資格截止時間。",
  "session.lifecycle.closingTitle": "正在關閉工作階段",
  "session.lifecycle.closingBody": "此受管工作階段正在關閉。",
  "session.lifecycle.collapsedOrphaned": "{remaining} 後具備清理資格",
  "session.lifecycle.collapsedOrphanedDue": "等待 Runtime 清理",
  "session.lifecycle.collapsedOrphanedUnknown": "孤兒狀態 — 等待自動關閉",
  "session.lifecycle.collapsedClosing": "正在關閉",

  "terminal.header": "終端 · 唯讀即時",
  "terminal.headerInteractive": "終端 · 可互動",
  "terminal.aria": "{id} 的終端畫面",
  "terminal.resizeAria": "調整終端高度",
  "terminal.resizeTitle": "拖曳以調整終端高度",
  "terminal.resizeHint": "使用方向鍵調整高度；Enter 還原預設高度",
  "terminal.resizeValue": "{height} 像素",

  "interactive.label": "鍵盤輸入",
  "interactive.on": "開",
  "interactive.off": "關",
  "interactive.aria": "切換全部終端的互動鍵盤輸入",
  "interactive.offHint": "開啟後可向所有 Grok 終端傳送鍵盤輸入",
  "interactive.warning": "互動模式已開啟：按鍵與貼上會傳送到即時 Grok 工作階段。終端尺寸一律跟隨可見視口同步。重新整理頁面會恢復鍵盤唯讀。",
  "interactive.disconnected": "即時通道已中斷；輸入不會傳送，也不會快取。",
  "interactive.invalidPayload": "終端命令參數無效",
  "interactive.sendFailed": "無法透過即時通道傳送終端命令",
  "interactive.error": "終端輸入失敗：{detail}",
  "interactive.unavailable": "終端輸入不可用",
  "interactive.unavailableShort": "輸入離線",

  "action.confirmCloseSession": "確認關閉 {id} 及其 Grok 程序？",
  "action.closedSession": "已關閉 Grok 工作階段 {id}。即時通道將推送更新。",
  "action.closeFailed": "關閉失敗：{detail}",
  "action.confirmCloseGroup":
    "確認關閉 Codex「{owner}」下的全部 {count} 個 Grok 工作階段？",
  "action.groupEmpty": "該 Codex 分組已沒有作用中的 Grok 工作階段。",
  "action.groupPartial":
    "已關閉 {closed}/{matched} 個工作階段；失敗：{failures}",
  "action.groupClosed":
    "已關閉 Codex「{owner}」下的全部 {count} 個 Grok 工作階段。即時通道將推送更新。",
  "action.failureJoin": "、",
};
