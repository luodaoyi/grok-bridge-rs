import { useCallback, useEffect, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal as XTerm } from "@xterm/xterm";
import { useTerminalIO } from "../context/TerminalIOContext.jsx";
import { useI18n } from "../i18n/index.js";
import {
  chunkUtf8ToBase64,
  decodeBase64ToUint8Array,
} from "../utils/base64.js";
import { subscribeTerminal } from "../utils/terminalFeeds.js";
import { clampTerminalGrid } from "../utils/terminalGrid.js";
import {
  TERMINAL_HEIGHT_DEFAULT,
  TERMINAL_HEIGHT_MIN,
  canFitElement,
  clampTerminalHeight,
  maxTerminalHeight,
  readTerminalHeight,
  subscribeTerminalHeight,
  writeTerminalHeight,
} from "../utils/terminalHeight.js";
import {
  TERMINAL_FONT_FAMILY,
  readTerminalTheme,
} from "../utils/terminalTheme.js";

const DEFAULT_ROWS = 24;
const DEFAULT_COLS = 80;
const RESIZE_STEP_PX = 24;
const RESIZE_DEBOUNCE_MS = 120;

function safeRows(rows) {
  const value = Number(rows);
  if (!Number.isFinite(value) || value < 1) return DEFAULT_ROWS;
  return Math.min(Math.floor(value), 500);
}

function safeCols(cols) {
  const value = Number(cols);
  if (!Number.isFinite(value) || value < 1) return DEFAULT_COLS;
  return Math.min(Math.floor(value), 500);
}

/**
 * Per-terminal FIFO drain for xterm writes.
 * xterm `write(data, callback)` parses asynchronously; a later `reset()` must
 * not run until earlier queued writes' callbacks have fired, and a reset entry
 * must complete its snapshot write before later appends.
 */
export function createTerminalWriteQueue(term) {
  const queue = [];
  let busy = false;
  let disposed = false;

  const writeBytes = (bytes, done) => {
    if (disposed) {
      done();
      return;
    }
    if (!bytes || bytes.length === 0) {
      done();
      return;
    }
    try {
      term.write(bytes, () => {
        done();
      });
    } catch {
      done();
    }
  };

  const processEntry = (entry, done) => {
    if (disposed) {
      done();
      return;
    }
    const bytes = decodeBase64ToUint8Array(entry.data_base64);
    if (entry.reset) {
      try {
        term.reset();
      } catch {
        /* ignore reset races after dispose */
      }
      writeBytes(bytes, done);
      return;
    }
    writeBytes(bytes, done);
  };

  const pump = () => {
    if (disposed || busy) return;
    const entry = queue.shift();
    if (!entry) return;
    busy = true;
    processEntry(entry, () => {
      busy = false;
      if (disposed) {
        queue.length = 0;
        return;
      }
      pump();
    });
  };

  return {
    enqueue(entry) {
      if (disposed || !entry) return;
      queue.push(entry);
      pump();
    },
    dispose() {
      disposed = true;
      queue.length = 0;
    },
    get pending() {
      return queue.length;
    },
    get isBusy() {
      return busy;
    },
  };
}

/**
 * Schedule FitAddon.fit only when the host has a real non-zero box.
 * Returns true when fit ran.
 */
export function fitTerminalHost(fitAddon, host) {
  if (!fitAddon || !canFitElement(host)) return false;
  try {
    fitAddon.fit();
    return true;
  } catch {
    return false;
  }
}

/**
 * xterm.js terminal driven by the WebSocket feed.
 * terminal_resize follows visible fit always (viewport sync).
 * terminal_input is gated strictly by the global interactive switch.
 */
export function Terminal({ id, heightKey, rows, cols, label }) {
  const { t } = useI18n();
  const {
    interactive,
    sendTerminalInput,
    sendTerminalResize,
    connectionState,
  } = useTerminalIO();

  const hostRef = useRef(null);
  const shellRef = useRef(null);
  const termRef = useRef(null);
  const fitRef = useRef(null);
  const fitRafRef = useRef(0);
  const dragRef = useRef(null);
  const interactiveRef = useRef(interactive);
  const sendInputRef = useRef(sendTerminalInput);
  const sendResizeRef = useRef(sendTerminalResize);
  const onDataDisposableRef = useRef(null);
  const lastSentSizeRef = useRef({ cols: 0, rows: 0 });
  const resizeTimerRef = useRef(0);
  // Height is scoped to the Codex supervisor group, not the Grok session.
  const groupHeightKey = heightKey;

  interactiveRef.current = interactive;
  sendInputRef.current = sendTerminalInput;
  sendResizeRef.current = sendTerminalResize;

  const [height, setHeight] = useState(() =>
    readTerminalHeight(groupHeightKey),
  );

  const maybeSendResize = useCallback((term) => {
    if (!term) return;
    if (!canFitElement(hostRef.current)) return;
    const { cols: nextCols, rows: nextRows } = clampTerminalGrid(
      term.cols,
      term.rows,
    );
    const last = lastSentSizeRef.current;
    if (last.cols === nextCols && last.rows === nextRows) return;
    const result = sendResizeRef.current(id, nextCols, nextRows);
    // Only commit dedupe state after a successful send so failures stay retryable.
    if (result?.ok) {
      lastSentSizeRef.current = { cols: nextCols, rows: nextRows };
    }
  }, [id]);

  const scheduleFit = useCallback(() => {
    if (fitRafRef.current) {
      cancelAnimationFrame(fitRafRef.current);
    }
    fitRafRef.current = requestAnimationFrame(() => {
      fitRafRef.current = 0;
      const fitted = fitTerminalHost(fitRef.current, hostRef.current);
      if (!fitted) return;
      const term = termRef.current;
      if (!term) return;
      if (resizeTimerRef.current) {
        window.clearTimeout(resizeTimerRef.current);
      }
      resizeTimerRef.current = window.setTimeout(() => {
        resizeTimerRef.current = 0;
        maybeSendResize(term);
      }, RESIZE_DEBOUNCE_MS);
    });
  }, [maybeSendResize]);

  const applyHeight = useCallback(
    (next) => {
      // Persists + notifies same-group terminals; always update local height.
      setHeight(writeTerminalHeight(groupHeightKey, next));
    },
    [groupHeightKey],
  );

  useEffect(() => {
    setHeight(readTerminalHeight(groupHeightKey));
    return subscribeTerminalHeight(groupHeightKey, setHeight);
  }, [groupHeightKey]);

  useEffect(() => {
    lastSentSizeRef.current = { cols: 0, rows: 0 };
  }, [id]);

  // Mount xterm once per session id.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;

    const term = new XTerm({
      disableStdin: true,
      cursorBlink: false,
      convertEol: false,
      scrollback: 5000,
      fontFamily: TERMINAL_FONT_FAMILY,
      fontSize: 13,
      lineHeight: 1.25,
      theme: readTerminalTheme(),
      rows: safeRows(rows),
      cols: safeCols(cols),
      allowProposedApi: false,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(host);
    termRef.current = term;
    fitRef.current = fitAddon;

    const writeQueue = createTerminalWriteQueue(term);
    const unsubscribe = subscribeTerminal(id, (entry) => {
      writeQueue.enqueue(entry);
    });

    const runFit = () => {
      fitTerminalHost(fitAddon, host);
    };
    runFit();
    scheduleFit();

    let resizeObserver = null;
    if (typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(() => {
        scheduleFit();
      });
      resizeObserver.observe(host);
    }

    const onWindowResize = () => {
      setHeight((current) => clampTerminalHeight(current));
      scheduleFit();
    };
    window.addEventListener("resize", onWindowResize);

    return () => {
      unsubscribe();
      writeQueue.dispose();
      window.removeEventListener("resize", onWindowResize);
      if (resizeObserver) resizeObserver.disconnect();
      if (fitRafRef.current) {
        cancelAnimationFrame(fitRafRef.current);
        fitRafRef.current = 0;
      }
      if (resizeTimerRef.current) {
        window.clearTimeout(resizeTimerRef.current);
        resizeTimerRef.current = 0;
      }
      if (onDataDisposableRef.current) {
        try {
          onDataDisposableRef.current.dispose();
        } catch {
          /* ignore */
        }
        onDataDisposableRef.current = null;
      }
      fitRef.current = null;
      termRef.current = null;
      try {
        fitAddon.dispose();
      } catch {
        /* ignore */
      }
      term.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional mount-per-id
  }, [id, scheduleFit]);

  // Toggle stdin + onData without rebuilding Terminal.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return undefined;

    if (onDataDisposableRef.current) {
      try {
        onDataDisposableRef.current.dispose();
      } catch {
        /* ignore */
      }
      onDataDisposableRef.current = null;
    }

    term.options.disableStdin = !interactive;
    term.options.cursorBlink = interactive;

    if (!interactive) {
      return undefined;
    }

    const disposable = term.onData((data) => {
      // Never buffer: if mode flipped off mid-event, drop immediately.
      if (!interactiveRef.current) return;
      const chunks = chunkUtf8ToBase64(data);
      for (const dataBase64 of chunks) {
        if (!interactiveRef.current) return;
        sendInputRef.current(id, dataBase64);
      }
    });
    onDataDisposableRef.current = disposable;

    return () => {
      if (onDataDisposableRef.current === disposable) {
        try {
          disposable.dispose();
        } catch {
          /* ignore */
        }
        onDataDisposableRef.current = null;
      }
    };
  }, [id, interactive]);

  useEffect(() => {
    scheduleFit();
  }, [height, scheduleFit]);

  useEffect(() => {
    const shell = shellRef.current;
    if (!shell || typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(() => {
      scheduleFit();
    });
    observer.observe(shell);
    return () => observer.disconnect();
  }, [id, scheduleFit]);

  useEffect(() => {
    const applyTheme = () => {
      const term = termRef.current;
      if (!term) return;
      term.options.theme = readTerminalTheme();
    };

    applyTheme();
    const root = document.documentElement;
    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (
          mutation.type === "attributes" &&
          (mutation.attributeName === "data-resolved-theme" ||
            mutation.attributeName === "data-theme")
        ) {
          applyTheme();
          break;
        }
      }
    });
    observer.observe(root, {
      attributes: true,
      attributeFilter: ["data-resolved-theme", "data-theme"],
    });
    return () => observer.disconnect();
  }, [id]);

  useEffect(() => {
    return () => {
      const drag = dragRef.current;
      if (!drag) return;
      window.removeEventListener("pointermove", drag.onMove);
      window.removeEventListener("pointerup", drag.onUp);
      window.removeEventListener("pointercancel", drag.onUp);
      dragRef.current = null;
    };
  }, []);

  const onResizePointerDown = (event) => {
    if (event.button != null && event.button !== 0) return;
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = height;
    const onMove = (moveEvent) => {
      const delta = moveEvent.clientY - startY;
      applyHeight(startHeight + delta);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      dragRef.current = null;
      scheduleFit();
    };
    dragRef.current = { onMove, onUp };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const onResizeKeyDown = (event) => {
    if (event.key === "ArrowUp") {
      event.preventDefault();
      applyHeight(height - RESIZE_STEP_PX);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      applyHeight(height + RESIZE_STEP_PX);
    } else if (event.key === "Home") {
      event.preventDefault();
      applyHeight(TERMINAL_HEIGHT_MIN);
    } else if (event.key === "End") {
      event.preventDefault();
      applyHeight(maxTerminalHeight());
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      applyHeight(TERMINAL_HEIGHT_DEFAULT);
    }
  };

  const maxHeight = maxTerminalHeight();
  const ptyLabel = `${safeCols(cols)}×${safeRows(rows)}`;
  const headerLabel = interactive
    ? t("terminal.headerInteractive")
    : t("terminal.header");

  return (
    <div
      ref={shellRef}
      className="terminal-shell relative flex w-full min-w-0 flex-col overflow-hidden rounded-xl border border-[var(--terminal-border)] bg-[var(--terminal-bg)] shadow-[inset_0_1px_0_var(--terminal-inset)] focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-[var(--focus)]"
      data-terminal={id}
      data-readonly={interactive ? "false" : "true"}
      data-interactive={interactive ? "on" : "off"}
      data-terminal-height={height}
    >
      <div
        className="flex min-w-0 shrink-0 items-center justify-between gap-2 border-b border-[var(--terminal-border)] px-3 py-1.5"
        data-terminal-header="true"
      >
        <span className="min-w-0 text-[10px] font-bold tracking-[0.14em] break-words text-[var(--faint)] uppercase">
          {headerLabel}
        </span>
        <span className="shrink-0 text-[10px] tabular-nums text-[var(--subtle)]">
          {ptyLabel}
          {connectionState !== "connected" && interactive
            ? ` · ${t("interactive.unavailableShort")}`
            : ""}
        </span>
      </div>
      <div
        ref={hostRef}
        className="terminal-xterm w-full min-h-0 shrink-0 overflow-hidden px-2 py-2"
        style={{
          height: `${height}px`,
          minHeight: `${TERMINAL_HEIGHT_MIN}px`,
          maxHeight: `${maxHeight}px`,
        }}
        role="log"
        aria-label={label || t("terminal.aria", { id })}
        aria-live="off"
        tabIndex={0}
        data-terminal-host="true"
      />
      <div
        className="terminal-resize-handle group flex shrink-0 cursor-ns-resize touch-none select-none items-center justify-center border-t border-[var(--terminal-border)] bg-[var(--session-bg)] py-1"
        role="separator"
        aria-orientation="horizontal"
        aria-label={t("terminal.resizeAria")}
        aria-valuemin={TERMINAL_HEIGHT_MIN}
        aria-valuemax={maxHeight}
        aria-valuenow={height}
        aria-valuetext={t("terminal.resizeValue", { height })}
        title={t("terminal.resizeTitle")}
        tabIndex={0}
        data-terminal-resize="true"
        onPointerDown={onResizePointerDown}
        onKeyDown={onResizeKeyDown}
      >
        <span
          className="h-1 w-10 rounded-full bg-[var(--faint)] opacity-70 group-hover:opacity-100 group-focus-visible:opacity-100"
          aria-hidden="true"
        />
        <span className="sr-only">{t("terminal.resizeHint")}</span>
      </div>
    </div>
  );
}

export {
  TERMINAL_HEIGHT_DEFAULT,
  TERMINAL_HEIGHT_MIN,
  clampTerminalHeight,
  maxTerminalHeight,
} from "../utils/terminalHeight.js";
