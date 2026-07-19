import {
  ChevronsDownUp,
  ChevronsUpDown,
  ExternalLink,
  RefreshCw,
  Server,
  Wifi,
  WifiOff,
} from "lucide-react";
import { useI18n } from "../i18n/index.js";
import { GITHUB_URL } from "../utils/constants.js";
import { secondaryButton } from "../utils/ui.js";
import { InteractiveToggle } from "./InteractiveToggle.jsx";
import { LanguageSwitcher } from "./LanguageSwitcher.jsx";
import { ThemeSwitcher } from "./ThemeSwitcher.jsx";

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
  interactive = false,
  onInteractiveChange,
  onReconnect,
  onExpandAll,
  onCollapseAll,
}) {
  const { t } = useI18n();
  const tone = connectionTone(connectionState);
  const connectionKey = {
    initial: "connection.initial",
    connected: "connection.connected",
    disconnected: "connection.disconnected",
    retrying: "connection.retrying",
  }[connectionState];
  const label = t(connectionKey ?? "connection.disconnected");
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
            {t("app.brand")}
          </p>
          <h1 className="mt-1.5 text-2xl leading-tight font-bold text-[var(--strong)] sm:text-[30px]">
            {t("app.title")}
          </h1>
          {version?.current ? (
            <p className="mt-1.5 text-[11px] text-[var(--faint)]">
              {t("app.runtimeVersion", { version: version.current })}
            </p>
          ) : null}
        </div>
        <div className="flex min-w-0 flex-wrap items-center justify-end gap-2.5 max-sm:w-full max-sm:justify-between">
          <span
            className={`inline-flex max-w-full items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-semibold break-words ${
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
              className={`shrink-0 ${
                connectionState === "retrying" || connectionState === "initial"
                  ? "animate-pulse motion-reduce:animate-none"
                  : ""
              }`}
            />
            <span className="min-w-0">{label}</span>
          </span>
          <a
            className={`${secondaryButton} no-underline`}
            href={GITHUB_URL}
            target="_blank"
            rel="noreferrer"
            title={t("app.githubTitle")}
          >
            <ExternalLink aria-hidden="true" size={14} />
            {t("app.github")}
          </a>
          <InteractiveToggle
            interactive={interactive}
            connectionState={connectionState}
            onChange={onInteractiveChange}
          />
          <LanguageSwitcher />
          <ThemeSwitcher />
        </div>
      </div>
      <p className="mt-3 max-w-4xl text-sm leading-6 text-[var(--muted)]">
        {t("app.subtitle")}
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
          aria-label={t("connection.reconnectAria")}
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
          {t("connection.reconnect")}
        </button>
        <button className={secondaryButton} type="button" onClick={onExpandAll}>
          <ChevronsUpDown aria-hidden="true" size={14} />
          {t("header.expandAll")}
        </button>
        <button
          className={secondaryButton}
          type="button"
          onClick={onCollapseAll}
        >
          <ChevronsDownUp aria-hidden="true" size={14} />
          {t("header.collapseAll")}
        </button>
      </div>
    </header>
  );
}
