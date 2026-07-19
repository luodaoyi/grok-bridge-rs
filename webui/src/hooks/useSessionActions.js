import { useCallback } from "react";
import {
  closeClientRequest,
  closeOwnerRequest,
  closeSessionRequest,
} from "../api.js";
import { errorMessage } from "../utils/errors.js";

/**
 * Close actions POST to the Runtime and rely on the WebSocket push for state.
 * They intentionally do not force a GET /api/sessions refresh.
 */
export function useSessionActions({ loadingRef, setLoading, setNotice }) {
  const closeSession = useCallback(
    async (id) => {
      if (loadingRef.current) return;
      if (!window.confirm(`确认关闭 ${id} 及其 Grok 进程？`)) return;
      loadingRef.current = true;
      setLoading(true);
      try {
        await closeSessionRequest(id);
        setNotice({
          tone: "info",
          text: `已关闭 Grok 会话 ${id}。实时通道将推送更新。`,
        });
      } catch (error) {
        setNotice({
          tone: "error",
          text: `关闭失败：${errorMessage(error)}`,
        });
      } finally {
        loadingRef.current = false;
        setLoading(false);
      }
    },
    [loadingRef, setLoading, setNotice],
  );

  const closeGroup = useCallback(
    async (owner, clientSessionId, count) => {
      if (loadingRef.current) return;
      const displayOwner = owner ?? clientSessionId;
      if (
        !window.confirm(
          `确认关闭 Codex“${displayOwner}”下的全部 ${count} 个 Grok 会话？`,
        )
      ) {
        return;
      }
      loadingRef.current = true;
      setLoading(true);
      try {
        const result = clientSessionId
          ? await closeClientRequest(clientSessionId)
          : await closeOwnerRequest(owner);
        if (result.matched === 0) {
          setNotice({
            tone: "info",
            text: "该 Codex 分组已没有活跃 Grok 会话。",
          });
        } else if (
          result.failures?.length ||
          result.closed !== result.matched
        ) {
          setNotice({
            tone: "error",
            text: `已关闭 ${result.closed}/${result.matched} 个会话；失败：${(result.failures || []).join("、")}`,
          });
        } else {
          setNotice({
            tone: "info",
            text: `已关闭 Codex“${displayOwner}”下的全部 ${result.closed} 个 Grok 会话。实时通道将推送更新。`,
          });
        }
      } catch (error) {
        setNotice({
          tone: "error",
          text: `关闭失败：${errorMessage(error)}`,
        });
      } finally {
        loadingRef.current = false;
        setLoading(false);
      }
    },
    [loadingRef, setLoading, setNotice],
  );

  return { closeSession, closeGroup };
}
