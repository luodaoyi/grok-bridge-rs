const WEB_UI_HEADER = { "X-Grok-Bridge-WebUI": "1" };
const DEFAULT_TIMEOUT_MS = 8000;

async function responseError(response) {
  try {
    const message = await response.text();
    return message || `${response.status} ${response.statusText}`;
  } catch {
    return `${response.status} ${response.statusText || "request failed"}`;
  }
}

function withTimeout(timeoutMs = DEFAULT_TIMEOUT_MS) {
  const controller = new AbortController();
  const timer = window.setTimeout(() => controller.abort(), timeoutMs);
  return {
    signal: controller.signal,
    clear: () => window.clearTimeout(timer),
  };
}

async function parseJson(response) {
  try {
    return await response.json();
  } catch (error) {
    throw new Error(`invalid JSON response: ${error?.message || error}`);
  }
}

export function normalizeSessions(data) {
  if (!Array.isArray(data)) {
    throw new Error("sessions payload is not an array");
  }
  return data.filter(
    (item) =>
      item &&
      typeof item === "object" &&
      typeof item.session === "string" &&
      item.session.length > 0,
  );
}

export async function getSessions({ timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
  const timeout = withTimeout(timeoutMs);
  try {
    const response = await fetch("/api/sessions", {
      cache: "no-store",
      signal: timeout.signal,
    });
    if (!response.ok) throw new Error(await responseError(response));
    return normalizeSessions(await parseJson(response));
  } finally {
    timeout.clear();
  }
}

export function normalizeVersionStatus(data) {
  if (!data || typeof data !== "object" || Array.isArray(data)) {
    throw new Error("version payload is not an object");
  }
  if (typeof data.current !== "string" || data.current.length === 0) {
    throw new Error("version payload is missing current");
  }
  const latest =
    typeof data.latest === "string" && data.latest.length > 0
      ? data.latest
      : null;
  const releaseUrl =
    typeof data.release_url === "string" && data.release_url.length > 0
      ? data.release_url
      : "https://github.com/luodaoyi/grok-bridge-rs/releases/latest";
  return {
    current: data.current,
    latest,
    update_available: Boolean(data.update_available) && latest != null,
    release_url: releaseUrl,
    checked_at_ms:
      typeof data.checked_at_ms === "number" ? data.checked_at_ms : null,
  };
}

export async function getVersionStatus({ timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
  const timeout = withTimeout(timeoutMs);
  try {
    const response = await fetch("/api/version", {
      cache: "no-store",
      signal: timeout.signal,
    });
    if (!response.ok) throw new Error(await responseError(response));
    return normalizeVersionStatus(await parseJson(response));
  } finally {
    timeout.clear();
  }
}

export async function closeSessionRequest(id, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
  const timeout = withTimeout(timeoutMs);
  try {
    const response = await fetch(
      `/api/sessions/${encodeURIComponent(id)}/close`,
      {
        method: "POST",
        headers: WEB_UI_HEADER,
        signal: timeout.signal,
      },
    );
    if (!response.ok) throw new Error(await responseError(response));
  } finally {
    timeout.clear();
  }
}

export async function closeOwnerRequest(owner, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
  const timeout = withTimeout(timeoutMs);
  try {
    const response = await fetch(
      `/api/owners/${encodeURIComponent(owner)}/close`,
      {
        method: "POST",
        headers: WEB_UI_HEADER,
        signal: timeout.signal,
      },
    );
    if (!response.ok) throw new Error(await responseError(response));
    return await parseJson(response);
  } finally {
    timeout.clear();
  }
}

export async function closeClientRequest(
  clientSessionId,
  { timeoutMs = DEFAULT_TIMEOUT_MS } = {},
) {
  const timeout = withTimeout(timeoutMs);
  try {
    const response = await fetch(
      `/api/clients/${encodeURIComponent(clientSessionId)}/close`,
      {
        method: "POST",
        headers: WEB_UI_HEADER,
        signal: timeout.signal,
      },
    );
    if (!response.ok) throw new Error(await responseError(response));
    return await parseJson(response);
  } finally {
    timeout.clear();
  }
}
