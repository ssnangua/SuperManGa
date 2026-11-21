import { langBase } from "../langs";
import { isWindowsDesktop } from "../platform";
import { allOcrLangTagIds } from "./langTags";

/** User-selectable OCR backends. */
export type OcrEngineId =
  | "windows"
  | "manga-ocr"
  | "paddle-ocr"
  | "mit-48px"
  | "easyocr";

export type OcrEngineMeta = {
  id: OcrEngineId;
  /** i18n label key */
  nameKey: string;
  /** i18n hint key */
  hintKey: string;
  /** Asset pack id required before use; null = always available. */
  assetId: string | null;
  /**
   * Source langs this engine is meant for. `"*"` = not limited to UI locales.
   * `tagLangs` are display chips (full list for multilingual engines).
   */
  langs: "*" | readonly string[];
  tagLangs: readonly string[];
};

/** Full content-language chip set used by multilingual engines / model packs. */
const MULTILINGUAL_TAGS = allOcrLangTagIds();

/**
 * Latin-script chips shared by PP-OCRv6 / EasyOCR latin_g2
 * (EN/FR/PT/DE/…; not Cyrillic / Arabic / Indic / Thai / Greek).
 */
const LATIN_SCRIPT_TAGS: readonly string[] = [
  "en",
  "fr",
  "es",
  "pt",
  "de",
  "it",
  "nl",
  "pl",
  "cs",
  "sk",
  "hu",
  "ro",
  "hr",
  "sl",
  "tr",
  "sv",
  "da",
  "fi",
  "no",
  "et",
  "lv",
  "lt",
  "vi",
  "id",
  "ms",
  "fil",
];

/** PP-OCRv6 medium unified model: zh + ja + Latin (no Korean / Cyrillic / Arabic…). */
const PADDLE_OCR_TAGS: readonly string[] = ["zh", "ja", ...LATIN_SCRIPT_TAGS];

/** EasyOCR g2 pack: zh_sim + japanese + korean + latin_g2. */
const EASYOCR_TAGS: readonly string[] = ["zh", "ja", "ko", ...LATIN_SCRIPT_TAGS];

export const OCR_ENGINES: readonly OcrEngineMeta[] = [
  {
    id: "windows",
    nameKey: "ocrEngineWindows",
    hintKey: "ocrEngineWindowsHint",
    assetId: null,
    langs: "*",
    tagLangs: MULTILINGUAL_TAGS,
  },
  {
    id: "manga-ocr",
    nameKey: "ocrEngineMangaOcr",
    hintKey: "ocrEngineMangaOcrHint",
    assetId: "ocr-manga-ocr",
    langs: "*",
    tagLangs: ["ja"],
  },
  {
    id: "paddle-ocr",
    nameKey: "ocrEnginePaddle",
    hintKey: "ocrEnginePaddleHint",
    assetId: "ocr-paddle",
    langs: "*",
    tagLangs: PADDLE_OCR_TAGS,
  },
  {
    id: "mit-48px",
    nameKey: "ocrEngineMit48",
    hintKey: "ocrEngineMit48Hint",
    assetId: "ocr-mit48px",
    langs: "*",
    tagLangs: ["ja", "ko"],
  },
  {
    id: "easyocr",
    nameKey: "ocrEngineEasyOcr",
    hintKey: "ocrEngineEasyOcrHint",
    assetId: "ocr-easyocr",
    langs: "*",
    tagLangs: EASYOCR_TAGS,
  },
] as const;

export const OCR_ENGINE_IDS: readonly OcrEngineId[] = OCR_ENGINES.map((e) => e.id);

/** Default OCR engine (needs pack + ORT; Windows OCR remains available as a manual choice). */
export function defaultOcrEngineId(): OcrEngineId {
  return "manga-ocr";
}

export function normalizeOcrEngineId(value: unknown): OcrEngineId {
  if (typeof value === "string" && (OCR_ENGINE_IDS as readonly string[]).includes(value)) {
    const id = value as OcrEngineId;
    if (id === "windows" && !isWindowsDesktop()) return defaultOcrEngineId();
    return id;
  }
  return defaultOcrEngineId();
}

export function ocrEngineMeta(id: OcrEngineId): OcrEngineMeta {
  return (
    OCR_ENGINES.find((e) => e.id === id) ??
    OCR_ENGINES.find((e) => e.id === defaultOcrEngineId()) ??
    OCR_ENGINES[0]!
  );
}

export function ocrEngineSupportsSourceLang(engine: OcrEngineId, sourceLang: string): boolean {
  const langs = ocrEngineMeta(engine).langs;
  const code = sourceLang.trim();
  if (code === "auto" || code === "" || langs === "*") return true;
  return langs.includes(code) || langs.includes(langBase(code));
}

/** Do not coerce content languages down to UI locales. */
export function clampSourceLangForOcrEngine(_engine: OcrEngineId, sourceLang: string): string {
  return sourceLang.trim() || "auto";
}

/** Assets that must be installed for the selected engine. */
export function requiredOcrAssetIdsForEngine(engine: OcrEngineId): string[] {
  const assetId = ocrEngineMeta(engine).assetId;
  return assetId ? [assetId] : [];
}
