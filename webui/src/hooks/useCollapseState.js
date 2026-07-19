import { useCallback, useState } from "react";

export function useCollapseState() {
  const [collapsedOwners, setCollapsedOwners] = useState(() => new Set());
  const [collapsedSessions, setCollapsedSessions] = useState(() => new Set());

  const toggleOwner = useCallback((key, open) => {
    setCollapsedOwners((current) => {
      const next = new Set(current);
      if (open) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const toggleSession = useCallback((sessionId, open) => {
    setCollapsedSessions((current) => {
      const next = new Set(current);
      if (open) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  }, []);

  const setAllExpanded = useCallback((open, groups, sessions) => {
    setCollapsedOwners(
      open ? new Set() : new Set(groups.map(([groupKey]) => groupKey)),
    );
    setCollapsedSessions(
      open ? new Set() : new Set(sessions.map((session) => session.session)),
    );
  }, []);

  return {
    collapsedOwners,
    collapsedSessions,
    toggleOwner,
    toggleSession,
    setAllExpanded,
  };
}
