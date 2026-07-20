/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge 세션",
  "doc.description":
    "Grok Bridge 로컬 세션 콘솔: Codex 슈퍼바이저 및 Grok 서브에이전트 관리",

  "app.skipToSessions": "세션 목록으로 건너뛰기",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "슈퍼바이저 및 서브에이전트 콘솔",
  "app.subtitle":
    "각 Codex 대화는 슈퍼바이저이며, 그 아래 Grok 세션은 접을 수 있는 영구 서브에이전트입니다. 터미널은 WebSocket으로 실시간 읽기 전용 출력을 받으며, 이 창을 닫아도 Runtime이 보유한 세션에는 영향이 없습니다.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "GitHub에서 grok-bridge-rs 열기",
  "app.github": "GitHub",

  "connection.initial": "실시간 채널 연결 중",
  "connection.connected": "실시간 채널 연결됨",
  "connection.disconnected": "실시간 채널 연결 끊김",
  "connection.retrying": "실시간 채널 재연결 중",
  "connection.reconnect": "다시 연결",
  "connection.reconnectAria": "실시간 채널 수동 재연결",

  "header.expandAll": "모두 펼치기",
  "header.collapseAll": "모두 접기",

  "lang.label": "언어",
  "lang.aria": "인터페이스 언어",

  "theme.aria": "색상 테마",
  "theme.auto": "자동",
  "theme.light": "라이트",
  "theme.dark": "다크",
  "theme.title": "{label} 테마",

  "stats.aria": "세션 통계",
  "stats.owners": "슈퍼바이저 (Codex)",
  "stats.sessions": "서브에이전트 (Grok)",
  "stats.working": "작업 중",
  "stats.waiting": "입력 대기",
  "stats.done": "완료 / 유휴",

  "stream.initial": "실시간 채널 연결 중…",
  "stream.retrying": "실시간 채널 재연결 중…",
  "stream.disconnected": "실시간 채널 연결 끊김, 재연결 대기 중…",
  "stream.updated": "실시간 업데이트: {time}",
  "stream.waitingData": "실시간 채널 연결됨, 세션 데이터 대기 중…",
  "stream.pushMode": "푸시: WebSocket · /api/events",
  "stream.error": "실시간 채널 오류: {detail} (자동으로 다시 시도)",

  "connecting.title": "실시간 채널 연결 중",
  "connecting.body":
    "로컬 Runtime에 대한 WebSocket 연결을 설정 중입니다. 첫 세션 스냅샷이 즉시 렌더링됩니다.",

  "empty.title": "Grok 세션 없음",
  "empty.body":
    "새 Codex 슈퍼바이저 호출이 여기에 자동으로 표시됩니다. 각 Grok 세션은 영구 서브에이전트로 터미널과 수명 주기를 보여 줍니다.",

  "board.aria": "슈퍼바이저 그룹",

  "update.title": "새 버전 사용 가능: v{version}",
  "update.body":
    "현재 Runtime은 v{current}입니다. 로컬 바이너리를 수동으로 다운로드해 교체한 뒤 다시 시작하세요.",
  "update.openRelease": "최신 Release 열기",
  "update.dismiss": "나중에 알림",

  "error.renderTitle": "페이지 렌더 오류",
  "error.reload": "다시 로드",
  "error.unknown": "알 수 없는 오류",
  "error.timeout": "요청 시간 초과, 자동으로 다시 시도 중",
  "error.brand": "GROK BRIDGE",

  "activity.working": "작업 중",
  "activity.waiting": "입력 대기",
  "activity.done": "완료",
  "activity.stopped": "종료됨",
  "activity.unknown": "알 수 없음",

  "client.unmanaged": "추적 안 됨",
  "client.connected": "Codex 온라인",
  "client.disconnected": "Codex 연결 끊김",
  "client.orphaned": "자동 정리 대기",
  "client.closing": "정리 중",
  "client.unknown": "알 수 없음",

  "lifecycle.unmanaged": "관리되지 않음",
  "lifecycle.connected": "슈퍼바이저 온라인",
  "lifecycle.disconnected": "슈퍼바이저 오프라인",
  "lifecycle.orphaned": "정리 카운트다운",
  "lifecycle.closing": "정리 중",
  "lifecycle.unknown": "알 수 없음",

  "badge.phase": "PTY 단계: {phase}",

  "group.supervisor": "슈퍼바이저 · Codex",
  "group.unowned": "레이블 없는 Codex 대화",
  "group.subagentCount": "서브에이전트 {n}개",
  "group.closeAll": "이 Codex의 모든 Grok 닫기",
  "group.closeAllAria":
    "슈퍼바이저 {owner} 아래의 모든 Grok 서브에이전트 닫기",
  "group.summary.working": "작업 중 {n}",
  "group.summary.waiting": "입력 대기 {n}",
  "group.summary.done": "완료/유휴 {n}",
  "group.summary.sep": " · ",
  "group.summary.none": "상태 없음",
  "group.idPrefix": "id {id}",

  "session.subagent": "서브에이전트",
  "session.close": "Grok 닫기",
  "session.closeAria": "서브에이전트 {id} 닫기",
  "session.waitingCollapsed": "대기: {reason}",
  "session.waitingNote": "Codex 대기 중: {reason}",
  "session.meta.id": "세션 ID",
  "session.meta.pid": "프로세스",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "최근 업데이트",
  "session.meta.client": "Codex 연결",
  "session.meta.autoClose": "자동 정리 카운트다운",
  "session.meta.hook": "최근 Hook",
  "session.meta.tool": "현재 도구",
  "session.meta.cwd": "작업 디렉터리",
  "session.terminalAria": "서브에이전트 {title} 터미널",

  "session.lifecycle.disconnectedTitle":
    "슈퍼바이저 오프라인 — 아직 종료하지 않음",
  "session.lifecycle.disconnectedBody":
    "Running / Waiting 단계는 자동 종료되지 않습니다. 안전한 Idle 또는 종료 상태 이후에만 유예가 시작됩니다.",
  "session.lifecycle.orphanedTitle": "자동 종료 카운트다운",
  "session.lifecycle.orphanedCountdown":
    "{remaining} 후 정리 대상이 됩니다",
  "session.lifecycle.orphanedCountdownDue":
    "정리 자격 기한 경과 — Runtime 정리 대기 중",
  "session.lifecycle.orphanedAt":
    "로컬 정리 자격 기한 {at}; Runtime이 기한 직후 정리합니다",
  "session.lifecycle.orphanedNoDeadline":
    "고아 상태입니다. Runtime이 정리 자격 기한을 보고하지 않았습니다.",
  "session.lifecycle.closingTitle": "세션 종료 중",
  "session.lifecycle.closingBody":
    "이 관리 세션을 지금 종료하고 있습니다.",
  "session.lifecycle.collapsedOrphaned": "{remaining} 후 정리 대상",
  "session.lifecycle.collapsedOrphanedDue": "Runtime 정리 대기 중",
  "session.lifecycle.collapsedOrphanedUnknown": "고아 — 자동 종료 대기",
  "session.lifecycle.collapsedClosing": "종료 중",

  "terminal.header": "터미널 · 읽기 전용 실시간",
  "terminal.headerInteractive": "터미널 · 대화형",
  "terminal.aria": "{id} 터미널",
  "terminal.resizeAria": "터미널 높이 조절",
  "terminal.resizeTitle": "드래그하여 터미널 높이 조절",
  "terminal.resizeHint": "화살표 키로 높이 조절, Enter로 기본 높이 복원",
  "terminal.resizeValue": "{height}픽셀",

  "interactive.label": "키보드",
  "interactive.on": "켜짐",
  "interactive.off": "꺼짐",
  "interactive.aria": "모든 터미널의 대화형 키보드 입력 전환",
  "interactive.offHint": "모든 Grok 터미널에 키보드 입력 활성화",
  "interactive.warning": "대화형 모드가 켜져 있습니다. 키 입력과 붙여넣기는 실시간 Grok 세션으로 전송됩니다. 터미널 크기는 항상 보이는 영역에 맞춰 동기화됩니다. 새로고침하면 키보드는 읽기 전용으로 돌아갑니다.",
  "interactive.disconnected": "실시간 채널이 끊겼습니다. 입력은 전송되지 않으며 버퍼에도 남지 않습니다.",
  "interactive.invalidPayload": "터미널 명령 페이로드가 올바르지 않습니다",
  "interactive.sendFailed": "실시간 채널로 터미널 명령을 보내지 못했습니다",
  "interactive.error": "터미널 입력 실패: {detail}",
  "interactive.unavailable": "터미널 입력을 사용할 수 없음",
  "interactive.unavailableShort": "입력 오프라인",

  "action.confirmCloseSession": "{id} 및 해당 Grok 프로세스를 닫으시겠습니까?",
  "action.closedSession":
    "Grok 세션 {id}을(를) 닫았습니다. 실시간 채널이 업데이트를 푸시합니다.",
  "action.closeFailed": "닫기 실패: {detail}",
  "action.confirmCloseGroup":
    "Codex “{owner}” 아래 Grok 세션 {count}개를 모두 닫으시겠습니까?",
  "action.groupEmpty": "이 Codex 그룹에 활성 Grok 세션이 없습니다.",
  "action.groupPartial":
    "세션 {closed}/{matched}개를 닫음; 실패: {failures}",
  "action.groupClosed":
    "Codex “{owner}” 아래 Grok 세션 {count}개를 모두 닫았습니다. 실시간 채널이 업데이트를 푸시합니다.",
  "action.failureJoin": ", ",
};
