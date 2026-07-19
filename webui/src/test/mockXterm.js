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
    /** Ordered operation log: ['write', data] | ['reset'] | ['resize', cols, rows] */
    this.ops = [];
    this.resetCount = 0;
    this.disposed = false;
    this.opened = false;
    this.handlers = { data: [], key: [] };
    this.addons = [];
    this._host = null;
    /** When true, write callbacks are queued until flushWriteCallback(s). */
    this.holdWriteCallbacks = false;
    /** @type {Array<() => void>} */
    this.pendingCallbacks = [];
    MockXTerm.instances.push(this);
  }

  open(host) {
    this.opened = true;
    this._host = host ?? null;
  }

  loadAddon(addon) {
    this.addons.push(addon);
    if (addon && typeof addon.activate === "function") {
      addon.activate(this);
    }
  }

  write(data, callback) {
    this.written.push(data);
    this.ops.push(["write", data]);
    if (typeof callback !== "function") return;
    if (this.holdWriteCallbacks) {
      this.pendingCallbacks.push(callback);
      return;
    }
    callback();
  }

  flushWriteCallback() {
    const callback = this.pendingCallbacks.shift();
    if (callback) callback();
  }

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
    this.ops.push(["resize", cols, rows]);
  }

  dispose() {
    this.disposed = true;
    this.handlers.data = [];
    this.handlers.key = [];
    for (const addon of this.addons) {
      if (addon && typeof addon.dispose === "function") {
        try {
          addon.dispose();
        } catch {
          /* ignore */
        }
      }
    }
  }

  onData(handler) {
    this.handlers.data.push(handler);
    return {
      dispose: () => {
        this.handlers.data = this.handlers.data.filter((h) => h !== handler);
      },
    };
  }

  onKey(handler) {
    this.handlers.key.push(handler);
    return {
      dispose: () => {
        this.handlers.key = this.handlers.key.filter((h) => h !== handler);
      },
    };
  }

  /** Test helper: fire onData as xterm would for typed/pasted text. */
  emitData(data) {
    for (const handler of [...this.handlers.data]) {
      handler(data);
    }
  }
}

export function installMockXterm(vi) {
  MockXTerm.reset();
  vi.doMock("@xterm/xterm", () => ({
    Terminal: MockXTerm,
  }));
  return MockXTerm;
}
