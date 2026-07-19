import { Component } from "react";
import { createTranslator, resolveLocale } from "../i18n/index.js";
import { secondaryButton } from "../utils/ui.js";

export class AppErrorBoundary extends Component {
  state = { error: null };

  static getDerivedStateFromError(error) {
    return { error };
  }

  componentDidCatch(error) {
    console.error("WebUI render error:", error);
  }

  render() {
    if (this.state.error) {
      const t = createTranslator(resolveLocale());
      return (
        <div
          className="mx-auto flex min-h-screen max-w-xl flex-col items-center justify-center px-4 py-16 text-center"
          role="alert"
        >
          <p className="text-xs font-bold tracking-[0.16em] text-[var(--accent)]">
            {t("error.brand")}
          </p>
          <h1 className="mt-2 text-lg font-bold text-[var(--strong)]">
            {t("error.renderTitle")}
          </h1>
          <p className="mt-2 text-sm text-[var(--muted)]">
            {String(this.state.error?.message || this.state.error)}
          </p>
          <button
            className={`${secondaryButton} mt-5`}
            type="button"
            onClick={() => {
              this.setState({ error: null });
              window.location.reload();
            }}
          >
            {t("error.reload")}
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
