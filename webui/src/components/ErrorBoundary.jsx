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
          className="page page-center"
          role="alert"
        >
          <div className="container-tight py-4">
            <div className="card card-md">
              <div className="card-body text-center">
                <p className="subheader text-cyan">{t("error.brand")}</p>
                <h1 className="h2">{t("error.renderTitle")}</h1>
                <p className="text-secondary">
                  {String(this.state.error?.message || this.state.error)}
                </p>
                <button
                  className={secondaryButton}
                  type="button"
                  onClick={() => {
                    this.setState({ error: null });
                    window.location.reload();
                  }}
                >
                  {t("error.reload")}
                </button>
              </div>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
