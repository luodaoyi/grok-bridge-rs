import ar from "./ar.js";
import de from "./de.js";
import en from "./en.js";
import es from "./es.js";
import fr from "./fr.js";
import id from "./id.js";
import ja from "./ja.js";
import ko from "./ko.js";
import ptBR from "./pt-BR.js";
import ptPT from "./pt-PT.js";
import ru from "./ru.js";
import th from "./th.js";
import vi from "./vi.js";
import zhCN from "./zh-CN.js";
import zhTW from "./zh-TW.js";

/** @type {Record<string, Record<string, string>>} */
export const catalogs = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  en,
  fr,
  de,
  ru,
  ja,
  ko,
  es,
  "pt-BR": ptBR,
  "pt-PT": ptPT,
  id,
  th,
  vi,
  ar,
};

/** Canonical key set from English catalog. */
export const MESSAGE_KEYS = Object.freeze(Object.keys(en).sort());
