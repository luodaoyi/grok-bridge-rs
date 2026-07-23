/** @type {Record<string, string>} */
export default {
  "doc.title": "Sesiones Grok Bridge",
  "doc.description":
    "Consola local de sesiones Grok Bridge: supervisores Codex y gestión de subagentes Grok",

  "app.skipToSessions": "Saltar a la lista de sesiones",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Consola de supervisores y subagentes",
  "app.subtitle":
    "Cada sesión de Codex actúa como supervisor; las sesiones de Grok que lanza son sus subagentes. La salida del terminal se transmite en tiempo real vía WebSocket. Cerrar la página no detiene las sesiones existentes.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Abrir grok-bridge-rs en GitHub",
  "app.github": "GitHub",

  "connection.initial": "Conectando el canal en vivo",
  "connection.connected": "Canal en vivo conectado",
  "connection.disconnected": "Canal en vivo desconectado",
  "connection.retrying": "Reconectando el canal en vivo",
  "connection.reconnect": "Reconectar",
  "connection.reconnectAria": "Reconectar manualmente el canal en vivo",

  "header.expandAll": "Expandir todo",
  "header.collapseAll": "Contraer todo",

  "lang.label": "Idioma",
  "lang.aria": "Idioma de la interfaz",

  "theme.aria": "Tema de color",
  "theme.auto": "Auto",
  "theme.light": "Claro",
  "theme.dark": "Oscuro",
  "theme.title": "Tema {label}",

  "stats.aria": "Estadísticas de sesiones",
  "stats.owners": "Supervisores (Codex)",
  "stats.sessions": "Subagentes (Grok)",
  "stats.working": "En curso",
  "stats.waiting": "En espera",
  "stats.done": "Listo / Inactivo",

  "stream.initial": "Conectando el canal en vivo…",
  "stream.retrying": "Reconectando el canal en vivo…",
  "stream.disconnected": "Canal en vivo desconectado, esperando reconexión…",
  "stream.updated": "Actualización en vivo: {time}",
  "stream.waitingData": "Canal en vivo conectado, esperando datos de sesión…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error": "Error del canal en vivo: {detail} (se reintentará automáticamente)",

  "connecting.title": "Conectando el canal en vivo",
  "connecting.body":
    "Se está estableciendo una conexión WebSocket con el Runtime local; la primera instantánea de sesión se mostrará de inmediato.",

  "empty.title": "No hay sesiones Grok",
  "empty.body":
    "Las sesiones de Grok iniciadas por Codex aparecen aquí automáticamente como subagentes y muestran la salida del terminal y el estado del ciclo de vida.",

  "board.aria": "Grupos de supervisores",

  "update.title": "Actualización disponible: v{version}",
  "update.body":
    "El Runtime actual es v{current}. Descargue y reemplace manualmente el binario local, luego reinicie para aplicar los cambios.",
  "update.openRelease": "Abrir el último Release",
  "update.dismiss": "Recordármelo más tarde",

  "error.renderTitle": "Error de renderizado de la página",
  "error.reload": "Recargar",
  "error.unknown": "error desconocido",
  "error.timeout": "La solicitud agotó el tiempo de espera; reintentando automáticamente",
  "error.brand": "GROK BRIDGE",

  "activity.working": "En curso",
  "activity.waiting": "En espera",
  "activity.done": "Listo",
  "activity.stopped": "Finalizado",
  "activity.unknown": "Desconocido",

  "client.unmanaged": "Sin seguimiento",
  "client.connected": "Codex en línea",
  "client.disconnected": "Codex desconectado",
  "client.orphaned": "Limpieza automática pendiente",
  "client.closing": "Limpiando",
  "client.unknown": "Desconocido",

  "lifecycle.unmanaged": "No gestionado",
  "lifecycle.connected": "Supervisor en línea",
  "lifecycle.disconnected": "Supervisor desconectado",
  "lifecycle.orphaned": "Cuenta atrás de limpieza",
  "lifecycle.closing": "Limpiando",
  "lifecycle.unknown": "Desconocido",

  "badge.phase": "Fase PTY: {phase}",

  "group.supervisor": "Supervisor · Codex",
  "group.unowned": "Conversación Codex sin etiqueta",
  "group.subagentCount": "{n} subagentes",
  "group.closeAll": "Cerrar todos los Grok de este Codex",
  "group.closeAllAria":
    "Cerrar todos los subagentes Grok bajo el supervisor {owner}",
  "group.summary.working": "{n} en curso",
  "group.summary.waiting": "{n} en espera",
  "group.summary.done": "{n} listo/inactivo",
  "group.summary.sep": " · ",
  "group.summary.none": "Sin estado",
  "group.idPrefix": "id {id}",

  "session.subagent": "Subagente",
  "session.close": "Cerrar Grok",
  "session.closeAria": "Cerrar el subagente {id}",
  "session.waitingCollapsed": "En espera: {reason}",
  "session.waitingNote": "Esperando a Codex: {reason}",
  "session.meta.id": "ID de sesión",
  "session.meta.pid": "Proceso",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Última actualización",
  "session.meta.client": "Conexión Codex",
  "session.meta.autoClose": "Cuenta atrás de limpieza automática",
  "session.meta.hook": "Último Hook",
  "session.meta.tool": "Herramienta actual",
  "session.meta.cwd": "Directorio de trabajo",
  "session.terminalAria": "Terminal del subagente {title}",

  "session.lifecycle.disconnectedTitle": "Supervisor desconectado — aún no se cierra",
  "session.lifecycle.disconnectedBody":
    "Las fases Running o Waiting nunca se cierran automáticamente. El período de gracia comienza solo tras una fase Idle o terminal segura.",
  "session.lifecycle.orphanedTitle": "Cuenta atrás de cierre automático",
  "session.lifecycle.orphanedCountdown": "Elegible para limpieza en {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Plazo de elegibilidad superado — esperando la limpieza del Runtime",
  "session.lifecycle.orphanedAt":
    "Plazo local de elegibilidad para limpieza {at}; el Runtime limpia poco después",
  "session.lifecycle.orphanedNoDeadline":
    "Huérfana; el Runtime no informó un plazo de elegibilidad para limpieza.",
  "session.lifecycle.closingTitle": "Cerrando la sesión",
  "session.lifecycle.closingBody":
    "Esta sesión gestionada se está cerrando ahora.",
  "session.lifecycle.collapsedOrphaned": "Elegible para limpieza en {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "Esperando la limpieza del Runtime",
  "session.lifecycle.collapsedOrphanedUnknown": "Huérfana — cierre automático pendiente",
  "session.lifecycle.collapsedClosing": "Cerrando ahora",

  "terminal.header": "Terminal · solo lectura en vivo",
  "terminal.headerInteractive": "Terminal · interactivo",
  "terminal.aria": "Terminal de {id}",
  "terminal.resizeAria": "Redimensionar la altura del terminal",
  "terminal.resizeTitle": "Arrastre para redimensionar la altura del terminal",
  "terminal.resizeHint": "Use las teclas de flecha para redimensionar; Intro restablece la altura predeterminada",
  "terminal.resizeValue": "{height} píxeles",

  "interactive.label": "Teclado",
  "interactive.on": "Activado",
  "interactive.off": "Desactivado",
  "interactive.aria": "Alternar la entrada de teclado interactiva para todos los terminales",
  "interactive.offHint": "Activar la entrada de teclado hacia todos los terminales Grok",
  "interactive.warning": "El modo interactivo está activado: las pulsaciones y el pegado se envían a las sesiones Grok en vivo. El redimensionado del terminal siempre sigue el área visible. Al recargar, la entrada de teclado vuelve a solo lectura.",
  "interactive.disconnected": "Canal en vivo desconectado; la entrada no se envía ni se almacena en búfer.",
  "interactive.invalidPayload": "Carga útil de comando de terminal no válida",
  "interactive.sendFailed": "No se pudo enviar el comando de terminal por el canal en vivo",
  "interactive.error": "Error de entrada del terminal: {detail}",
  "interactive.unavailable": "Entrada del terminal no disponible",
  "interactive.unavailableShort": "entrada desconectada",

  "action.confirmCloseSession": "¿Cerrar {id} y su proceso Grok?",
  "action.closedSession":
    "Sesión Grok {id} cerrada. El canal en vivo enviará las actualizaciones.",
  "action.closeFailed": "Error al cerrar: {detail}",
  "action.confirmCloseGroup":
    "¿Cerrar las {count} sesiones Grok bajo Codex «{owner}»?",
  "action.groupEmpty": "Este grupo Codex no tiene sesiones Grok activas.",
  "action.groupPartial":
    "Cerradas {closed}/{matched} sesiones; fallos: {failures}",
  "action.groupClosed":
    "Cerradas las {count} sesiones Grok bajo Codex «{owner}». El canal en vivo enviará las actualizaciones.",
  "action.failureJoin": ", ",
};
