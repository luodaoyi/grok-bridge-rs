import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  I18nProvider,
  LOCALE_LABELS,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES,
} from "../i18n/index.js";
import { LanguageSwitcher } from "./LanguageSwitcher.jsx";

const LANG_MENU_VARS = [
  "--lang-menu-bg",
  "--lang-menu-text",
  "--lang-menu-border",
  "--lang-menu-selected-bg",
  "--lang-menu-selected-text",
  "--lang-menu-active-bg",
  "--lang-menu-active-text",
  "--lang-menu-check",
];

const cssPath = join(dirname(fileURLToPath(import.meta.url)), "../index.css");
const indexCss = readFileSync(cssPath, "utf8");

function extractThemeBlock(source, selector) {
  const start = source.indexOf(selector);
  if (start < 0) return "";
  const open = source.indexOf("{", start);
  if (open < 0) return "";
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  return "";
}

function readVarsFromBlock(block) {
  /** @type {Record<string, string>} */
  const vars = {};
  for (const name of LANG_MENU_VARS) {
    const match = block.match(
      new RegExp(`${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*:\\s*([^;]+);`),
    );
    if (match) vars[name] = match[1].trim();
  }
  return vars;
}

function relativeLuminance(hex) {
  const raw = hex.replace("#", "").trim();
  if (raw.length !== 6) return null;
  const channels = [0, 2, 4].map((offset) => {
    const value = Number.parseInt(raw.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928
      ? value / 12.92
      : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrastRatio(fgHex, bgHex) {
  const fg = relativeLuminance(fgHex);
  const bg = relativeLuminance(bgHex);
  if (fg == null || bg == null) return 0;
  const lighter = Math.max(fg, bg);
  const darker = Math.min(fg, bg);
  return (lighter + 0.05) / (darker + 0.05);
}

describe("LanguageSwitcher", () => {
  let container;
  let root;

  beforeEach(() => {
    localStorage.clear();
    document.documentElement.dataset.resolvedTheme = "dark";
    document.documentElement.dataset.theme = "dark";
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
  });

  async function renderSwitcher(initialLocale = "en") {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale={initialLocale}>
          <LanguageSwitcher />
        </I18nProvider>,
      );
    });
  }

  function trigger() {
    return container.querySelector("[data-language-trigger]");
  }

  function menu() {
    return container.querySelector("[data-language-menu]");
  }

  it("does not use a native select and exposes listbox semantics", async () => {
    await renderSwitcher("zh-CN");
    expect(container.querySelector("select")).toBeNull();
    const button = trigger();
    expect(button).not.toBeNull();
    expect(button.getAttribute("aria-haspopup")).toBe("listbox");
    expect(button.getAttribute("aria-expanded")).toBe("false");
    expect(button.textContent.trim()).toBe("");
    expect(button.getAttribute("title")).toContain(LOCALE_LABELS["zh-CN"]);

    await act(async () => button.click());
    expect(button.getAttribute("aria-expanded")).toBe("true");
    expect(menu()?.getAttribute("role")).toBe("listbox");
    const options = container.querySelectorAll('[role="option"]');
    expect(options).toHaveLength(SUPPORTED_LOCALES.length);
    const optionCodes = [...options].map((option) =>
      option.getAttribute("data-language-option"),
    );
    expect(optionCodes).toEqual(SUPPORTED_LOCALES);
    for (const code of SUPPORTED_LOCALES) {
      const option = container.querySelector(`[data-language-option="${code}"]`);
      expect(option).not.toBeNull();
      expect(option.textContent).toContain(LOCALE_LABELS[code]);
    }
    const selected = container.querySelector(
      '[data-language-option="zh-CN"]',
    );
    expect(selected?.getAttribute("aria-selected")).toBe("true");
  });

  it("defines readable language-menu color tokens for dark and light themes", async () => {
    await renderSwitcher("en");
    await act(async () => trigger().click());
    expect(menu()?.classList.contains("lang-menu")).toBe(true);
    expect(container.querySelector(".lang-menu-option")).not.toBeNull();
    expect(container.querySelector(".lang-menu-option-selected")).not.toBeNull();
    expect(indexCss).toMatch(/\.lang-menu,\s*\.theme-menu\s*\{/);
    expect(indexCss).toContain(".lang-menu-option-selected");
    expect(indexCss).toContain(".lang-menu-option-active");

    // :root is the dark default; light overrides live under data-resolved-theme.
    const darkBlock = extractThemeBlock(indexCss, ":root {");
    const lightBlock = extractThemeBlock(
      indexCss,
      ':root[data-resolved-theme="light"]',
    );
    const darkVars = readVarsFromBlock(darkBlock);
    const lightVars = readVarsFromBlock(lightBlock);

    for (const name of LANG_MENU_VARS) {
      expect(darkVars[name], `dark ${name}`).toBeTruthy();
      expect(lightVars[name], `light ${name}`).toBeTruthy();
      expect(darkVars[name]).not.toBe(lightVars[name]);
    }

    for (const [theme, vars] of [
      ["dark", darkVars],
      ["light", lightVars],
    ]) {
      const pairs = [
        [vars["--lang-menu-text"], vars["--lang-menu-bg"]],
        [vars["--lang-menu-selected-text"], vars["--lang-menu-selected-bg"]],
        [vars["--lang-menu-active-text"], vars["--lang-menu-active-bg"]],
      ];
      for (const [fg, bg] of pairs) {
        expect(
          contrastRatio(fg, bg),
          `${theme} contrast ${fg} on ${bg}`,
        ).toBeGreaterThanOrEqual(3);
      }
      // Body text on menu surface should meet AA for normal text.
      expect(
        contrastRatio(vars["--lang-menu-text"], vars["--lang-menu-bg"]),
        `${theme} body text`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("supports keyboard open, arrow navigation, Enter select, Escape close", async () => {
    await renderSwitcher("en");
    const button = trigger();
    button.focus();

    await act(async () => {
      button.dispatchEvent(
        new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }),
      );
    });
    expect(menu()).not.toBeNull();
    expect(button.getAttribute("aria-expanded")).toBe("true");

    const list = menu();
    await act(async () => {
      list.dispatchEvent(
        new KeyboardEvent("keydown", { key: "End", bubbles: true }),
      );
    });
    // Last supported locale is Portuguese (Portugal).
    expect(
      container
        .querySelector('[data-language-option="pt-PT"]')
        ?.getAttribute("data-active"),
    ).toBe("true");

    await act(async () => {
      list.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Home", bubbles: true }),
      );
    });
    expect(
      container
        .querySelector('[data-language-option="zh-CN"]')
        ?.getAttribute("data-active"),
    ).toBe("true");

    await act(async () => {
      list.dispatchEvent(
        new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }),
      );
    });
    await act(async () => {
      list.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
      );
    });
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("zh-TW");
    expect(document.documentElement.lang).toBe("zh-TW");
    expect(document.documentElement.dir).toBe("ltr");
    expect(menu()).toBeNull();
    expect(button.getAttribute("aria-expanded")).toBe("false");
    expect(button.getAttribute("title")).toContain(LOCALE_LABELS["zh-TW"]);

    await act(async () => {
      button.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
      );
    });
    expect(menu()).not.toBeNull();
    await act(async () => {
      menu().dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(menu()).toBeNull();
    expect(button.getAttribute("aria-expanded")).toBe("false");
  });

  it("closes on outside pointerdown without changing locale", async () => {
    await renderSwitcher("fr");
    await act(async () => trigger().click());
    expect(menu()).not.toBeNull();
    await act(async () => {
      document.body.dispatchEvent(
        new PointerEvent("pointerdown", { bubbles: true }),
      );
    });
    expect(menu()).toBeNull();
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBeNull();
    expect(document.documentElement.lang).toBe("fr");
  });

  it("selects a locale by pointer and persists it", async () => {
    await renderSwitcher("en");
    await act(async () => trigger().click());
    const ja = container.querySelector('[data-language-option="ja"]');
    await act(async () => ja.click());
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("ja");
    expect(document.documentElement.lang).toBe("ja");
    expect(document.documentElement.dir).toBe("ltr");
    expect(trigger().getAttribute("title")).toContain(LOCALE_LABELS.ja);
    expect(menu()).toBeNull();
  });

  it("shows native labels for new locales and applies Arabic rtl then restores ltr", async () => {
    await renderSwitcher("en");
    await act(async () => trigger().click());
    expect(menu()?.classList.contains("lang-menu")).toBe(true);
    expect(indexCss).toMatch(/inset-inline-end\s*:\s*0/);
    expect(indexCss).toMatch(/\.terminal-xterm[\s\S]*direction:\s*ltr/);

    for (const code of ["es", "id", "th", "vi", "ar"]) {
      const option = container.querySelector(`[data-language-option="${code}"]`);
      expect(option).not.toBeNull();
      expect(option.textContent).toContain(LOCALE_LABELS[code]);
    }

    const ar = container.querySelector('[data-language-option="ar"]');
    await act(async () => ar.click());
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("ar");
    expect(document.documentElement.lang).toBe("ar");
    expect(document.documentElement.dir).toBe("rtl");
    expect(trigger().getAttribute("title")).toContain(LOCALE_LABELS.ar);
    expect(menu()).toBeNull();

    await act(async () => trigger().click());
    const en = container.querySelector('[data-language-option="en"]');
    await act(async () => en.click());
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("en");
    expect(document.documentElement.lang).toBe("en");
    expect(document.documentElement.dir).toBe("ltr");
    expect(trigger().getAttribute("title")).toContain(LOCALE_LABELS.en);
  });
});
