/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge セッション",
  "doc.description":
    "Grok Bridge ローカルセッションコンソール：Codex スーパーバイザーと Grok サブエージェント管理",

  "app.skipToSessions": "セッション一覧へスキップ",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "スーパーバイザー／サブエージェント コンソール",
  "app.subtitle":
    "各 Codex セッションはスーパーバイザーとして機能し、そこから起動された Grok セッションがサブエージェントとなります。ターミナル出力は WebSocket 経由でリアルタイムに配信されます。ページを閉じても既存のセッションは終了しません。",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "GitHub で grok-bridge-rs を開く",
  "app.github": "GitHub",

  "connection.initial": "ライブチャネル接続中",
  "connection.connected": "ライブチャネル接続済み",
  "connection.disconnected": "ライブチャネル切断",
  "connection.retrying": "ライブチャネル再接続中",
  "connection.reconnect": "再接続",
  "connection.reconnectAria": "ライブチャネルを手動で再接続",

  "header.expandAll": "すべて展開",
  "header.collapseAll": "すべて折りたたむ",

  "lang.label": "言語",
  "lang.aria": "表示言語",

  "theme.aria": "カラーテーマ",
  "theme.auto": "自動",
  "theme.light": "ライト",
  "theme.dark": "ダーク",
  "theme.title": "{label}テーマ",

  "stats.aria": "セッション統計",
  "stats.owners": "スーパーバイザー（Codex）",
  "stats.sessions": "サブエージェント（Grok）",
  "stats.working": "作業中",
  "stats.waiting": "入力待ち",
  "stats.done": "完了 / アイドル",

  "stream.initial": "ライブチャネル接続中…",
  "stream.retrying": "ライブチャネル再接続中…",
  "stream.disconnected": "ライブチャネル切断、再接続待機中…",
  "stream.updated": "ライブ更新：{time}",
  "stream.waitingData": "ライブチャネル接続済み、セッションデータ待機中…",
  "stream.pushMode": "プッシュ：WebSocket · /api/events",
  "stream.error": "ライブチャネル異常：{detail}（自動で再試行します）",

  "connecting.title": "ライブチャネル接続中",
  "connecting.body":
    "ローカル Runtime への WebSocket 接続を確立中です。最初のセッションスナップショットはすぐに表示されます。",

  "empty.title": "Grok セッションはありません",
  "empty.body":
    "Codex が起動した Grok セッションは、サブエージェントとしてここに自動表示され、端末出力とライフサイクル状態が示されます。",

  "board.aria": "スーパーバイザーグループ",

  "update.title": "新しいバージョンがあります：v{version}",
  "update.body":
    "現在の Runtime は v{current} です。ローカルバイナリを手動でダウンロードして置き換え、再起動後に反映されます。",
  "update.openRelease": "最新 Release を開く",
  "update.dismiss": "後で通知",

  "error.renderTitle": "ページ描画エラー",
  "error.reload": "再読み込み",
  "error.unknown": "不明なエラー",
  "error.timeout": "リクエストがタイムアウトしました。自動で再試行します",
  "error.brand": "GROK BRIDGE",

  "activity.working": "作業中",
  "activity.waiting": "入力待ち",
  "activity.done": "完了",
  "activity.stopped": "終了",
  "activity.unknown": "不明",

  "client.unmanaged": "未追跡",
  "client.connected": "Codex オンライン",
  "client.disconnected": "Codex 切断",
  "client.orphaned": "自動クリーンアップ待ち",
  "client.closing": "クリーンアップ中",
  "client.unknown": "不明",

  "lifecycle.unmanaged": "未管理",
  "lifecycle.connected": "スーパーバイザー オンライン",
  "lifecycle.disconnected": "スーパーバイザー オフライン",
  "lifecycle.orphaned": "クリーンアップ カウントダウン",
  "lifecycle.closing": "クリーンアップ中",
  "lifecycle.unknown": "不明",

  "badge.phase": "PTY フェーズ：{phase}",

  "group.supervisor": "スーパーバイザー · Codex",
  "group.unowned": "ラベルなしの Codex 会話",
  "group.subagentCount": "{n} 個のサブエージェント",
  "group.closeAll": "この Codex の全 Grok を閉じる",
  "group.closeAllAria":
    "スーパーバイザー {owner} 下のすべての Grok サブエージェントを閉じる",
  "group.summary.working": "{n} 作業中",
  "group.summary.waiting": "{n} 入力待ち",
  "group.summary.done": "{n} 完了/アイドル",
  "group.summary.sep": " · ",
  "group.summary.none": "ステータスなし",
  "group.idPrefix": "id {id}",

  "session.subagent": "サブエージェント",
  "session.close": "Grok を閉じる",
  "session.closeAria": "サブエージェント {id} を閉じる",
  "session.waitingCollapsed": "待機：{reason}",
  "session.waitingNote": "Codex 待機中：{reason}",
  "session.meta.id": "セッション ID",
  "session.meta.pid": "プロセス",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "最終更新",
  "session.meta.client": "Codex 接続",
  "session.meta.autoClose": "自動クリーンアップ カウントダウン",
  "session.meta.hook": "最新 Hook",
  "session.meta.tool": "現在のツール",
  "session.meta.cwd": "作業ディレクトリ",
  "session.terminalAria": "サブエージェント {title} の端末",

  "session.lifecycle.disconnectedTitle":
    "スーパーバイザー切断 — まだ終了しません",
  "session.lifecycle.disconnectedBody":
    "Running / Waiting 段階は自動終了しません。安全な Idle または終了状態になってから猶予が始まります。",
  "session.lifecycle.orphanedTitle": "自動終了カウントダウン",
  "session.lifecycle.orphanedCountdown":
    "{remaining} 後にクリーンアップ対象になります",
  "session.lifecycle.orphanedCountdownDue":
    "対象期限を過ぎました — Runtime のクリーンアップ待ち",
  "session.lifecycle.orphanedAt":
    "ローカルのクリーンアップ対象期限 {at}；Runtime はその後まもなく実行します",
  "session.lifecycle.orphanedNoDeadline":
    "孤立状態です。Runtime がクリーンアップ対象期限を報告していません。",
  "session.lifecycle.closingTitle": "セッションを終了中",
  "session.lifecycle.closingBody":
    "この管理対象セッションは現在終了処理中です。",
  "session.lifecycle.collapsedOrphaned":
    "{remaining} 後にクリーンアップ対象",
  "session.lifecycle.collapsedOrphanedDue": "Runtime クリーンアップ待ち",
  "session.lifecycle.collapsedOrphanedUnknown": "孤立 — 自動終了待ち",
  "session.lifecycle.collapsedClosing": "終了中",

  "terminal.header": "端末 · 読み取り専用ライブ",
  "terminal.headerInteractive": "端末 · 対話",
  "terminal.aria": "{id} の端末",
  "terminal.resizeAria": "端末の高さを変更",
  "terminal.resizeTitle": "ドラッグして端末の高さを変更",
  "terminal.resizeHint": "矢印キーで高さ変更、Enter で既定の高さに戻す",
  "terminal.resizeValue": "{height} ピクセル",

  "interactive.label": "キーボード",
  "interactive.on": "オン",
  "interactive.off": "オフ",
  "interactive.aria": "すべての端末の対話キーボード入力を切り替え",
  "interactive.offHint": "すべての Grok 端末へのキーボード入力を有効化",
  "interactive.warning": "対話モードがオンです。キー入力と貼り付けは稼働中の Grok セッションに送られます。端末サイズは常に表示領域に追従します。再読み込みでキーボードは読み取り専用に戻ります。",
  "interactive.disconnected": "ライブチャネル切断中。入力は送信されず、バッファにも残りません。",
  "interactive.invalidPayload": "端末コマンドのペイロードが無効です",
  "interactive.sendFailed": "ライブチャネル経由で端末コマンドを送信できませんでした",
  "interactive.error": "端末入力に失敗：{detail}",
  "interactive.unavailable": "端末入力を利用できません",
  "interactive.unavailableShort": "入力オフライン",

  "action.confirmCloseSession": "{id} とその Grok プロセスを閉じますか？",
  "action.closedSession":
    "Grok セッション {id} を閉じました。ライブチャネルが更新をプッシュします。",
  "action.closeFailed": "クローズに失敗：{detail}",
  "action.confirmCloseGroup":
    "Codex「{owner}」下の {count} 個の Grok セッションをすべて閉じますか？",
  "action.groupEmpty":
    "この Codex グループにアクティブな Grok セッションはありません。",
  "action.groupPartial":
    "{closed}/{matched} セッションを閉じました。失敗：{failures}",
  "action.groupClosed":
    "Codex「{owner}」下の {count} 個の Grok セッションをすべて閉じました。ライブチャネルが更新をプッシュします。",
  "action.failureJoin": "、",
};
