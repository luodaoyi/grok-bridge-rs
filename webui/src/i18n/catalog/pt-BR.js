/** @type {Record<string, string>} */
export default {
  "doc.title": "Grok Bridge — Sessões",
  "doc.description":
    "Console local do Grok Bridge: gerenciamento de supervisores Codex e subagentes Grok",

  "app.skipToSessions": "Ir para a lista de sessões",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Console de Supervisores e Subagentes",
  "app.subtitle":
    "Cada sessão Codex atua como supervisor; as sessões Grok iniciadas por ela são seus subagentes. A saída do terminal é transmitida em tempo real via WebSocket. Fechar a página não encerra as sessões existentes.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Abrir grok-bridge-rs no GitHub",
  "app.github": "GitHub",

  "connection.initial": "Conectando canal em tempo real",
  "connection.connected": "Canal em tempo real conectado",
  "connection.disconnected": "Canal em tempo real desconectado",
  "connection.retrying": "Reconectando canal em tempo real",
  "connection.reconnect": "Reconectar",
  "connection.reconnectAria": "Reconectar manualmente o canal em tempo real",

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
  "stats.working": "Trabalhando",
  "stats.waiting": "Aguardando",
  "stats.done": "Concluído / Ocioso",

  "stream.initial": "Conectando canal em tempo real…",
  "stream.retrying": "Reconectando canal em tempo real…",
  "stream.disconnected":
    "Canal em tempo real desconectado, aguardando reconexão…",
  "stream.updated": "Atualização em tempo real: {time}",
  "stream.waitingData":
    "Canal em tempo real conectado, aguardando dados de sessão…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error":
    "Erro no canal em tempo real: {detail} (será feita uma nova tentativa automaticamente)",

  "connecting.title": "Conectando canal em tempo real",
  "connecting.body":
    "Estabelecendo conexão WebSocket com o Runtime local; o primeiro snapshot de sessão será renderizado imediatamente.",

  "empty.title": "Nenhuma sessão Grok",
  "empty.body":
    "As sessões Grok iniciadas pelo Codex aparecem automaticamente aqui como subagentes e exibem a saída do terminal e o estado do ciclo de vida.",

  "board.aria": "Grupos de supervisores",

  "update.title": "Atualização disponível: v{version}",
  "update.body":
    "O Runtime atual é v{current}. Baixe e substitua o binário local manualmente e reinicie para aplicar.",
  "update.openRelease": "Abrir Release mais recente",
  "update.dismiss": "Lembrar depois",

  "error.renderTitle": "Erro de renderização da página",
  "error.reload": "Recarregar",
  "error.unknown": "erro desconhecido",
  "error.timeout": "Tempo limite da requisição; tentando novamente",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Trabalhando",
  "activity.waiting": "Aguardando",
  "activity.done": "Concluído",
  "activity.stopped": "Encerrado",
  "activity.unknown": "Desconhecido",

  "client.unmanaged": "Não rastreado",
  "client.connected": "Codex online",
  "client.disconnected": "Codex desconectado",
  "client.orphaned": "Aguardando limpeza automática",
  "client.closing": "Limpando",
  "client.unknown": "Desconhecido",

  "lifecycle.unmanaged": "Não gerenciado",
  "lifecycle.connected": "Supervisor online",
  "lifecycle.disconnected": "Supervisor offline",
  "lifecycle.orphaned": "Contagem regressiva para limpeza",
  "lifecycle.closing": "Limpando",
  "lifecycle.unknown": "Desconhecido",

  "badge.phase": "Fase PTY: {phase}",

  "group.supervisor": "Supervisor · Codex",
  "group.unowned": "Conversa Codex não rotulada",
  "group.subagentCount": "Subagentes: {n}",
  "group.closeAll": "Fechar todos os Grok deste Codex",
  "group.closeAllAria":
    "Fechar todos os subagentes Grok sob o supervisor {owner}",
  "group.summary.working": "Trabalhando: {n}",
  "group.summary.waiting": "Aguardando: {n}",
  "group.summary.done": "Concluídos/ociosos: {n}",
  "group.summary.sep": " · ",
  "group.summary.none": "Sem status",
  "group.idPrefix": "id {id}",

  "session.subagent": "Subagente",
  "session.close": "Fechar Grok",
  "session.closeAria": "Fechar subagente {id}",
  "session.waitingCollapsed": "Aguardando: {reason}",
  "session.waitingNote": "Aguardando Codex: {reason}",
  "session.meta.id": "ID da sessão",
  "session.meta.pid": "Processo",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Última atualização",
  "session.meta.client": "Conexão Codex",
  "session.meta.autoClose": "Contagem regressiva para limpeza automática",
  "session.meta.hook": "Último Hook",
  "session.meta.tool": "Ferramenta atual",
  "session.meta.cwd": "Diretório de trabalho",
  "session.terminalAria": "Terminal do subagente {title}",

  "session.lifecycle.disconnectedTitle":
    "Supervisor offline — ainda não fechando",
  "session.lifecycle.disconnectedBody":
    "Estágios Running ou Waiting nunca são fechados automaticamente. O período de carência começa apenas após um estágio seguro Idle ou terminal.",
  "session.lifecycle.orphanedTitle":
    "Contagem regressiva para fechamento automático",
  "session.lifecycle.orphanedCountdown":
    "Elegível para limpeza em {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Prazo de elegibilidade expirado — aguardando limpeza do Runtime",
  "session.lifecycle.orphanedAt":
    "Prazo local de elegibilidade para limpeza {at}; o Runtime limpa pouco depois",
  "session.lifecycle.orphanedNoDeadline":
    "Órfã; o Runtime não informou prazo de elegibilidade para limpeza.",
  "session.lifecycle.closingTitle": "Fechando sessão",
  "session.lifecycle.closingBody":
    "Esta sessão gerenciada está sendo fechada agora.",
  "session.lifecycle.collapsedOrphaned":
    "Elegível para limpeza em {remaining}",
  "session.lifecycle.collapsedOrphanedDue":
    "Aguardando limpeza do Runtime",
  "session.lifecycle.collapsedOrphanedUnknown":
    "Órfã — fechamento automático pendente",
  "session.lifecycle.collapsedClosing": "Fechando agora",

  "terminal.header": "Terminal · somente leitura em tempo real",
  "terminal.headerInteractive": "Terminal · interativo",
  "terminal.aria": "Terminal de {id}",
  "terminal.resizeAria": "Redimensionar altura do terminal",
  "terminal.resizeTitle": "Arraste para redimensionar a altura do terminal",
  "terminal.resizeHint":
    "Use as setas para redimensionar; Enter restaura a altura padrão",
  "terminal.resizeValue": "{height} pixels",

  "interactive.label": "Teclado",
  "interactive.on": "Ligado",
  "interactive.off": "Desligado",
  "interactive.aria":
    "Alternar entrada interativa do teclado para todos os terminais",
  "interactive.offHint":
    "Habilitar entrada do teclado para todos os terminais Grok",
  "interactive.warning":
    "Modo interativo ativado: teclas e colagem são enviadas para sessões Grok ativas. O redimensionamento do terminal sempre acompanha a viewport visível. Recarregar restaura o teclado para somente leitura.",
  "interactive.disconnected":
    "Canal em tempo real desconectado; a entrada não é enviada nem armazenada em buffer.",
  "interactive.invalidPayload":
    "Payload de comando do terminal inválido",
  "interactive.sendFailed":
    "Falha ao enviar o comando do terminal pelo canal em tempo real",
  "interactive.error": "Falha na entrada do terminal: {detail}",
  "interactive.unavailable": "Entrada do terminal indisponível",
  "interactive.unavailableShort": "entrada offline",

  "action.confirmCloseSession":
    "Fechar {id} e seu processo Grok?",
  "action.closedSession":
    "Sessão Grok {id} fechada. O canal em tempo real enviará atualizações.",
  "action.closeFailed": "Falha ao fechar: {detail}",
  "action.confirmCloseGroup":
    "Fechar todas as {count} sessões Grok sob Codex “{owner}”?",
  "action.groupEmpty":
    "Este grupo Codex não possui sessões Grok ativas.",
  "action.groupPartial":
    "Fechadas {closed}/{matched} sessões; falhas: {failures}",
  "action.groupClosed":
    "Todas as {count} sessões Grok sob Codex “{owner}” foram fechadas. O canal em tempo real enviará atualizações.",
  "action.failureJoin": ", ",
};
