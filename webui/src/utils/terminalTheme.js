function cssVar(name, fallback) {
  try {
    const value = getComputedStyle(document.documentElement)
      .getPropertyValue(name)
      .trim();
    return value || fallback;
  } catch {
    return fallback;
  }
}

/** Map WebUI CSS tokens to an xterm.js theme object. */
export function readTerminalTheme() {
  const background = cssVar("--terminal-bg", "#050706");
  const foreground = cssVar("--terminal-text", "#d1fae5");
  const accent = cssVar("--accent", "#5eead4");
  const selection = cssVar("--terminal-selection", "rgba(94, 234, 212, 0.35)");
  return {
    background,
    foreground,
    cursor: foreground,
    cursorAccent: background,
    selectionBackground: selection,
    selectionInactiveBackground: selection,
    black: "#0b0d0c",
    red: "#f87171",
    green: "#4ade80",
    yellow: "#fbbf24",
    blue: "#38bdf8",
    magenta: "#c084fc",
    cyan: accent,
    white: foreground,
    brightBlack: "#6b7280",
    brightRed: "#fca5a5",
    brightGreen: "#86efac",
    brightYellow: "#fde68a",
    brightBlue: "#7dd3fc",
    brightMagenta: "#d8b4fe",
    brightCyan: "#99f6e4",
    brightWhite: "#f8fafc",
  };
}

export const TERMINAL_FONT_FAMILY =
  'Consolas, "Cascadia Mono", "Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC", "Segoe UI", monospace';
