/** Deterministic @xterm/addon-fit mock. */
export class MockFitAddon {
  static instances = [];

  static reset() {
    MockFitAddon.instances = [];
  }

  constructor() {
    this.fitCount = 0;
    this.disposed = false;
    this.terminal = null;
    this.fitCalls = [];
    MockFitAddon.instances.push(this);
  }

  activate(terminal) {
    this.terminal = terminal;
  }

  fit() {
    if (this.disposed) return;
    this.fitCount += 1;
    this.fitCalls.push(Date.now());
    // Mirror FitAddon: derive a plausible grid from the host box when present.
    const host = this.terminal?._host;
    if (host && host.clientWidth > 0 && host.clientHeight > 0) {
      const cols = Math.max(2, Math.floor(host.clientWidth / 9));
      const rows = Math.max(1, Math.floor(host.clientHeight / 17));
      this.terminal.resize(cols, rows);
    }
  }

  dispose() {
    this.disposed = true;
  }
}
