/** Deterministic @xterm/xterm mock capturing writes/resets/options. */
export class MockXTerm {
  static instances = [];

  static reset() {
    MockXTerm.instances = [];
  }

  constructor(options = {}) {
    this.options = { ...options };
    this.rows = options.rows ?? 24;
    this.cols = options.cols ?? 80;
    this.written = [];
    /** Ordered operation log: ['write', data] | ['reset'] */
    this.ops = [];
    this.resetCount = 0;
    this.disposed = false;
    this.opened = false;
    this.handlers = { data: [], key: [] };
    /** When true, write callbacks are queued until flushWriteCallback(s). */
    this.holdWriteCallbacks = false;
    /** @type {Array<() => void>} */
    this.pendingCallbacks = [];
    MockXTerm.instances.push(this);
  }

  open() {
    this.opened = true;
  }

  /**
   * Mirrors xterm write(data, callback): parser work is async relative to
   * callers; callback fires when that write is complete.
   */
  write(data, callback) {
    this.written.push(data);
    this.ops.push(["write", data]);
    if (typeof callback !== "function") return;
    if (this.holdWriteCallbacks) {
      this.pendingCallbacks.push(callback);
      return;
    }
    // Default: complete synchronously so existing tests stay simple.
    callback();
  }

  /** Fire the oldest pending write callback (FIFO). */
  flushWriteCallback() {
    const callback = this.pendingCallbacks.shift();
    if (callback) callback();
  }

  /** Fire all pending write callbacks in order. */
  flushAllWriteCallbacks() {
    while (this.pendingCallbacks.length > 0) {
      this.flushWriteCallback();
    }
  }

  reset() {
    this.resetCount += 1;
    this.ops.push(["reset"]);
    this.written = [];
  }

  resize(cols, rows) {
    this.cols = cols;
    this.rows = rows;
  }

  dispose() {
    this.disposed = true;
  }

  // Intentionally no real event system — tests assert these are never used.
  onData(handler) {
    this.handlers.data.push(handler);
    return { dispose() {} };
  }

  onKey(handler) {
    this.handlers.key.push(handler);
    return { dispose() {} };
  }
}

export function installMockXterm(vi) {
  MockXTerm.reset();
  vi.doMock("@xterm/xterm", () => ({
    Terminal: MockXTerm,
  }));
  return MockXTerm;
}
