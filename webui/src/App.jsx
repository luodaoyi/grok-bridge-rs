import { AppErrorBoundary } from "./components/ErrorBoundary.jsx";
import { AppShell } from "./components/AppShell.jsx";
import { ThemeSwitcher } from "./components/ThemeSwitcher.jsx";
import { I18nProvider } from "./i18n/index.js";

export default function App() {
  return (
    <I18nProvider>
      <AppErrorBoundary>
        <AppShell />
      </AppErrorBoundary>
    </I18nProvider>
  );
}

export { ThemeSwitcher };
