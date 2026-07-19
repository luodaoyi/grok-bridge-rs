/**
 * Intl helpers scoped to the active UI locale.
 * Numbers and relative/remaining times always go through Intl.
 */

export function formatNumber(value, locale) {
  try {
    return new Intl.NumberFormat(locale).format(value);
  } catch {
    return String(value);
  }
}

export function formatClock(date, locale) {
  const value = date instanceof Date ? date : new Date(date);
  try {
    return new Intl.DateTimeFormat(locale, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }).format(value);
  } catch {
    return value.toLocaleTimeString();
  }
}

/** Relative age for "last updated" (past). */
export function formatAge(updatedAt, now = Date.now(), locale = "en") {
  const seconds = Math.max(0, Math.floor((now - updatedAt) / 1000));
  try {
    const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "always" });
    if (seconds < 60) return rtf.format(-seconds, "second");
    if (seconds < 3600) return rtf.format(-Math.floor(seconds / 60), "minute");
    return rtf.format(-Math.floor(seconds / 3600), "hour");
  } catch {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    return `${Math.floor(seconds / 3600)}h`;
  }
}

function unitFormat(value, unit, locale) {
  try {
    return new Intl.NumberFormat(locale, {
      style: "unit",
      unit,
      unitDisplay: "long",
    }).format(value);
  } catch {
    return `${value} ${unit}`;
  }
}

/** Remaining duration until a deadline (countdown, coarser units). */
export function formatRemaining(deadline, now = Date.now(), locale = "en") {
  const seconds = Math.max(0, Math.ceil((deadline - now) / 1000));
  if (seconds < 60) return unitFormat(seconds, "second", locale);
  if (seconds < 3600) return unitFormat(Math.ceil(seconds / 60), "minute", locale);
  return unitFormat(Math.ceil(seconds / 3600), "hour", locale);
}

/**
 * Second-precise countdown for orphan auto-close (local clock).
 * Always includes the seconds unit so the live timer is exact.
 */
export function formatCountdown(deadline, now = Date.now(), locale = "en") {
  const totalSeconds = Math.max(0, Math.ceil((deadline - now) / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts = [];
  if (hours > 0) parts.push(unitFormat(hours, "hour", locale));
  if (hours > 0 || minutes > 0) parts.push(unitFormat(minutes, "minute", locale));
  parts.push(unitFormat(seconds, "second", locale));
  return parts.join(" ");
}

/**
 * Format a known policy duration from wire (lease/grace ms).
 * Returns null when the value is missing or invalid — never invent defaults.
 */
export function formatDurationMs(ms, locale = "en") {
  if (typeof ms !== "number" || !Number.isFinite(ms) || ms < 0) return null;
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return unitFormat(seconds, "second", locale);
  if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    const rem = seconds % 60;
    if (rem === 0) return unitFormat(minutes, "minute", locale);
    return `${unitFormat(minutes, "minute", locale)} ${unitFormat(rem, "second", locale)}`;
  }
  const hours = Math.floor(seconds / 3600);
  const remMin = Math.floor((seconds % 3600) / 60);
  if (remMin === 0) return unitFormat(hours, "hour", locale);
  return `${unitFormat(hours, "hour", locale)} ${unitFormat(remMin, "minute", locale)}`;
}
