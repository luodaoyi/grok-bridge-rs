/** Deterministic WebSocket mock for vitest. */
export class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  static instances = [];

  static reset() {
    MockWebSocket.instances = [];
  }

  constructor(url) {
    this.url = String(url);
    this.readyState = MockWebSocket.CONNECTING;
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
    this.sent = [];
    MockWebSocket.instances.push(this);
  }

  open() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.({ type: "open", target: this });
  }

  emitMessage(payload) {
    const data =
      typeof payload === "string" ? payload : JSON.stringify(payload);
    this.onmessage?.({ type: "message", data, target: this });
  }

  close() {
    if (
      this.readyState === MockWebSocket.CLOSED ||
      this.readyState === MockWebSocket.CLOSING
    ) {
      return;
    }
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ type: "close", target: this });
  }

  send(data) {
    this.sent.push(data);
  }
}

export function installMockWebSocket() {
  MockWebSocket.reset();
  globalThis.WebSocket = MockWebSocket;
  return MockWebSocket;
}
