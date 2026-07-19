import { useEffect, useMemo, useState } from "react";
import { applyTheme, readTheme } from "../theme.js";

export function useTheme() {
  const [theme, setTheme] = useState(readTheme);
  const mediaQuery = useMemo(
    () => window.matchMedia("(prefers-color-scheme: dark)"),
    [],
  );

  useEffect(() => {
    applyTheme(theme, mediaQuery);
  }, [mediaQuery, theme]);

  useEffect(() => {
    const handleChange = () => {
      if (document.documentElement.dataset.theme === "auto") {
        applyTheme("auto", mediaQuery);
      }
    };
    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [mediaQuery]);

  return { theme, setTheme };
}
