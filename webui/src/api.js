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
