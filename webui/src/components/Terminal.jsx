import { useEffect, useRef } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { decodeBase64ToUint8Array } from "../utils/base64.js";
import { subscribeTerminal } from "../utils/terminalFeeds.js";
import {
  TERMINAL_FONT_FAMILY,
  readTerminalTheme,
} from "../utils/terminalTheme.js";

const DEFAULT_ROWS = 24;
const DEFAULT_COLS = 80;

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
      // Reset only at this entry's exact queue position, then write snapshot.
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
      // Pending write callbacks may still fire; pump/process check disposed.
    },
    /** Test helper */
    get pending() {
      return queue.length;
    },
    /** Test helper */
    get isBusy() {
      return busy;
    },
  };
}

/**
 * Read-only xterm.js terminal driven exclusively by the WebSocket terminal feed.
 * Never registers onData/onKey and never sends input to the Runtime.
 */
export function Terminal({ id, rows, cols, label }) {
  const hostRef = useRef(null);
  const termRef = useRef(null);

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

    term.open(host);
    termRef.current = term;

    const writeQueue = createTerminalWriteQueue(term);
    const unsubscribe = subscribeTerminal(id, (entry) => {
      writeQueue.enqueue(entry);
    });

    return () => {
      unsubscribe();
      writeQueue.dispose();
      termRef.current = null;
      term.dispose();
    };
    // Mount once per session id; rows/cols and theme are applied via separate effects.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional mount-per-id
  }, [id]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    const nextRows = safeRows(rows);
    const nextCols = safeCols(cols);
    if (term.rows !== nextRows || term.cols !== nextCols) {
      term.resize(nextCols, nextRows);
    }
  }, [rows, cols]);

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

  return (
    <div
      className="terminal-shell relative w-full overflow-hidden rounded-xl border border-[var(--terminal-border)] bg-[var(--terminal-bg)] shadow-[inset_0_1px_0_var(--terminal-inset)] focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-[var(--focus)]"
      data-terminal={id}
      data-readonly="true"
    >
      <div className="flex items-center justify-between gap-2 border-b border-[var(--terminal-border)] px-3 py-1.5">
        <span className="text-[10px] font-bold tracking-[0.14em] text-[var(--faint)] uppercase">
          终端 · 只读实时
        </span>
        <span className="text-[10px] tabular-nums text-[var(--subtle)]">
          {safeCols(cols)}×{safeRows(rows)}
        </span>
      </div>
      <div
        ref={hostRef}
        className="terminal-xterm max-h-[min(460px,55vh)] min-h-36 w-full overflow-hidden px-2 py-2"
        role="log"
        aria-label={label || `${id} 的终端画面`}
        aria-live="off"
        tabIndex={0}
      />
    </div>
  );
}
