import { useCallback, useEffect, useRef, useState } from "react";
import {
  eventsWebSocketUrl,
  normalizeEventsMessage,
} from "../api.js";
import { useI18n } from "../i18n/index.js";
import { sessionsSignature } from "../sessions.js";
import { WS_BACKOFF_MS } from "../utils/constants.js";
import { errorMessage } from "../utils/errors.js";
import {
  pushTerminalEntries,
  reconcileTerminalSessions,
} from "../utils/terminalFeeds.js";

/** @typedef {'initial' | 'connected' | 'disconnected' | 'retrying'} ConnectionState */

/** Stable client-side I/O error codes (never shown raw in the UI). */
export const CLIENT_IO_ERROR = Object.freeze({
  DISCONNECTED: "disconnected",
  INVALID_PAYLOAD: "invalid_payload",
  SEND_FAILED: "send_failed",
});

const CLIENT_IO_ERROR_KEYS = Object.freeze({
  [CLIENT_IO_ERROR.DISCONNECTED]: "interactive.disconnected",
  [CLIENT_IO_ERROR.INVALID_PAYLOAD]: "interactive.invalidPayload",
  [CLIENT_IO_ERROR.SEND_FAILED]: "interactive.sendFailed",
});

let requestSeq = 0;

function nextRequestId() {
  requestSeq += 1;
  return `webui-${requestSeq}`;
}

export function useSessionStream({ setNotice } = {}) {
  const { t } = useI18n();
  const tRef = useRef(t);
  tRef.current = t;

  const [sessions, setSessions] = useState([]);
  const [loading, setLoading] = useState(false);
  const [connectionState, setConnectionState] = useState(
    /** @type {ConnectionState} */ ("initial"),
  );
  const [lastUpdated, setLastUpdated] = useState(null);
  const signatureRef = useRef(null);
  const loadingRef = useRef(false);
  const mountedRef = useRef(true);
  const reconnectRef = useRef(() => {});
  /** @type {import('react').MutableRefObject<WebSocket | null>} */
  const socketRef = useRef(null);

  const clearStreamError = useCallback(() => {
    if (!setNotice) return;
    setNotice((current) =>
      current?.tone === "error" && current?.kind === "stream" ? null : current,
    );
  }, [setNotice]);

  const reportStreamError = useCallback(
    (error) => {
      if (!setNotice) return;
      const translate = tRef.current;
      setNotice({
        tone: "error",
        kind: "stream",
        text: translate("stream.error", {
          detail: errorMessage(error, translate),
        }),
      });
    },
    [setNotice],
  );

  /** Map a stable client error code to a fully localized Notice (no English leak). */
  const reportClientIoError = useCallback(
    (code) => {
      if (!setNotice) return;
      const translate = tRef.current;
      const key =
        CLIENT_IO_ERROR_KEYS[code] ?? "interactive.unavailable";
      setNotice({
        tone: "error",
        kind: "input",
        text: translate(key),
      });
    },
    [setNotice],
  );

  /**
   * Backend input_result / resize_result detail may stay after the localized
   * prefix from interactive.error.
   */
  const reportBackendIoError = useCallback(
    (detail) => {
      if (!setNotice) return;
      const translate = tRef.current;
      setNotice({
        tone: "error",
        kind: "input",
        text: translate("interactive.error", {
          detail: detail || translate("error.unknown"),
        }),
      });
    },
    [setNotice],
  );

  /**
   * Send a JSON command on the live events socket.
   * Never buffers: if the socket is not OPEN, fails immediately.
   * @returns {{ ok: true, id: string } | { ok: false, error: string }}
   *   `error` is a stable CLIENT_IO_ERROR code, never a free-form English string.
   */
  const sendClientCommand = useCallback((message) => {
    const ws = socketRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return { ok: false, error: CLIENT_IO_ERROR.DISCONNECTED };
    }
    const id = message.id || nextRequestId();
    const payload = { ...message, id };
    try {
      ws.send(JSON.stringify(payload));
      return { ok: true, id };
    } catch {
      return { ok: false, error: CLIENT_IO_ERROR.SEND_FAILED };
    }
  }, []);

  const sendTerminalInput = useCallback(
    (session, dataBase64) => {
      if (!session || !dataBase64) {
        reportClientIoError(CLIENT_IO_ERROR.INVALID_PAYLOAD);
        return { ok: false, error: CLIENT_IO_ERROR.INVALID_PAYLOAD };
      }
      const result = sendClientCommand({
        type: "terminal_input",
        session,
        data_base64: dataBase64,
      });
      if (!result.ok) {
        reportClientIoError(result.error);
      }
      return result;
    },
    [reportClientIoError, sendClientCommand],
  );

  const sendTerminalResize = useCallback(
    (session, cols, rows) => {
      if (!session) {
        reportClientIoError(CLIENT_IO_ERROR.INVALID_PAYLOAD);
        return { ok: false, error: CLIENT_IO_ERROR.INVALID_PAYLOAD };
      }
      const result = sendClientCommand({
        type: "terminal_resize",
        session,
        cols,
        rows,
      });
      if (!result.ok) {
        reportClientIoError(result.error);
      }
      return result;
    },
    [reportClientIoError, sendClientCommand],
  );

  useEffect(() => {
    mountedRef.current = true;
    let cancelled = false;
    let socket = null;
    let retryTimer = 0;
    let attempt = 0;
    let everConnected = false;

    const clearRetry = () => {
      if (retryTimer) {
        window.clearTimeout(retryTimer);
        retryTimer = 0;
      }
    };

    const applySessionsMessage = (parsed) => {
      const message = normalizeEventsMessage(parsed);
      if (!mountedRef.current || cancelled) return;

      const signature = sessionsSignature(message.sessions);
      if (signature !== signatureRef.current) {
        signatureRef.current = signature;
        setSessions(message.sessions);
      }

      pushTerminalEntries(message.terminals);
      reconcileTerminalSessions(
        new Set(message.sessions.map((session) => session.session)),
      );
      setLastUpdated(new Date());
      clearStreamError();
    };

    const applyMessage = (rawText) => {
      let parsed;
      try {
        parsed = JSON.parse(rawText);
      } catch (error) {
        throw new Error(`invalid events JSON: ${error?.message || error}`);
      }

      // Command results must not go through sessions normalization.
      if (
        parsed &&
        typeof parsed === "object" &&
        (parsed.type === "input_result" || parsed.type === "resize_result")
      ) {
        if (parsed.ok === false) {
          reportBackendIoError(
            typeof parsed.error === "string" ? parsed.error : null,
          );
        }
        return;
      }

      applySessionsMessage(parsed);
    };

    const scheduleReconnect = () => {
      if (cancelled) return;
      clearRetry();
      const delay =
        WS_BACKOFF_MS[Math.min(attempt, WS_BACKOFF_MS.length - 1)] ?? 30000;
      attempt += 1;
      if (mountedRef.current) setConnectionState("retrying");
      retryTimer = window.setTimeout(() => {
        retryTimer = 0;
        connect();
      }, delay);
    };

    const connect = () => {
      if (cancelled) return;
      clearRetry();

      if (socket) {
        try {
          socket.onopen = null;
          socket.onmessage = null;
          socket.onerror = null;
          socket.onclose = null;
          if (
            socket.readyState === WebSocket.OPEN ||
            socket.readyState === WebSocket.CONNECTING
          ) {
            socket.close();
          }
        } catch {
          /* ignore close races */
        }
        socket = null;
        socketRef.current = null;
      }

      if (mountedRef.current) {
        setConnectionState(everConnected ? "retrying" : "initial");
      }

      let ws;
      try {
        ws = new WebSocket(eventsWebSocketUrl());
      } catch (error) {
        reportStreamError(error);
        scheduleReconnect();
        return;
      }
      socket = ws;
      socketRef.current = ws;

      ws.onopen = () => {
        if (cancelled || socket !== ws) return;
        everConnected = true;
        attempt = 0;
        socketRef.current = ws;
        if (mountedRef.current) setConnectionState("connected");
        clearStreamError();
      };

      ws.onmessage = (event) => {
        if (cancelled || socket !== ws) return;
        try {
          applyMessage(String(event.data ?? ""));
        } catch (error) {
          reportStreamError(error);
        }
      };

      ws.onerror = () => {
        // Browsers surface details via onclose; keep a soft signal only.
        if (cancelled || socket !== ws) return;
      };

      ws.onclose = () => {
        if (cancelled || socket !== ws) return;
        socket = null;
        if (socketRef.current === ws) socketRef.current = null;
        if (mountedRef.current) {
          setConnectionState("disconnected");
        }
        scheduleReconnect();
      };
    };

    reconnectRef.current = () => {
      if (cancelled) return;
      attempt = 0;
      clearRetry();
      if (mountedRef.current) {
        setConnectionState(everConnected ? "retrying" : "initial");
      }
      connect();
    };

    connect();

    return () => {
      cancelled = true;
      mountedRef.current = false;
      clearRetry();
      reconnectRef.current = () => {};
      socketRef.current = null;
      if (socket) {
        try {
          socket.onopen = null;
          socket.onmessage = null;
          socket.onerror = null;
          socket.onclose = null;
          socket.close();
        } catch {
          /* ignore */
        }
        socket = null;
      }
    };
  }, [clearStreamError, reportBackendIoError, reportStreamError]);

  const reconnect = useCallback(() => {
    reconnectRef.current();
  }, []);

  return {
    sessions,
    loading,
    connectionState,
    connected: connectionState === "connected",
    lastUpdated,
    reconnect,
    loadingRef,
    setLoading,
    sendTerminalInput,
    sendTerminalResize,
  };
}
