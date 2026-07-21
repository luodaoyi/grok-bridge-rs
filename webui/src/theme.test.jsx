import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeSwitcher } from "./App.jsx";
import { I18nProvider } from "./i18n/index.js";
import { applyTheme, resolveTheme, THEME_KEY } from "./theme.js";

function mediaQuery(matches = false) {
  const listeners = new Set();
  return {
    matches,
    addEventListener: vi.fn((_, listener) => listeners.add(listener)),
    removeEventListener: vi.fn((_, listener) => listeners.delete(listener)),
    setMatches(value) {
      this.matches = value;
      for (const listener of listeners) listener({ matches: value });
    },
  };
}

describe("theme", () => {
  let container;
  let root;
  let query;

  beforeEach(() => {
    localStorage.clear();
    document.documentElement.dataset.theme = "auto";
    document.documentElement.dataset.resolvedTheme = "light";
    query = mediaQuery(false);
    vi.spyOn(window, "matchMedia").mockReturnValue(query);
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
  });

  it("resolves auto while leaving explicit themes unchanged", () => {
    expect(resolveTheme("auto", true)).toBe("dark");
    expect(resolveTheme("auto", false)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("persists manual choice and follows system changes in auto mode", async () => {
    await act(async () =>
      root.render(
        <I18nProvider initialLocale="zh-CN">
          <ThemeSwitcher />
        </I18nProvider>,
      ),
    );
    const trigger = container.querySelector("[data-theme-trigger]");
    expect(trigger).not.toBeNull();
    expect(trigger.textContent.trim()).toBe("");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    await act(async () => trigger.click());
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    const dark = container.querySelector('[data-theme-option="dark"]');
    await act(async () => dark.click());
    expect(localStorage.getItem(THEME_KEY)).toBe("dark");
    expect(document.documentElement.dataset.resolvedTheme).toBe("dark");
    expect(document.documentElement.dataset.bsTheme).toBe("dark");

    await act(async () => trigger.click());
    const auto = container.querySelector('[data-theme-option="auto"]');
    await act(async () => auto.click());
    expect(localStorage.getItem(THEME_KEY)).toBe("auto");
    await act(async () => query.setMatches(true));
    expect(document.documentElement.dataset.resolvedTheme).toBe("dark");
    expect(document.documentElement.dataset.bsTheme).toBe("dark");
    expect(query.addEventListener).toHaveBeenCalledWith(
      "change",
      expect.any(Function),
    );
  });

  it("normalizes invalid preferences", () => {
    applyTheme("invalid", query);
    expect(document.documentElement.dataset.theme).toBe("auto");
    expect(document.documentElement.dataset.bsTheme).toBe("light");
  });
});
