import { useI18n } from "../i18n/index.js";
import {
  activityLabel,
  clientLifecycle,
  clientLifecycleLabel,
} from "../sessions.js";

const activityClass = {
  working: "badge-working",
  waiting: "badge-waiting",
  done: "badge-done",
  stopped: "badge-stopped",
  unknown: "badge-unknown",
};

const lifecycleClass = {
  connected: "badge-connected",
  disconnected: "badge-disconnected",
  cleanup: "badge-cleanup",
  unmanaged: "badge-unmanaged",
  unknown: "badge-unknown",
};

export function ActivityBadge({ activity, phase, className = "" }) {
  const { t } = useI18n();
  return (
    <span
      className={`badge ${activityClass[activity] ?? activityClass.unknown} ${className}`}
      title={phase ? t("badge.phase", { phase }) : undefined}
    >
      {activityLabel(activity, t)}
    </span>
  );
}

export function LifecycleBadge({ clientState, className = "" }) {
  const { t } = useI18n();
  const lifecycle = clientLifecycle(clientState);
  return (
    <span
      className={`badge ${lifecycleClass[lifecycle] ?? lifecycleClass.unknown} ${className}`}
      title={clientLifecycleLabel(clientState, t)}
    >
      {clientLifecycleLabel(clientState, t)}
    </span>
  );
}
