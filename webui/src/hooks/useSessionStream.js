import { useCallback, useEffect, useRef, useState } from "react";
import {
  eventsWebSocketUrl,
  normalizeEventsMessage,
} from "../api.js";
import { sessionsSignature } from "../sessions.js";
import { WS_BACKOFF_MS } from "../utils/constants.js";
import { errorMessage } from "../utils/errors.js";
import {
  pushTerminalEntries,
  reconcileTerminalSessions,
} from "../utils/terminalFeeds.js";

/** @typedef {'initial' | 'connected' | 'disconnected' | 'retrying'} ConnectionState */

export function useSessionStream({ setNotice } = {}) {
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

  const clearStreamError = useCallback(() => {
    if (!setNotice) return;
    setNotice((current) =>
      current?.tone === "error" &&
      String(current.text || "").startsWith("实时通道")
        ? null
        : current,
    );
  }, [setNotice]);

  const reportStreamError = useCallback(
    (error) => {
      if (!setNotice) return;
      setNotice({
        tone: "error",
        text: `实时通道异常：${errorMessage(error)}（将自动重试）`,
      });
    },
    [setNotice],
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

    const applyMessage = (rawText) => {
      let parsed;
      try {
        parsed = JSON.parse(rawText);
      } catch (error) {
        throw new Error(`invalid events JSON: ${error?.message || error}`);
      }
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

      ws.onopen = () => {
        if (cancelled || socket !== ws) return;
        everConnected = true;
        attempt = 0;
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
        if (mountedRef.current) {
          setConnectionState(everConnected ? "disconnected" : "disconnected");
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
  }, [clearStreamError, reportStreamError]);

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
  };
}
