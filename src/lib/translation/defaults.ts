import { detectSystemTargetLang } from "../langs";
import {
  clampSourceLangForOcrEngine,
  defaultOcrEngineId,
  normalizeOcrEngineId,
} from "../ocr/engines";
import { normalizeTextDetectorId } from "../ocr/detectors";
import type {
  TranslationAiProfile,
  TranslationProviderId,
  TranslationSettings,
  TranslationTextAlign,
  TranslationTextDirection,
} from "./types";
import { TRANSLATION_PROVIDER_IDS } from "./types";

export const DEFAULT_TRANSLATION_AI_MAX_TOKENS = 2048;
export const DEFAULT_TEXT_FONT_SIZE = 26;
export const DEFAULT_TEXT_LINE_SPACING = 1;
export const DEFAULT_TEXT_STROKE_WIDTH = 3;
export const DEFAULT_TEXT_COLOR = "#000000";

export function createTranslationAiProfile(
  partial?: Partial<TranslationAiProfile> & { id?: string },
): TranslationAiProfile {
  const id = partial?.id || `profile-${Date.now()}`;
  return {
    id,
    name: partial?.name ?? "",
    endpoint: {
      baseUrl: partial?.endpoint?.baseUrl ?? "",
      apiKey: partial?.endpoint?.apiKey ?? "",
      model: partial?.endpoint?.model ?? "",
      maxTokens: partial?.endpoint?.maxTokens ?? DEFAULT_TRANSLATION_AI_MAX_TOKENS,
    },
    updatedAt: partial?.updatedAt ?? Date.now(),
  };
}

export function defaultTranslationSettings(): TranslationSettings {
  const profile = createTranslationAiProfile({ id: "profile-default", name: "" });
  return {
    provider: "microsoft",
    targetLang: detectSystemTargetLang(),
    sourceLang: "auto",
    ocrEngine: defaultOcrEngineId(),
    textDetector: "ctd",
    enableAuxYolo: false,
    enableSaberYoloRefine: true,
    bookPreTranslate: true,
    deeplApiKey: "",
    aiProfiles: [profile],
    activeAiProfileId: profile.id,
    aiBaseUrl: "",
    aiApiKey: "",
    aiModel: "",
    aiMaxTokens: DEFAULT_TRANSLATION_AI_MAX_TOKENS,
    aiVendorKeys: {},
    aiVendorModels: {},
    baiduAppId: "",
    baiduSecret: "",
    youdaoAppKey: "",
    youdaoAppSecret: "",
    tencentSecretId: "",
    tencentSecretKey: "",
    tencentRegion: "ap-guangzhou",
    volcAccessKeyId: "",
    volcSecretKey: "",
    volcRegion: "cn-north-1",
    aliyunAccessKeyId: "",
    aliyunAccessKeySecret: "",
    caiyunToken: "",
    textFontSize: DEFAULT_TEXT_FONT_SIZE,
    textAutoFontSize: true,
    textFontFamily: "",
    textDirection: "auto",
    textLineSpacing: DEFAULT_TEXT_LINE_SPACING,
    textAlign: "start",
    textColor: DEFAULT_TEXT_COLOR,
    // Auto uses 48px line colors (Saber-style); works well when pack installed.
    textAutoColor: true,
    textStrokeEnabled: true,
    textStrokeWidth: DEFAULT_TEXT_STROKE_WIDTH,
    textStrokeAutoScale: true,
  };
}

function isProviderId(v: unknown): v is TranslationProviderId {
  return typeof v === "string" && (TRANSLATION_PROVIDER_IDS as readonly string[]).includes(v);
}

function str(v: unknown, fallback = ""): string {
  return typeof v === "string" ? v : fallback;
}

function bool(v: unknown, fallback: boolean): boolean {
  return typeof v === "boolean" ? v : fallback;
}

function clampNum(v: unknown, fallback: number, min: number, max: number, step?: number): number {
  if (typeof v !== "number" || !Number.isFinite(v)) return fallback;
  let n = Math.min(max, Math.max(min, v));
  if (step && step > 0) {
    n = Math.round(n / step) * step;
    // Avoid 1.0000001 style floats for 0.1 steps.
    const decimals = String(step).includes(".") ? String(step).split(".")[1]!.length : 0;
    if (decimals > 0) n = Number(n.toFixed(decimals));
  }
  return n;
}

function textDirection(v: unknown, fallback: TranslationTextDirection): TranslationTextDirection {
  return v === "auto" || v === "horizontal" || v === "vertical" ? v : fallback;
}

function textAlign(v: unknown, fallback: TranslationTextAlign): TranslationTextAlign {
  return v === "start" || v === "center" || v === "end" ? v : fallback;
}

function normalizeVendorKeys(raw: unknown): Record<string, string> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, string> = {};
  for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
    const key = id.trim();
    if (!key || typeof value !== "string" || !value.trim()) continue;
    out[key] = value;
  }
  return out;
}

export function mergeTranslationSettings(
  partial: Partial<TranslationSettings> | null | undefined,
): TranslationSettings {
  const base = defaultTranslationSettings();
  if (!partial) return base;
  const profile = createTranslationAiProfile(partial.aiProfiles?.[0] ?? base.aiProfiles[0]);
  const profiles =
    Array.isArray(partial.aiProfiles) && partial.aiProfiles.length > 0
      ? partial.aiProfiles.map((p, i) => createTranslationAiProfile({ ...p, id: p.id || `profile-${i}` }))
      : [profile];
  const merged: TranslationSettings = {
    ...base,
    ...partial,
    provider: isProviderId(partial.provider) ? partial.provider : base.provider,
    targetLang: str(partial.targetLang, base.targetLang) || base.targetLang,
    ocrEngine: normalizeOcrEngineId(partial.ocrEngine ?? base.ocrEngine),
    textDetector: normalizeTextDetectorId(partial.textDetector ?? base.textDetector),
    enableAuxYolo:
      typeof (partial as { enableAuxYolo?: unknown }).enableAuxYolo === "boolean"
        ? (partial as { enableAuxYolo: boolean }).enableAuxYolo
        : base.enableAuxYolo,
    enableSaberYoloRefine:
      typeof (partial as { enableSaberYoloRefine?: unknown }).enableSaberYoloRefine === "boolean"
        ? (partial as { enableSaberYoloRefine: boolean }).enableSaberYoloRefine
        : base.enableSaberYoloRefine,
    bookPreTranslate: bool(
      (partial as { bookPreTranslate?: unknown }).bookPreTranslate,
      base.bookPreTranslate,
    ),
    sourceLang: clampSourceLangForOcrEngine(
      normalizeOcrEngineId(partial.ocrEngine ?? base.ocrEngine),
      str(partial.sourceLang, base.sourceLang) || base.sourceLang,
    ),
    deeplApiKey: str(partial.deeplApiKey),
    aiProfiles: profiles,
    activeAiProfileId: str(partial.activeAiProfileId, profiles[0]!.id) || profiles[0]!.id,
    aiBaseUrl: str(partial.aiBaseUrl),
    aiApiKey: str(partial.aiApiKey),
    aiModel: str(partial.aiModel),
    aiMaxTokens:
      typeof partial.aiMaxTokens === "number" && Number.isFinite(partial.aiMaxTokens)
        ? Math.max(256, Math.min(8192, Math.round(partial.aiMaxTokens)))
        : base.aiMaxTokens,
    aiVendorKeys: normalizeVendorKeys(partial.aiVendorKeys),
    aiVendorModels: normalizeVendorKeys(partial.aiVendorModels),
    baiduAppId: str(partial.baiduAppId),
    baiduSecret: str(partial.baiduSecret),
    youdaoAppKey: str(partial.youdaoAppKey),
    youdaoAppSecret: str(partial.youdaoAppSecret),
    tencentSecretId: str(partial.tencentSecretId),
    tencentSecretKey: str(partial.tencentSecretKey),
    tencentRegion: str(partial.tencentRegion, base.tencentRegion) || base.tencentRegion,
    volcAccessKeyId: str(partial.volcAccessKeyId),
    volcSecretKey: str(partial.volcSecretKey),
    volcRegion: str(partial.volcRegion, base.volcRegion) || base.volcRegion,
    aliyunAccessKeyId: str(partial.aliyunAccessKeyId),
    aliyunAccessKeySecret: str(partial.aliyunAccessKeySecret),
    caiyunToken: str(partial.caiyunToken),
    textFontSize: clampNum(partial.textFontSize, base.textFontSize, 1, 200, 1),
    textAutoFontSize: bool(partial.textAutoFontSize, base.textAutoFontSize),
    textFontFamily: str(partial.textFontFamily),
    textDirection: textDirection(partial.textDirection, base.textDirection),
    textLineSpacing: clampNum(partial.textLineSpacing, base.textLineSpacing, 0.1, 5, 0.1),
    textAlign: textAlign(partial.textAlign, base.textAlign),
    textColor: str(partial.textColor, base.textColor) || base.textColor,
    textAutoColor: bool(partial.textAutoColor, base.textAutoColor),
    textStrokeEnabled: bool(partial.textStrokeEnabled, base.textStrokeEnabled),
    textStrokeWidth: clampNum(partial.textStrokeWidth, base.textStrokeWidth, 1, 32, 1),
    textStrokeAutoScale: bool(partial.textStrokeAutoScale, base.textStrokeAutoScale),
  };
  const active = merged.aiProfiles.find((p) => p.id === merged.activeAiProfileId) ?? merged.aiProfiles[0];
  if (active) {
    if (merged.aiBaseUrl) active.endpoint.baseUrl = merged.aiBaseUrl;
    if (merged.aiApiKey) active.endpoint.apiKey = merged.aiApiKey;
    if (merged.aiModel) active.endpoint.model = merged.aiModel;
    active.endpoint.maxTokens = merged.aiMaxTokens;
  }
  return merged;
}
