import {
  ChevronsDownUp,
  ChevronsUpDown,
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

function GithubLogo({ size = 14 }) {
  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      focusable="false"
    >
      <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.084-.729.084-.729 1.205.084 1.838 1.237 1.838 1.237 1.07 1.835 2.809 1.305 3.495.998.108-.776.418-1.305.762-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23a11.5 11.5 0 0 1 3.003-.404c1.018.005 2.045.138 3.003.404 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.435.375.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
    </svg>
  );
}

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
    <>
      <header className="navbar navbar-expand-md d-print-none border-bottom">
        <div className="container-xl header-bar">
          <div className="navbar-brand navbar-brand-autodark p-0">
            <div className="brand-mark" aria-hidden="true">
              <Server size={20} />
            </div>
            <div className="brand-copy">
              <span className="brand-kicker">{t("app.brand")}</span>
              <span className="brand-title">{t("app.title")}</span>
              {version?.current ? (
                <span className="brand-version">
                  {t("app.runtimeVersion", { version: version.current })}
                </span>
              ) : null}
            </div>
          </div>
          <div className="header-controls">
          <span
            className={`badge connection-badge connection-${tone}`}
            role="status"
            aria-live="polite"
            data-connection-state={connectionState}
          >
            <StatusIcon
              aria-hidden="true"
              size={14}
              className={
                connectionState === "retrying" || connectionState === "initial"
                  ? "connection-pulse"
                  : ""
              }
            />
            <span>{label}</span>
          </span>
          <a
            className={secondaryButton}
            href={GITHUB_URL}
            target="_blank"
            rel="noreferrer"
            title={t("app.githubTitle")}
          >
            <GithubLogo />
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
      </header>
      <div className="page-header d-print-none">
        <div className="container-xl">
          <p className="page-description text-secondary">{t("app.subtitle")}</p>
          <div className="btn-list header-actions">
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
                    ? "icon-spin"
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
            <button className={secondaryButton} type="button" onClick={onCollapseAll}>
              <ChevronsDownUp aria-hidden="true" size={14} />
              {t("header.collapseAll")}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
