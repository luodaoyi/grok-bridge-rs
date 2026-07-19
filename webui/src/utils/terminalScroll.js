const terminalScroll = new Map();

export function captureTerminal(terminal) {
  return terminal
    ? {
        top: terminal.scrollTop,
        left: terminal.scrollLeft,
        stickToBottom:
          terminal.scrollHeight - terminal.scrollTop - terminal.clientHeight < 8,
      }
    : null;
}

export function restoreTerminal(terminal, saved) {
  if (!terminal || !saved) return;
  terminal.scrollLeft = saved.left;
  terminal.scrollTop = saved.stickToBottom ? terminal.scrollHeight : saved.top;
}

export function rememberTerminal(id, terminal) {
  const saved = captureTerminal(terminal);
  if (saved) terminalScroll.set(id, saved);
}

export function recallTerminal(id) {
  return terminalScroll.get(id) ?? null;
}
