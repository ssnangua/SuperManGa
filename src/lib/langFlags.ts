/** ISO 3166-1 alpha-2 used on the vendored flag sprite (representative country). */
const LANG_TO_FLAG: Record<string, string> = {
  "zh-CN": "CN",
  "zh-TW": "TW",
  en: "US",
  ja: "JP",
  ko: "KR",
  fr: "FR",
  es: "ES",
  pt: "PT",
  "pt-BR": "BR",
  de: "DE",
  it: "IT",
  ru: "RU",
  uk: "UA",
  nl: "NL",
  pl: "PL",
  cs: "CZ",
  sk: "SK",
  hu: "HU",
  ro: "RO",
  bg: "BG",
  hr: "HR",
  sr: "RS",
  sl: "SI",
  el: "GR",
  tr: "TR",
  sv: "SE",
  da: "DK",
  fi: "FI",
  no: "NO",
  et: "EE",
  lv: "LV",
  lt: "LT",
  ar: "SA",
  he: "IL",
  hi: "IN",
  th: "TH",
  vi: "VN",
  id: "ID",
  ms: "MY",
  fil: "PH",
};

export function langFlagCode(langId: string): string | undefined {
  const id = langId.trim();
  if (!id || id === "auto") return undefined;
  return LANG_TO_FLAG[id];
}
