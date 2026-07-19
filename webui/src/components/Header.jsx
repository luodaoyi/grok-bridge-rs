import {
  ChevronsDownUp,
  ChevronsUpDown,
  ExternalLink,
  RefreshCw,
  Server,
  Wifi,
  WifiOff,
} from "lucide-react";
import { GITHUB_URL } from "../utils/constants.js";
import { secondaryButton } from "../utils/ui.js";
import { ThemeSwitcher } from "./ThemeSwitcher.jsx";

const CONNECTION_LABELS = {
  initial: "正在连接实时通道",
  connected: "实时通道已连接",
  disconnected: "实时通道已断开",
  retrying: "实时通道重连中",
};

function connectionTone(connectionState) {
  if (connectionState === "connected") return "ok";
  if (connectionState === "retrying" || connectionState === "initial") {
    return "warn";
  }
  return "error";
}

export function Header({
  version,
  connectionState = "initial",
  loading,
  onReconnect,
  onExpandAll,
  onCollapseAll,
}) {
  const tone = connectionTone(connectionState);
  const label =
    CONNECTION_LABELS[connectionState] ?? CONNECTION_LABELS.disconnected;
  const StatusIcon =
    connectionState === "connected"
      ? Wifi
      : connectionState === "disconnected"
        ? WifiOff
        : Server;

  return (
    <header className="border-b border-[var(--border)] pb-5">
      <div className="flex items-start justify-between gap-3 max-sm:flex-col">
        <div className="min-w-0">
          <p className="text-[11px] font-bold tracking-[0.18em] text-[var(--accent)]">
            GROK BRIDGE RUNTIME
          </p>
          <h1 className="mt-1.5 text-2xl leading-tight font-bold text-[var(--strong)] sm:text-[30px]">
            监督者与子代理控制台
          </h1>
          {version?.current ? (
            <p className="mt-1.5 text-[11px] text-[var(--faint)]">
              Runtime v{version.current}
            </p>
          ) : null}
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2.5 max-sm:w-full max-sm:justify-between">
          <span
            className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-semibold ${
              tone === "ok"
                ? "border-[var(--runtime-ok-border)] bg-[var(--runtime-ok-bg)] text-[var(--runtime-ok)]"
                : tone === "warn"
                  ? "border-[var(--runtime-warn-border)] bg-[var(--runtime-warn-bg)] text-[var(--runtime-warn)]"
                  : "border-[var(--runtime-error-border)] bg-[var(--runtime-error-bg)] text-[var(--runtime-error)]"
            }`}
            role="status"
            aria-live="polite"
            data-connection-state={connectionState}
          >
            <StatusIcon
              aria-hidden="true"
              size={14}
              className={
                connectionState === "retrying" || connectionState === "initial"
                  ? "animate-pulse motion-reduce:animate-none"
                  : ""
              }
            />
            {label}
          </span>
          <a
            className={`${secondaryButton} no-underline`}
            href={GITHUB_URL}
            target="_blank"
            rel="noreferrer"
            title="在 GitHub 打开 grok-bridge-rs"
          >
            <ExternalLink aria-hidden="true" size={14} />
            GitHub
          </a>
          <ThemeSwitcher />
        </div>
      </div>
      <p className="mt-3 max-w-4xl text-sm leading-6 text-[var(--muted)]">
        每个 Codex 对话是监督者，其下的 Grok 会话是可折叠的持久子代理。终端通过
        WebSocket 实时推送只读输出；关闭窗口不会影响 Runtime 中已持有的会话。
      </p>
      <div className="mt-4 flex flex-wrap gap-2">
        <button
          className={secondaryButton}
          type="button"
          disabled={loading}
          onClick={onReconnect}
          aria-busy={
            connectionState === "retrying" || connectionState === "initial"
          }
          aria-label="手动重连实时通道"
        >
          <RefreshCw
            aria-hidden="true"
            className={
              connectionState === "retrying" || connectionState === "initial"
                ? "animate-spin motion-reduce:animate-none"
                : ""
            }
            size={14}
          />
          重新连接
        </button>
        <button className={secondaryButton} type="button" onClick={onExpandAll}>
          <ChevronsUpDown aria-hidden="true" size={14} />
          全部展开
        </button>
        <button
          className={secondaryButton}
          type="button"
          onClick={onCollapseAll}
        >
          <ChevronsDownUp aria-hidden="true" size={14} />
          全部折叠
        </button>
      </div>
    </header>
  );
}
