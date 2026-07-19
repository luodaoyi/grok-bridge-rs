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
  return (
    <span
      className={`badge ${activityClass[activity] ?? activityClass.unknown} inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-[11px] font-bold tracking-tight ${className}`}
      title={phase ? `PTY 阶段：${phase}` : undefined}
    >
      {activityLabel(activity)}
    </span>
  );
}

export function LifecycleBadge({ clientState, className = "" }) {
  const lifecycle = clientLifecycle(clientState);
  return (
    <span
      className={`badge ${lifecycleClass[lifecycle] ?? lifecycleClass.unknown} inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-[11px] font-bold tracking-tight ${className}`}
      title={clientLifecycleLabel(clientState)}
    >
      {clientLifecycleLabel(clientState)}
    </span>
  );
}
