import { AppErrorBoundary } from "./components/ErrorBoundary.jsx";
import { AppShell } from "./components/AppShell.jsx";
import { ThemeSwitcher } from "./components/ThemeSwitcher.jsx";

export default function App() {
  return (
    <AppErrorBoundary>
      <AppShell />
    </AppErrorBoundary>
  );
}

export { ThemeSwitcher };
