/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge — Sessões",
  "doc.description":
    "Consola local do Grok Bridge: gestão de supervisores Codex e subagentes Grok",

  "app.skipToSessions": "Ir para a lista de sessões",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Consola de Supervisores e Subagentes",
  "app.subtitle":
    "Cada sessão Codex atua como supervisor; as sessões Grok que inicia são os seus subagentes. A saída do terminal é transmitida em tempo real via WebSocket. Fechar a página não encerra as sessões existentes.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Abrir grok-bridge-rs no GitHub",
  "app.github": "GitHub",

  "connection.initial": "A ligar o canal em tempo real",
  "connection.connected": "Canal em tempo real ligado",
  "connection.disconnected": "Canal em tempo real desligado",
  "connection.retrying": "A religar o canal em tempo real",
  "connection.reconnect": "Religar",
  "connection.reconnectAria": "Religar manualmente o canal em tempo real",

  "header.expandAll": "Expandir tudo",
  "header.collapseAll": "Recolher tudo",

  "lang.label": "Idioma",
  "lang.aria": "Idioma da interface",

  "theme.aria": "Tema de cores",
  "theme.auto": "Automático",
  "theme.light": "Claro",
  "theme.dark": "Escuro",
  "theme.title": "Tema {label}",

  "stats.aria": "Estatísticas de sessões",
  "stats.owners": "Supervisores (Codex)",
  "stats.sessions": "Subagentes (Grok)",
  "stats.working": "Em curso",
  "stats.waiting": "A aguardar",
  "stats.done": "Concluído / Inativo",

  "stream.initial": "A ligar o canal em tempo real…",
  "stream.retrying": "A religar o canal em tempo real…",
  "stream.disconnected":
    "Canal em tempo real desligado, a aguardar religação…",
  "stream.updated": "Atualização em tempo real: {time}",
  "stream.waitingData":
    "Canal em tempo real ligado, a aguardar dados de sessão…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error":
    "Erro no canal em tempo real: {detail} (será feita uma nova tentativa automaticamente)",

  "connecting.title": "A ligar o canal em tempo real",
  "connecting.body":
    "A estabelecer uma ligação WebSocket com o Runtime local; o primeiro estado da sessão será apresentado de imediato.",

  "empty.title": "Sem sessões Grok",
  "empty.body":
    "As sessões Grok iniciadas pelo Codex surgem aqui automaticamente como subagentes e apresentam a saída do terminal e o estado do ciclo de vida.",

  "board.aria": "Grupos de supervisores",

  "update.title": "Atualização disponível: v{version}",
  "update.body":
    "O Runtime atual é v{current}. Transfira e substitua o binário local manualmente e reinicie para aplicar.",
  "update.openRelease": "Abrir Release mais recente",
  "update.dismiss": "Lembrar mais tarde",

  "error.renderTitle": "Erro de apresentação da página",
  "error.reload": "Recarregar",
  "error.unknown": "erro desconhecido",
  "error.timeout": "O pedido expirou; a tentar novamente",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Em curso",
  "activity.waiting": "A aguardar",
  "activity.done": "Concluído",
  "activity.stopped": "Terminado",
  "activity.unknown": "Desconhecido",

  "client.unmanaged": "Não monitorizado",
  "client.connected": "Codex online",
  "client.disconnected": "Codex desligado",
  "client.orphaned": "A aguardar limpeza automática",
  "client.closing": "A limpar",
  "client.unknown": "Desconhecido",

  "lifecycle.unmanaged": "Não gerido",
  "lifecycle.connected": "Supervisor online",
  "lifecycle.disconnected": "Supervisor offline",
  "lifecycle.orphaned": "Contagem decrescente de limpeza",
  "lifecycle.closing": "A limpar",
  "lifecycle.unknown": "Desconhecido",

  "badge.phase": "Fase da PTY: {phase}",

  "group.supervisor": "Supervisor · Codex",
  "group.unowned": "Conversa Codex sem etiqueta",
  "group.subagentCount": "Subagentes: {n}",
  "group.closeAll": "Fechar todos os Grok deste Codex",
  "group.closeAllAria":
    "Fechar todos os subagentes Grok sob o supervisor {owner}",
  "group.summary.working": "Em curso: {n}",
  "group.summary.waiting": "A aguardar: {n}",
  "group.summary.done": "Concluídos/inativos: {n}",
  "group.summary.sep": " · ",
  "group.summary.none": "Sem estado",
  "group.idPrefix": "id {id}",

  "session.subagent": "Subagente",
  "session.close": "Fechar Grok",
  "session.closeAria": "Fechar subagente {id}",
  "session.waitingCollapsed": "A aguardar: {reason}",
  "session.waitingNote": "A aguardar o Codex: {reason}",
  "session.meta.id": "ID da sessão",
  "session.meta.pid": "Processo",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Última atualização",
  "session.meta.client": "Ligação Codex",
  "session.meta.autoClose": "Contagem decrescente de limpeza automática",
  "session.meta.hook": "Hook mais recente",
  "session.meta.tool": "Ferramenta atual",
  "session.meta.cwd": "Diretório de trabalho",
  "session.terminalAria": "Terminal do subagente {title}",

  "session.lifecycle.disconnectedTitle":
    "Supervisor offline — ainda não a fechar",
  "session.lifecycle.disconnectedBody":
    "As fases Running ou Waiting nunca são fechadas automaticamente. O período de carência só começa após uma fase segura Idle ou terminal.",
  "session.lifecycle.orphanedTitle":
    "Contagem decrescente de fecho automático",
  "session.lifecycle.orphanedCountdown":
    "Elegível para limpeza em {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Prazo de elegibilidade expirado — a aguardar limpeza do Runtime",
  "session.lifecycle.orphanedAt":
    "Prazo local de elegibilidade para limpeza {at}; o Runtime limpa pouco depois",
  "session.lifecycle.orphanedNoDeadline":
    "Órfã; o Runtime não indicou prazo de elegibilidade para limpeza.",
  "session.lifecycle.closingTitle": "A fechar a sessão",
  "session.lifecycle.closingBody":
    "Esta sessão gerida está a ser fechada agora.",
  "session.lifecycle.collapsedOrphaned":
    "Elegível para limpeza em {remaining}",
  "session.lifecycle.collapsedOrphanedDue":
    "A aguardar limpeza do Runtime",
  "session.lifecycle.collapsedOrphanedUnknown":
    "Órfã — fecho automático pendente",
  "session.lifecycle.collapsedClosing": "A fechar agora",

  "terminal.header": "Terminal · só de leitura em tempo real",
  "terminal.headerInteractive": "Terminal · interativo",
  "terminal.aria": "Terminal de {id}",
  "terminal.resizeAria": "Redimensionar a altura do terminal",
  "terminal.resizeTitle": "Arraste para redimensionar a altura do terminal",
  "terminal.resizeHint":
    "Use as setas para redimensionar; Enter restaura a altura predefinida",
  "terminal.resizeValue": "{height} píxeis",

  "interactive.label": "Teclado",
  "interactive.on": "Ligado",
  "interactive.off": "Desligado",
  "interactive.aria":
    "Alternar entrada interativa do teclado para todos os terminais",
  "interactive.offHint":
    "Ativar entrada do teclado para todos os terminais Grok",
  "interactive.warning":
    "Modo interativo ativo: teclas e colagem são enviadas para sessões Grok ativas. O redimensionamento do terminal acompanha sempre a área visível. Recarregar restaura o teclado para só de leitura.",
  "interactive.disconnected":
    "Canal em tempo real desligado; a entrada não é enviada nem guardada temporariamente.",
  "interactive.invalidPayload":
    "Dados do comando do terminal inválidos",
  "interactive.sendFailed":
    "Falha ao enviar o comando do terminal pelo canal em tempo real",
  "interactive.error": "Falha na entrada do terminal: {detail}",
  "interactive.unavailable": "Entrada do terminal indisponível",
  "interactive.unavailableShort": "entrada offline",

  "action.confirmCloseSession":
    "Fechar {id} e o respetivo processo Grok?",
  "action.closedSession":
    "Sessão Grok {id} fechada. O canal em tempo real enviará atualizações.",
  "action.closeFailed": "Falha ao fechar: {detail}",
  "action.confirmCloseGroup":
    "Fechar todas as {count} sessões Grok sob o Codex “{owner}”?",
  "action.groupEmpty":
    "Este grupo Codex não tem sessões Grok ativas.",
  "action.groupPartial":
    "Fechadas {closed}/{matched} sessões; falhas: {failures}",
  "action.groupClosed":
    "Todas as {count} sessões Grok sob o Codex “{owner}” foram fechadas. O canal em tempo real enviará atualizações.",
  "action.failureJoin": ", ",
};
