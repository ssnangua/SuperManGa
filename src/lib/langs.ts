/** UI / OCR / translation language catalog. App chrome is only zh/en/ja/ko; these codes are for content. */
export type AppLangOption = {
  code: string;
  labelKey: string;
};

export const APP_LANGS: readonly AppLangOption[] = [
  { code: "zh-CN", labelKey: "langZhCn" },
  { code: "zh-TW", labelKey: "langZhTw" },
  { code: "en", labelKey: "langEn" },
  { code: "ja", labelKey: "langJa" },
  { code: "ko", labelKey: "langKo" },
  { code: "fr", labelKey: "langFr" },
  { code: "es", labelKey: "langEs" },
  { code: "pt", labelKey: "langPt" },
  { code: "pt-BR", labelKey: "langPtBr" },
  { code: "de", labelKey: "langDe" },
  { code: "it", labelKey: "langIt" },
  { code: "ru", labelKey: "langRu" },
  { code: "uk", labelKey: "langUk" },
  { code: "nl", labelKey: "langNl" },
  { code: "pl", labelKey: "langPl" },
  { code: "cs", labelKey: "langCs" },
  { code: "sk", labelKey: "langSk" },
  { code: "hu", labelKey: "langHu" },
  { code: "ro", labelKey: "langRo" },
  { code: "bg", labelKey: "langBg" },
  { code: "hr", labelKey: "langHr" },
  { code: "sr", labelKey: "langSr" },
  { code: "sl", labelKey: "langSl" },
  { code: "el", labelKey: "langEl" },
  { code: "tr", labelKey: "langTr" },
  { code: "sv", labelKey: "langSv" },
  { code: "da", labelKey: "langDa" },
  { code: "fi", labelKey: "langFi" },
  { code: "no", labelKey: "langNo" },
  { code: "et", labelKey: "langEt" },
  { code: "lv", labelKey: "langLv" },
  { code: "lt", labelKey: "langLt" },
  { code: "ar", labelKey: "langAr" },
  { code: "he", labelKey: "langHe" },
  { code: "hi", labelKey: "langHi" },
  { code: "th", labelKey: "langTh" },
  { code: "vi", labelKey: "langVi" },
  { code: "id", labelKey: "langId" },
  { code: "ms", labelKey: "langMs" },
  { code: "fil", labelKey: "langFil" },
] as const;

export function langBase(code: string): string {
  const trimmed = code.trim();
  if (trimmed === "zh-CN" || trimmed === "zh-TW") return "zh";
  return trimmed.split("-")[0]?.toLowerCase() ?? trimmed.toLowerCase();
}

const APP_LANG_CODES = new Set(APP_LANGS.map((lang) => lang.code));

function mapSystemTagToTargetLang(tag: string): string | null {
  const normalized = tag.trim().replace(/_/g, "-");
  if (!normalized) return null;
  if (APP_LANG_CODES.has(normalized)) return normalized;

  const parts = normalized.split("-");
  const lang = (parts[0] ?? "").toLowerCase();
  const rest = parts.slice(1).map((p) => p.toLowerCase());

  if (lang === "zh") {
    if (rest.some((p) => p === "hant" || p === "tw" || p === "hk" || p === "mo")) return "zh-TW";
    return "zh-CN";
  }
  if (lang === "pt" && rest.some((p) => p === "br")) return "pt-BR";
  if (lang === "nb" || lang === "nn") return "no";
  if (lang === "fil" || lang === "tl") return "fil";

  const exactBase = APP_LANGS.find((opt) => opt.code.toLowerCase() === lang);
  if (exactBase) return exactBase.code;

  const byBase = APP_LANGS.find((opt) => langBase(opt.code) === lang);
  return byBase?.code ?? null;
}

/** Map OS / browser languages onto a translation target in APP_LANGS. Fallback: en. */
export function detectSystemTargetLang(): string {
  const candidates: string[] = [];
  if (typeof navigator !== "undefined") {
    if (Array.isArray(navigator.languages)) candidates.push(...navigator.languages);
    if (navigator.language) candidates.push(navigator.language);
  }
  for (const tag of candidates) {
    const mapped = mapSystemTagToTargetLang(tag);
    if (mapped) return mapped;
  }
  return "en";
}
