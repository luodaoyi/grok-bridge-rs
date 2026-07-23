/** @type {Record<string, string>} */
export default {
  "doc.title": "Sessions Grok Bridge",
  "doc.description":
    "Console locale Grok Bridge : superviseurs Codex et gestion des sous-agents Grok",

  "app.skipToSessions": "Aller à la liste des sessions",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Console superviseurs et sous-agents",
  "app.subtitle":
    "Chaque session Codex fait office de superviseur ; les sessions Grok qu'elle lance sont ses sous-agents. La sortie du terminal est diffusée en temps réel via WebSocket. Fermer la page ne met pas fin aux sessions existantes.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Ouvrir grok-bridge-rs sur GitHub",
  "app.github": "GitHub",

  "connection.initial": "Connexion du canal en direct",
  "connection.connected": "Canal en direct connecté",
  "connection.disconnected": "Canal en direct déconnecté",
  "connection.retrying": "Reconnexion du canal en direct",
  "connection.reconnect": "Reconnecter",
  "connection.reconnectAria": "Reconnecter manuellement le canal en direct",

  "header.expandAll": "Tout développer",
  "header.collapseAll": "Tout réduire",

  "lang.label": "Langue",
  "lang.aria": "Langue de l’interface",

  "theme.aria": "Thème de couleur",
  "theme.auto": "Auto",
  "theme.light": "Clair",
  "theme.dark": "Sombre",
  "theme.title": "Thème {label}",

  "stats.aria": "Statistiques des sessions",
  "stats.owners": "Superviseurs (Codex)",
  "stats.sessions": "Sous-agents (Grok)",
  "stats.working": "En cours",
  "stats.waiting": "En attente",
  "stats.done": "Terminé / Inactif",

  "stream.initial": "Connexion du canal en direct…",
  "stream.retrying": "Reconnexion du canal en direct…",
  "stream.disconnected":
    "Canal en direct déconnecté, attente de reconnexion…",
  "stream.updated": "Mise à jour en direct : {time}",
  "stream.waitingData":
    "Canal en direct connecté, en attente des données de session…",
  "stream.pushMode": "Push : WebSocket · /api/events",
  "stream.error":
    "Erreur du canal en direct : {detail} (nouvel essai automatique)",

  "connecting.title": "Connexion du canal en direct",
  "connecting.body":
    "Une connexion WebSocket vers le Runtime local est en cours ; le premier instantané de session s’affichera immédiatement.",

  "empty.title": "Aucune session Grok",
  "empty.body":
    "Les sessions Grok lancées par Codex apparaissent automatiquement ici comme sous-agents et affichent la sortie du terminal ainsi que l’état du cycle de vie.",

  "board.aria": "Groupes de superviseurs",

  "update.title": "Mise à jour disponible : v{version}",
  "update.body":
    "Le Runtime actuel est v{current}. Téléchargez et remplacez manuellement le binaire local, puis redémarrez pour appliquer.",
  "update.openRelease": "Ouvrir la dernière Release",
  "update.dismiss": "Me le rappeler plus tard",

  "error.renderTitle": "Erreur de rendu de page",
  "error.reload": "Recharger",
  "error.unknown": "erreur inconnue",
  "error.timeout": "Délai de requête dépassé ; nouvel essai automatique",
  "error.brand": "GROK BRIDGE",

  "activity.working": "En cours",
  "activity.waiting": "En attente",
  "activity.done": "Terminé",
  "activity.stopped": "Arrêté",
  "activity.unknown": "Inconnu",

  "client.unmanaged": "Non suivi",
  "client.connected": "Codex en ligne",
  "client.disconnected": "Codex déconnecté",
  "client.orphaned": "Nettoyage auto en attente",
  "client.closing": "Nettoyage en cours",
  "client.unknown": "Inconnu",

  "lifecycle.unmanaged": "Non géré",
  "lifecycle.connected": "Superviseur en ligne",
  "lifecycle.disconnected": "Superviseur hors ligne",
  "lifecycle.orphaned": "Compte à rebours nettoyage",
  "lifecycle.closing": "Nettoyage",
  "lifecycle.unknown": "Inconnu",

  "badge.phase": "Phase PTY : {phase}",

  "group.supervisor": "Superviseur · Codex",
  "group.unowned": "Conversation Codex sans libellé",
  "group.subagentCount": "{n} sous-agents",
  "group.closeAll": "Fermer tous les Grok de ce Codex",
  "group.closeAllAria":
    "Fermer tous les sous-agents Grok sous le superviseur {owner}",
  "group.summary.working": "{n} en cours",
  "group.summary.waiting": "{n} en attente",
  "group.summary.done": "{n} terminé/inactif",
  "group.summary.sep": " · ",
  "group.summary.none": "Aucun statut",
  "group.idPrefix": "id {id}",

  "session.subagent": "Sous-agent",
  "session.close": "Fermer Grok",
  "session.closeAria": "Fermer le sous-agent {id}",
  "session.waitingCollapsed": "En attente : {reason}",
  "session.waitingNote": "En attente de Codex : {reason}",
  "session.meta.id": "ID de session",
  "session.meta.pid": "Processus",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Dernière mise à jour",
  "session.meta.client": "Connexion Codex",
  "session.meta.autoClose": "Compte à rebours auto-nettoyage",
  "session.meta.hook": "Dernier Hook",
  "session.meta.tool": "Outil actuel",
  "session.lifecycle.disconnectedTitle":
    "Superviseur hors ligne — pas encore de fermeture",
  "session.lifecycle.disconnectedBody":
    "Les phases Running ou Waiting ne sont jamais fermées automatiquement. Le délai de grâce ne commence qu’après une phase Idle ou terminale sûre.",
  "session.lifecycle.orphanedTitle": "Compte à rebours de fermeture auto",
  "session.lifecycle.orphanedCountdown":
    "Éligible au nettoyage dans {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Échéance dépassée — en attente du nettoyage Runtime",
  "session.lifecycle.orphanedAt":
    "Échéance locale d’éligibilité au nettoyage {at} ; le Runtime nettoie peu après",
  "session.lifecycle.orphanedNoDeadline":
    "Orpheline ; le Runtime n’a pas signalé d’échéance d’éligibilité au nettoyage.",
  "session.lifecycle.closingTitle": "Fermeture de la session",
  "session.lifecycle.closingBody":
    "Cette session gérée est en cours de fermeture.",
  "session.lifecycle.collapsedOrphaned":
    "Éligible au nettoyage dans {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "En attente du nettoyage Runtime",
  "session.lifecycle.collapsedOrphanedUnknown":
    "Orpheline — fermeture auto en attente",
  "session.lifecycle.collapsedClosing": "Fermeture en cours",
  "session.meta.cwd": "Répertoire de travail",
  "session.terminalAria": "Terminal du sous-agent {title}",

  "terminal.header": "Terminal · lecture seule en direct",
  "terminal.headerInteractive": "Terminal · interactif",
  "terminal.aria": "Terminal pour {id}",
  "terminal.resizeAria": "Redimensionner la hauteur du terminal",
  "terminal.resizeTitle": "Glisser pour redimensionner la hauteur du terminal",
  "terminal.resizeHint": "Utilisez les flèches pour redimensionner ; Entrée rétablit la hauteur par défaut",
  "terminal.resizeValue": "{height} pixels",

  "interactive.label": "Clavier",
  "interactive.on": "Activé",
  "interactive.off": "Désactivé",
  "interactive.aria": "Basculer la saisie clavier interactive pour tous les terminaux",
  "interactive.offHint": "Activer la saisie clavier vers tous les terminaux Grok",
  "interactive.warning": "Mode interactif activé : frappes et collages sont envoyés aux sessions Grok en direct. Le redimensionnement du terminal suit toujours la zone visible. Un rechargement repasse le clavier en lecture seule.",
  "interactive.disconnected": "Canal en direct déconnecté ; la saisie n'est ni envoyée ni mise en mémoire tampon.",
  "interactive.invalidPayload": "Charge utile de commande terminal invalide",
  "interactive.sendFailed": "Échec de l'envoi de la commande terminal via le canal en direct",
  "interactive.error": "Échec de la saisie terminal : {detail}",
  "interactive.unavailable": "Saisie terminal indisponible",
  "interactive.unavailableShort": "saisie hors ligne",

  "action.confirmCloseSession": "Fermer {id} et son processus Grok ?",
  "action.closedSession":
    "Session Grok {id} fermée. Le canal en direct poussera les mises à jour.",
  "action.closeFailed": "Échec de la fermeture : {detail}",
  "action.confirmCloseGroup":
    "Fermer les {count} sessions Grok sous Codex « {owner} » ?",
  "action.groupEmpty":
    "Ce groupe Codex n’a plus de sessions Grok actives.",
  "action.groupPartial":
    "Fermé {closed}/{matched} sessions ; échecs : {failures}",
  "action.groupClosed":
    "Fermé les {count} sessions Grok sous Codex « {owner} ». Le canal en direct poussera les mises à jour.",
  "action.failureJoin": ", ",
};
