import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  MESSAGE_KEYS,
  SUPPORTED_LOCALES,
  LOCALE_LABELS,
  catalogs,
  createTranslator,
  detectLocale,
  formatAge,
  formatClock,
  formatNumber,
  formatRemaining,
  interpolate,
  normalizeLocale,
  readStoredLocale,
  resolveLocale,
  writeStoredLocale,
  LOCALE_STORAGE_KEY,
  applyDocumentLocale,
  I18nProvider,
  useI18n,
} from "./index.js";
import { localeDirection } from "./translate.js";
import { RTL_LOCALES } from "./constants.js";

/** Keys intentionally identical to English (brands, protocol, punctuation, abbreviations). */
const INTENTIONAL_ENGLISH_IDENTICAL = new Set([
  "app.brand",
  "app.github",
  "app.runtimeVersion",
  "error.brand",
  "group.idPrefix",
  "group.summary.sep",
  "session.meta.pidValue",
  "action.failureJoin",
  "stream.pushMode",
  "theme.auto",
  "terminal.resizeValue",
  "group.supervisor",
  "session.subagent",
  "session.meta.hook",
  "badge.phase",
  "update.openRelease",
  // Indonesian keeps the English loanword for this short control label.
  "interactive.label",
]);

const REPRESENTATIVE_KEYS = [
  "doc.title",
  "app.title",
  "connection.connected",
  "session.close",
  "interactive.warning",
  "action.confirmCloseSession",
  "terminal.resizeAria",
  "error.renderTitle",
];

const EXPECTED_LOCALES = [
  "zh-CN",
  "zh-TW",
  "en",
  "ru",
  "es",
  "fr",
  "de",
  "ja",
  "ko",
  "id",
  "vi",
  "th",
  "ar",
];

describe("catalog completeness", () => {
  it("keeps the same keys for all thirteen locales", () => {
    expect(SUPPORTED_LOCALES).toEqual(EXPECTED_LOCALES);
    expect(SUPPORTED_LOCALES).toHaveLength(13);
    expect(MESSAGE_KEYS.length).toBe(135);
    for (const locale of SUPPORTED_LOCALES) {
      const keys = Object.keys(catalogs[locale]).sort();
      expect(keys).toEqual(MESSAGE_KEYS);
      for (const key of MESSAGE_KEYS) {
        expect(String(catalogs[locale][key] ?? "").length).toBeGreaterThan(0);
      }
    }
  });

  it("keeps {placeholder} names aligned with English across all locales", () => {
    const paramNames = (text) =>
      [...String(text).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const key of MESSAGE_KEYS) {
      const expected = paramNames(catalogs.en[key]);
      for (const locale of SUPPORTED_LOCALES) {
        expect(paramNames(catalogs[locale][key])).toEqual(expected);
      }
    }
  });

  it("uses natural non-English strings for representative chrome keys", () => {
    for (const locale of SUPPORTED_LOCALES) {
      if (locale === "en") continue;
      for (const key of REPRESENTATIVE_KEYS) {
        expect(catalogs[locale][key]).not.toBe(catalogs.en[key]);
        expect(String(catalogs[locale][key]).trim().length).toBeGreaterThan(0);
      }
    }
    // Spot-check a few high-visibility translations.
    expect(catalogs["zh-CN"]["session.close"]).toBe("关闭 Grok");
    expect(catalogs["zh-TW"]["session.close"]).toBe("關閉 Grok");
    expect(catalogs.fr["connection.connected"]).toMatch(/Canal|direct/i);
    expect(catalogs.de["connection.connected"]).toMatch(/Live-Kanal|verbunden/i);
    expect(catalogs.ru["app.title"]).toMatch(/Консоль|субагент/i);
    expect(catalogs.ja["empty.title"]).toMatch(/セッション/);
    expect(catalogs.ko["header.expandAll"]).toMatch(/펼치/);
    expect(catalogs.es["session.close"]).toMatch(/Cerrar|Grok/i);
    expect(catalogs.id["connection.connected"]).toMatch(/langsung|terhubung/i);
    expect(catalogs.th["app.title"]).toMatch(/คอนโซล|ตัวแทน/i);
    expect(catalogs.vi["session.close"]).toMatch(/Đóng|Grok/i);
    expect(catalogs.ar["app.title"]).toMatch(/وحدة|وكيل/i);
  });

  it("only keeps intentional English-identical catalog values", () => {
    const brandOrProtocol = (value) =>
      /Grok|Codex|GitHub|WebSocket|Runtime|Release|PTY|Hook|PID|\/api\/|Supervisor|Subagent|Auto|pixels|id \{| · |,\s*$/i.test(
        value,
      ) || /^[ ·,./\\-]+$/.test(value);

    for (const locale of SUPPORTED_LOCALES) {
      if (locale === "en") continue;
      const same = MESSAGE_KEYS.filter(
        (key) => catalogs[locale][key] === catalogs.en[key],
      );
      for (const key of same) {
        const value = catalogs.en[key];
        const allowed =
          INTENTIONAL_ENGLISH_IDENTICAL.has(key) || brandOrProtocol(value);
        expect(
          allowed,
          `${locale} key ${key} is English-identical without allowlist: ${value}`,
        ).toBe(true);
      }
    }
  });

  it("exposes native labels for every supported locale", () => {
    expect(LOCALE_LABELS.es).toBe("Español");
    expect(LOCALE_LABELS.id).toBe("Bahasa Indonesia");
    expect(LOCALE_LABELS.th).toBe("ไทย");
    expect(LOCALE_LABELS.vi).toBe("Tiếng Việt");
    expect(LOCALE_LABELS.ar).toBe("العربية");
    for (const locale of SUPPORTED_LOCALES) {
      expect(String(LOCALE_LABELS[locale] ?? "").trim().length).toBeGreaterThan(
        0,
      );
    }
  });
});

describe("locale detection and persistence", () => {
  it("maps zh script/region tags", () => {
    expect(normalizeLocale("zh-Hant")).toBe("zh-TW");
    expect(normalizeLocale("zh-Hant-TW")).toBe("zh-TW");
    expect(normalizeLocale("zh-TW")).toBe("zh-TW");
    expect(normalizeLocale("zh-HK")).toBe("zh-TW");
    expect(normalizeLocale("zh-Hans")).toBe("zh-CN");
    expect(normalizeLocale("zh-CN")).toBe("zh-CN");
    expect(normalizeLocale("zh")).toBe("zh-CN");
    expect(normalizeLocale("zh-SG")).toBe("zh-CN");
  });

  it("maps primary languages and falls back to en", () => {
    expect(normalizeLocale("en-US")).toBe("en");
    expect(normalizeLocale("fr-FR")).toBe("fr");
    expect(normalizeLocale("de-DE")).toBe("de");
    expect(normalizeLocale("ru-RU")).toBe("ru");
    expect(normalizeLocale("ja-JP")).toBe("ja");
    expect(normalizeLocale("ko-KR")).toBe("ko");
    expect(normalizeLocale("es-MX")).toBe("es");
    expect(normalizeLocale("es-ES")).toBe("es");
    expect(normalizeLocale("id-ID")).toBe("id");
    expect(normalizeLocale("th-TH")).toBe("th");
    expect(normalizeLocale("vi-VN")).toBe("vi");
    expect(normalizeLocale("ar-EG")).toBe("ar");
    expect(normalizeLocale("ar-SA")).toBe("ar");
    expect(normalizeLocale("pt-BR")).toBe(null);
    expect(detectLocale(["pt-BR", "it-IT"])).toBe("en");
    expect(detectLocale(["es-MX", "en"])).toBe("es");
    expect(detectLocale(["id-ID", "en"])).toBe("id");
    expect(detectLocale(["th-TH", "en"])).toBe("th");
    expect(detectLocale(["vi-VN", "en"])).toBe("vi");
    expect(detectLocale(["ar-EG", "en"])).toBe("ar");
    expect(detectLocale(["zh-Hant-TW", "en"])).toBe("zh-TW");
    expect(detectLocale(["fr-CA", "en-US"])).toBe("fr");
  });

  it("persists user choice and prefers storage over navigator", () => {
    const storage = {
      data: {},
      getItem(key) {
        return this.data[key] ?? null;
      },
      setItem(key, value) {
        this.data[key] = String(value);
      },
    };
    writeStoredLocale("de", storage);
    expect(readStoredLocale(storage)).toBe("de");
    expect(storage.data[LOCALE_STORAGE_KEY]).toBe("de");
    expect(
      resolveLocale({ storage, languages: ["zh-CN", "en"] }),
    ).toBe("de");
    expect(resolveLocale({ languages: ["ja-JP"] })).toBe("ja");
    writeStoredLocale("ar", storage);
    expect(readStoredLocale(storage)).toBe("ar");
    expect(
      resolveLocale({ storage, languages: ["en-US"] }),
    ).toBe("ar");
  });

  it("falls back missing keys to English then raw key", () => {
    const t = createTranslator("en");
    expect(t("app.github")).toBe("GitHub");
    expect(t("missing.key")).toBe("missing.key");
    expect(interpolate("Hi {name}", { name: "Ada" })).toBe("Hi Ada");
    expect(interpolate("Hi {name}", {})).toBe("Hi {name}");
  });
});

describe("document direction", () => {
  it("marks only Arabic as rtl and every other locale as ltr", () => {
    expect(RTL_LOCALES).toEqual(["ar"]);
    for (const locale of SUPPORTED_LOCALES) {
      expect(localeDirection(locale)).toBe(locale === "ar" ? "rtl" : "ltr");
    }
    expect(localeDirection("unknown")).toBe("ltr");
  });

  it("applyDocumentLocale sets html lang and dir including rtl/ltr switch", () => {
    document.documentElement.lang = "en";
    document.documentElement.dir = "ltr";
    applyDocumentLocale("ar");
    expect(document.documentElement.lang).toBe("ar");
    expect(document.documentElement.dir).toBe("rtl");
    expect(document.title).toBe(catalogs.ar["doc.title"]);

    applyDocumentLocale("es");
    expect(document.documentElement.lang).toBe("es");
    expect(document.documentElement.dir).toBe("ltr");
    expect(document.title).toBe(catalogs.es["doc.title"]);

    applyDocumentLocale("en");
    expect(document.documentElement.dir).toBe("ltr");
  });
});

describe("Intl helpers", () => {
  it("formats numbers, clocks, relative ages, and remaining durations", () => {
    expect(formatNumber(1234, "en")).toMatch(/1[,.]?234|1 234|1234/);
    const clock = formatClock(new Date("2026-07-19T12:34:56Z"), "en");
    expect(clock.length).toBeGreaterThan(0);

    const now = Date.UTC(2026, 6, 19, 12, 0, 0);
    const age = formatAge(now - 90_000, now, "en");
    expect(age).toMatch(/minute|min|分钟|分鐘|分钟|分/i);

    const remaining = formatRemaining(now + 45_000, now, "en");
    expect(remaining.toLowerCase()).toMatch(/second|sec|秒/);
  });
});

function Probe({ onState }) {
  const i18n = useI18n();
  onState(i18n);
  return (
    <div>
      <span data-title>{i18n.t("app.title")}</span>
      <button type="button" onClick={() => i18n.setLocale("fr")}>
        fr
      </button>
      <button type="button" onClick={() => i18n.setLocale("de")}>
        de
      </button>
      <button type="button" onClick={() => i18n.setLocale("ar")}>
        ar
      </button>
      <button type="button" onClick={() => i18n.setLocale("en")}>
        en
      </button>
    </div>
  );
}

describe("I18nProvider document effects and switching", () => {
  let container;
  let root;
  let latest;

  beforeEach(() => {
    localStorage.clear();
    document.documentElement.lang = "en";
    document.documentElement.dir = "ltr";
    document.title = "old";
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    latest = null;
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
  });

  it("applies html lang, document title, and switches locale", async () => {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale="zh-CN">
          <Probe
            onState={(state) => {
              latest = state;
            }}
          />
        </I18nProvider>,
      );
    });

    expect(document.documentElement.lang).toBe("zh-CN");
    expect(document.documentElement.dir).toBe("ltr");
    expect(document.title).toBe(catalogs["zh-CN"]["doc.title"]);
    expect(container.textContent).toContain(catalogs["zh-CN"]["app.title"]);

    await act(async () => {
      container.querySelector("button")?.click();
    });
    expect(latest.locale).toBe("fr");
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("fr");
    expect(document.documentElement.lang).toBe("fr");
    expect(document.documentElement.dir).toBe("ltr");
    expect(document.title).toBe(catalogs.fr["doc.title"]);
    expect(container.textContent).toContain(catalogs.fr["app.title"]);

    await act(async () => {
      [...container.querySelectorAll("button")]
        .find((button) => button.textContent === "de")
        ?.click();
    });
    expect(document.documentElement.lang).toBe("de");
    expect(document.title).toBe(catalogs.de["doc.title"]);
  });

  it("switches document dir to rtl for Arabic and back to ltr", async () => {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale="en">
          <Probe
            onState={(state) => {
              latest = state;
            }}
          />
        </I18nProvider>,
      );
    });
    expect(document.documentElement.dir).toBe("ltr");

    await act(async () => {
      [...container.querySelectorAll("button")]
        .find((button) => button.textContent === "ar")
        ?.click();
    });
    expect(latest.locale).toBe("ar");
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("ar");
    expect(document.documentElement.lang).toBe("ar");
    expect(document.documentElement.dir).toBe("rtl");
    expect(document.title).toBe(catalogs.ar["doc.title"]);
    expect(container.textContent).toContain(catalogs.ar["app.title"]);

    await act(async () => {
      [...container.querySelectorAll("button")]
        .find((button) => button.textContent === "en")
        ?.click();
    });
    expect(latest.locale).toBe("en");
    expect(document.documentElement.lang).toBe("en");
    expect(document.documentElement.dir).toBe("ltr");
  });

  it("applyDocumentLocale updates description meta", () => {
    let meta = document.querySelector('meta[name="description"]');
    if (!meta) {
      meta = document.createElement("meta");
      meta.setAttribute("name", "description");
      document.head.append(meta);
    }
    applyDocumentLocale("ja");
    expect(document.documentElement.lang).toBe("ja");
    expect(document.documentElement.dir).toBe("ltr");
    expect(meta.getAttribute("content")).toBe(catalogs.ja["doc.description"]);
  });

  it("interpolates action and interactive messages without mutating runtime params", () => {
    const t = createTranslator("fr");
    const owner = "Codex A/中文";
    const reason = "waiting_for_user_input_v2";
    expect(t("action.confirmCloseGroup", { owner, count: "3" })).toContain(
      owner,
    );
    expect(t("session.waitingNote", { reason })).toContain(reason);
    expect(t("session.waitingNote", { reason })).not.toContain("user input");
    expect(t("interactive.error", { detail: "session not found" })).toContain(
      "session not found",
    );

    const tAr = createTranslator("ar");
    expect(tAr("action.confirmCloseGroup", { owner, count: "3" })).toContain(
      owner,
    );
    expect(tAr("session.waitingNote", { reason })).toContain(reason);
  });
});
