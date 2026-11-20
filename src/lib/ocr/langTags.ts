import { APP_LANGS, langBase } from "../langs";
import type { MessageKey } from "../../i18n";

/** Unique language-tag ids for multilingual OCR chips (zh once, regional variants collapsed). */
export function allOcrLangTagIds(): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const lang of APP_LANGS) {
    const id = langBase(lang.code);
    if (!id || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

/** i18n key for an OCR language chip id (`zh` → 中文, else APP_LANGS label). */
export function ocrLangTagLabelKey(code: string): MessageKey {
  const id = code.trim();
  if (id === "zh") return "langZh";
  const exact = APP_LANGS.find((lang) => lang.code === id);
  if (exact) return exact.labelKey as MessageKey;
  const byBase = APP_LANGS.find((lang) => langBase(lang.code) === id);
  if (byBase) return byBase.labelKey as MessageKey;
  return "langEn";
}
