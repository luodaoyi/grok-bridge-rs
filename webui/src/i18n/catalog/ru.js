/** @type {Record<string, string>} */
export default {
  "doc.title": "Сессии Grok Bridge",
  "doc.description":
    "Локальная консоль сессий Grok Bridge: супервизоры Codex и управление субагентами Grok",

  "app.skipToSessions": "К списку сессий",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Консоль супервизоров и субагентов",
  "app.subtitle":
    "Каждый диалог Codex — супервизор; сессии Grok под ним — сворачиваемые постоянные субагенты. Терминалы получают вывод только для чтения по WebSocket в реальном времени; закрытие окна не влияет на сессии, удерживаемые Runtime.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Открыть grok-bridge-rs на GitHub",
  "app.github": "GitHub",

  "connection.initial": "Подключение живого канала",
  "connection.connected": "Живой канал подключён",
  "connection.disconnected": "Живой канал отключён",
  "connection.retrying": "Повторное подключение живого канала",
  "connection.reconnect": "Переподключить",
  "connection.reconnectAria": "Вручную переподключить живой канал",

  "header.expandAll": "Развернуть все",
  "header.collapseAll": "Свернуть все",

  "lang.label": "Язык",
  "lang.aria": "Язык интерфейса",

  "theme.aria": "Цветовая тема",
  "theme.auto": "Авто",
  "theme.light": "Светлая",
  "theme.dark": "Тёмная",
  "theme.title": "Тема: {label}",

  "stats.aria": "Статистика сессий",
  "stats.owners": "Супервизоры (Codex)",
  "stats.sessions": "Субагенты (Grok)",
  "stats.working": "Работает",
  "stats.waiting": "Ожидает",
  "stats.done": "Готово / Простой",

  "stream.initial": "Подключение живого канала…",
  "stream.retrying": "Повторное подключение живого канала…",
  "stream.disconnected":
    "Живой канал отключён, ожидание переподключения…",
  "stream.updated": "Обновление: {time}",
  "stream.waitingData":
    "Живой канал подключён, ожидание данных сессий…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error":
    "Ошибка живого канала: {detail} (будет повторная попытка)",

  "connecting.title": "Подключение живого канала",
  "connecting.body":
    "Устанавливается WebSocket-соединение с локальным Runtime; первый снимок сессий отобразится сразу.",

  "empty.title": "Нет сессий Grok",
  "empty.body":
    "Новые вызовы супервизора Codex появляются здесь автоматически; каждая сессия Grok показывает терминал и жизненный цикл как постоянный субагент.",

  "board.aria": "Группы супервизоров",

  "update.title": "Доступно обновление: v{version}",
  "update.body":
    "Текущий Runtime — v{current}. Скачайте и замените локальный бинарный файл вручную, затем перезапустите.",
  "update.openRelease": "Открыть последний Release",
  "update.dismiss": "Напомнить позже",

  "error.renderTitle": "Ошибка отрисовки страницы",
  "error.reload": "Перезагрузить",
  "error.unknown": "неизвестная ошибка",
  "error.timeout": "Время запроса истекло; автоматический повтор",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Работает",
  "activity.waiting": "Ожидает",
  "activity.done": "Готово",
  "activity.stopped": "Завершён",
  "activity.unknown": "Неизвестно",

  "client.unmanaged": "Не отслеживается",
  "client.connected": "Codex в сети",
  "client.disconnected": "Codex отключён",
  "client.orphaned": "Ожидание автоочистки",
  "client.closing": "Очистка",
  "client.unknown": "Неизвестно",

  "lifecycle.unmanaged": "Не управляется",
  "lifecycle.connected": "Супервизор в сети",
  "lifecycle.disconnected": "Супервизор офлайн",
  "lifecycle.orphaned": "Обратный отсчёт очистки",
  "lifecycle.closing": "Очистка",
  "lifecycle.unknown": "Неизвестно",

  "badge.phase": "Фаза PTY: {phase}",

  "group.supervisor": "Супервизор · Codex",
  "group.unowned": "Диалог Codex без метки",
  "group.subagentCount": "{n} субагентов",
  "group.closeAll": "Закрыть все Grok этого Codex",
  "group.closeAllAria":
    "Закрыть все субагенты Grok под супервизором {owner}",
  "group.summary.working": "{n} работает",
  "group.summary.waiting": "{n} ожидает",
  "group.summary.done": "{n} готово/простой",
  "group.summary.sep": " · ",
  "group.summary.none": "Нет статуса",
  "group.idPrefix": "id {id}",

  "session.subagent": "Субагент",
  "session.close": "Закрыть Grok",
  "session.closeAria": "Закрыть субагента {id}",
  "session.waitingCollapsed": "Ожидание: {reason}",
  "session.waitingNote": "Ожидание Codex: {reason}",
  "session.meta.id": "ID сессии",
  "session.meta.pid": "Процесс",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Последнее обновление",
  "session.meta.client": "Подключение Codex",
  "session.meta.autoClose": "Обратный отсчёт автоочистки",
  "session.meta.hook": "Последний Hook",
  "session.meta.tool": "Текущий инструмент",
  "session.lifecycle.disconnectedTitle":
    "Супервизор офлайн — закрытие ещё не начато",
  "session.lifecycle.disconnectedBody":
    "Фазы Running или Waiting никогда не закрываются автоматически. Льготный период начинается только после безопасного Idle или конечного состояния.",
  "session.lifecycle.orphanedTitle": "Обратный отсчёт автозакрытия",
  "session.lifecycle.orphanedCountdown":
    "Право на очистку через {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Срок истёк — ожидание очистки Runtime",
  "session.lifecycle.orphanedAt":
    "Локальный срок права на очистку {at}; Runtime очистит вскоре после",
  "session.lifecycle.orphanedNoDeadline":
    "Сирота; Runtime не сообщил срок права на очистку.",
  "session.lifecycle.closingTitle": "Закрытие сессии",
  "session.lifecycle.closingBody":
    "Эта управляемая сессия сейчас закрывается.",
  "session.lifecycle.collapsedOrphaned": "Право на очистку через {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "Ожидание очистки Runtime",
  "session.lifecycle.collapsedOrphanedUnknown":
    "Сирота — ожидание автозакрытия",
  "session.lifecycle.collapsedClosing": "Закрывается",
  "session.meta.cwd": "Рабочий каталог",
  "session.terminalAria": "Терминал субагента {title}",

  "terminal.header": "Терминал · только чтение, live",
  "terminal.headerInteractive": "Терминал · интерактив",
  "terminal.aria": "Терминал для {id}",
  "terminal.resizeAria": "Изменить высоту терминала",
  "terminal.resizeTitle": "Перетащите, чтобы изменить высоту терминала",
  "terminal.resizeHint": "Стрелки — изменить высоту; Enter — сбросить к высоте по умолчанию",
  "terminal.resizeValue": "{height} пикс.",

  "interactive.label": "Клавиатура",
  "interactive.on": "Вкл",
  "interactive.off": "Выкл",
  "interactive.aria": "Переключить интерактивный ввод со всех терминалов",
  "interactive.offHint": "Включить ввод с клавиатуры во все терминалы Grok",
  "interactive.warning": "Интерактивный режим включён: нажатия и вставка отправляются в живые сессии Grok. Размер терминала всегда следует видимой области. Перезагрузка вернёт клавиатуру в режим только чтения.",
  "interactive.disconnected": "Живой канал отключён; ввод не отправляется и не буферизуется.",
  "interactive.invalidPayload": "Недопустимые данные команды терминала",
  "interactive.sendFailed": "Не удалось отправить команду терминала по живому каналу",
  "interactive.error": "Ошибка ввода в терминал: {detail}",
  "interactive.unavailable": "Ввод в терминал недоступен",
  "interactive.unavailableShort": "ввод офлайн",

  "action.confirmCloseSession": "Закрыть {id} и его процесс Grok?",
  "action.closedSession":
    "Сессия Grok {id} закрыта. Живой канал отправит обновления.",
  "action.closeFailed": "Не удалось закрыть: {detail}",
  "action.confirmCloseGroup":
    "Закрыть все {count} сессий Grok под Codex «{owner}»?",
  "action.groupEmpty":
    "В этой группе Codex нет активных сессий Grok.",
  "action.groupPartial":
    "Закрыто {closed}/{matched} сессий; ошибки: {failures}",
  "action.groupClosed":
    "Закрыты все {count} сессий Grok под Codex «{owner}». Живой канал отправит обновления.",
  "action.failureJoin": ", ",
};
