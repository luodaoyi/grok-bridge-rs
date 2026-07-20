/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge Sitzungen",
  "doc.description":
    "Grok Bridge lokale Sitzungskonsole: Codex-Supervisoren und Grok-Subagenten-Verwaltung",

  "app.skipToSessions": "Zur Sitzungsliste springen",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Supervisor- und Subagenten-Konsole",
  "app.subtitle":
    "Jede Codex-Unterhaltung ist ein Supervisor; die Grok-Sitzungen darunter sind einklappbare persistente Subagenten. Terminals empfangen schreibgeschützte Ausgabe in Echtzeit über WebSocket; das Schließen dieses Fensters beeinflusst die vom Runtime gehaltenen Sitzungen nicht.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "grok-bridge-rs auf GitHub öffnen",
  "app.github": "GitHub",

  "connection.initial": "Live-Kanal wird verbunden",
  "connection.connected": "Live-Kanal verbunden",
  "connection.disconnected": "Live-Kanal getrennt",
  "connection.retrying": "Live-Kanal wird erneut verbunden",
  "connection.reconnect": "Erneut verbinden",
  "connection.reconnectAria": "Live-Kanal manuell erneut verbinden",

  "header.expandAll": "Alle ausklappen",
  "header.collapseAll": "Alle einklappen",

  "lang.label": "Sprache",
  "lang.aria": "Oberflächensprache",

  "theme.aria": "Farbdesign",
  "theme.auto": "Auto",
  "theme.light": "Hell",
  "theme.dark": "Dunkel",
  "theme.title": "{label}-Design",

  "stats.aria": "Sitzungsstatistik",
  "stats.owners": "Supervisoren (Codex)",
  "stats.sessions": "Subagenten (Grok)",
  "stats.working": "Aktiv",
  "stats.waiting": "Wartet",
  "stats.done": "Fertig / Leerlauf",

  "stream.initial": "Live-Kanal wird verbunden…",
  "stream.retrying": "Live-Kanal wird erneut verbunden…",
  "stream.disconnected":
    "Live-Kanal getrennt, warte auf erneute Verbindung…",
  "stream.updated": "Live-Update: {time}",
  "stream.waitingData":
    "Live-Kanal verbunden, warte auf Sitzungsdaten…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error":
    "Live-Kanal-Fehler: {detail} (wird automatisch erneut versucht)",

  "connecting.title": "Live-Kanal wird verbunden",
  "connecting.body":
    "Eine WebSocket-Verbindung zum lokalen Runtime wird aufgebaut; der erste Sitzungs-Snapshot wird sofort gerendert.",

  "empty.title": "Keine Grok-Sitzungen",
  "empty.body":
    "Neue Codex-Supervisor-Aufrufe erscheinen hier automatisch; jede Grok-Sitzung zeigt Terminal und Lebenszyklus als persistenten Subagenten.",

  "board.aria": "Supervisor-Gruppen",

  "update.title": "Update verfügbar: v{version}",
  "update.body":
    "Aktuelle Runtime ist v{current}. Bitte die lokale Binärdatei manuell herunterladen und ersetzen, dann neu starten.",
  "update.openRelease": "Neueste Release öffnen",
  "update.dismiss": "Später erinnern",

  "error.renderTitle": "Seiten-Renderfehler",
  "error.reload": "Neu laden",
  "error.unknown": "unbekannter Fehler",
  "error.timeout": "Anfrage zeitüberschritten; automatischer erneuter Versuch",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Aktiv",
  "activity.waiting": "Wartet",
  "activity.done": "Fertig",
  "activity.stopped": "Beendet",
  "activity.unknown": "Unbekannt",

  "client.unmanaged": "Nicht verfolgt",
  "client.connected": "Codex verbunden",
  "client.disconnected": "Codex getrennt",
  "client.orphaned": "Wartet auf Auto-Bereinigung",
  "client.closing": "Wird bereinigt",
  "client.unknown": "Unbekannt",

  "lifecycle.unmanaged": "Nicht verwaltet",
  "lifecycle.connected": "Supervisor verbunden",
  "lifecycle.disconnected": "Supervisor getrennt",
  "lifecycle.orphaned": "Bereinigungs-Countdown",
  "lifecycle.closing": "Bereinigung",
  "lifecycle.unknown": "Unbekannt",

  "badge.phase": "PTY-Phase: {phase}",

  "group.supervisor": "Supervisor · Codex",
  "group.unowned": "Unbeschriftete Codex-Unterhaltung",
  "group.subagentCount": "{n} Subagenten",
  "group.closeAll": "Alle Grok dieses Codex schließen",
  "group.closeAllAria":
    "Alle Grok-Subagenten unter Supervisor {owner} schließen",
  "group.summary.working": "{n} aktiv",
  "group.summary.waiting": "{n} wartend",
  "group.summary.done": "{n} fertig/leerlauf",
  "group.summary.sep": " · ",
  "group.summary.none": "Kein Status",
  "group.idPrefix": "id {id}",

  "session.subagent": "Subagent",
  "session.close": "Grok schließen",
  "session.closeAria": "Subagent {id} schließen",
  "session.waitingCollapsed": "Wartet: {reason}",
  "session.waitingNote": "Wartet auf Codex: {reason}",
  "session.meta.id": "Sitzungs-ID",
  "session.meta.pid": "Prozess",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Zuletzt aktualisiert",
  "session.meta.client": "Codex-Verbindung",
  "session.meta.autoClose": "Auto-Bereinigungs-Countdown",
  "session.meta.hook": "Letzter Hook",
  "session.meta.tool": "Aktuelles Werkzeug",
  "session.lifecycle.disconnectedTitle":
    "Supervisor offline — noch kein Schließen",
  "session.lifecycle.disconnectedBody":
    "Running- oder Waiting-Phasen werden nie automatisch geschlossen. Die Karenz beginnt erst nach sicherem Idle oder Endzustand.",
  "session.lifecycle.orphanedTitle": "Auto-Schließen-Countdown",
  "session.lifecycle.orphanedCountdown":
    "Bereinigungsberechtigt in {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Frist abgelaufen — warte auf Runtime-Bereinigung",
  "session.lifecycle.orphanedAt":
    "Lokale Bereinigungsfrist {at}; Runtime bereinigt kurz danach",
  "session.lifecycle.orphanedNoDeadline":
    "Verwaist; Runtime meldete keine Bereinigungsfrist.",
  "session.lifecycle.closingTitle": "Sitzung wird geschlossen",
  "session.lifecycle.closingBody":
    "Diese verwaltete Sitzung wird gerade geschlossen.",
  "session.lifecycle.collapsedOrphaned":
    "Bereinigungsberechtigt in {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "Warte auf Runtime-Bereinigung",
  "session.lifecycle.collapsedOrphanedUnknown":
    "Verwaist — Auto-Schließen ausstehend",
  "session.lifecycle.collapsedClosing": "Wird geschlossen",
  "session.meta.cwd": "Arbeitsverzeichnis",
  "session.terminalAria": "Terminal für Subagent {title}",

  "terminal.header": "Terminal · schreibgeschützt live",
  "terminal.headerInteractive": "Terminal · interaktiv",
  "terminal.aria": "Terminal für {id}",
  "terminal.resizeAria": "Terminalhöhe anpassen",
  "terminal.resizeTitle": "Ziehen, um die Terminalhöhe anzupassen",
  "terminal.resizeHint": "Pfeiltasten zum Anpassen; Enter stellt die Standardhöhe wieder her",
  "terminal.resizeValue": "{height} Pixel",

  "interactive.label": "Tastatur",
  "interactive.on": "An",
  "interactive.off": "Aus",
  "interactive.aria": "Interaktive Tastatureingabe für alle Terminals umschalten",
  "interactive.offHint": "Tastatureingabe an alle Grok-Terminals aktivieren",
  "interactive.warning": "Interaktiver Modus ist an: Tasten und Einfügen gehen an live Grok-Sitzungen. Die Terminalgröße folgt stets dem sichtbaren Viewport. Neu laden stellt die Tastatur auf Nur-Lesen zurück.",
  "interactive.disconnected": "Live-Kanal getrennt; Eingaben werden weder gesendet noch zwischengespeichert.",
  "interactive.invalidPayload": "Ungültige Terminalbefehlsdaten",
  "interactive.sendFailed": "Terminalbefehl konnte über den Live-Kanal nicht gesendet werden",
  "interactive.error": "Terminaleingabe fehlgeschlagen: {detail}",
  "interactive.unavailable": "Terminaleingabe nicht verfügbar",
  "interactive.unavailableShort": "Eingabe getrennt",

  "action.confirmCloseSession": "{id} und seinen Grok-Prozess schließen?",
  "action.closedSession":
    "Grok-Sitzung {id} geschlossen. Live-Kanal pusht Updates.",
  "action.closeFailed": "Schließen fehlgeschlagen: {detail}",
  "action.confirmCloseGroup":
    "Alle {count} Grok-Sitzungen unter Codex „{owner}“ schließen?",
  "action.groupEmpty":
    "Diese Codex-Gruppe hat keine aktiven Grok-Sitzungen mehr.",
  "action.groupPartial":
    "{closed}/{matched} Sitzungen geschlossen; Fehler: {failures}",
  "action.groupClosed":
    "Alle {count} Grok-Sitzungen unter Codex „{owner}“ geschlossen. Live-Kanal pusht Updates.",
  "action.failureJoin": ", ",
};
