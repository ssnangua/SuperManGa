import { APP_LANGS } from "../langs";

export type TranslationProviderId =
  | "microsoft"
  | "google"
  | "yandex"
  | "deepl"
  | "ai"
  | "caiyun"
  | "baidu"
  | "youdao"
  | "tencent"
  | "volcengine"
  | "aliyun";

export const TRANSLATION_PROVIDER_IDS: readonly TranslationProviderId[] = [
  "ai",
  "microsoft",
  "google",
  "yandex",
  "deepl",
  "caiyun",
  "baidu",
  "youdao",
  "tencent",
  "volcengine",
  "aliyun",
] as const;

/** Translation / OCR language list — not limited to UI locales. */
export const TRANSLATION_TARGET_LANGS = APP_LANGS;

export type TranslationTextDirection = "auto" | "horizontal" | "vertical";
export type TranslationTextAlign = "start" | "center" | "end";

export type TranslationAiProfile = {
  id: string;
  name: string;
  endpoint: {
    baseUrl: string;
    apiKey: string;
    model: string;
    maxTokens: number;
  };
  updatedAt: number;
};

export type TranslationSettings = {
  provider: TranslationProviderId;
  targetLang: string;
  sourceLang: string;
  /** Selected OCR engine id (see lib/ocr/engines). */
  ocrEngine: string;
  /** Primary text detector id (`off` | `ctd` | `default` | `yolo` | `saber_yolo`). */
  textDetector: string;
  /** Merge YSGYolo boxes into primary detector output (before merge). */
  enableAuxYolo: boolean;
  /** Split over-merged blocks using SaberYOLO OBB references. */
  enableSaberYoloRefine: boolean;
  /**
   * When translation mode is on, queue the whole book in the background.
   * Off: only the current page/spread and its neighbors (like neural book pre-upscale).
   */
  bookPreTranslate: boolean;
  deeplApiKey: string;
  aiProfiles: TranslationAiProfile[];
  activeAiProfileId: string;
  aiBaseUrl: string;
  aiApiKey: string;
  aiModel: string;
  aiMaxTokens: number;
  /** Per AI vendor API keys (runtime + vault only; never written to reader.json). */
  aiVendorKeys: Record<string, string>;
  /** Per AI vendor selected model id (persisted in settings). */
  aiVendorModels: Record<string, string>;
  baiduAppId: string;
  baiduSecret: string;
  youdaoAppKey: string;
  youdaoAppSecret: string;
  tencentSecretId: string;
  tencentSecretKey: string;
  tencentRegion: string;
  volcAccessKeyId: string;
  volcSecretKey: string;
  volcRegion: string;
  aliyunAccessKeyId: string;
  aliyunAccessKeySecret: string;
  /** Caiyun Lingocloud token. */
  caiyunToken: string;
  /** Overlay text style (frontend-only; not used by OCR pipeline). */
  textFontSize: number;
  textAutoFontSize: boolean;
  textFontFamily: string;
  textDirection: TranslationTextDirection;
  textLineSpacing: number;
  textAlign: TranslationTextAlign;
  textColor: string;
  textAutoColor: boolean;
  textStrokeEnabled: boolean;
  textStrokeWidth: number;
  textStrokeAutoScale: boolean;
};

export const TRANSLATION_SECRET_FIELD_KEYS = [
  "deeplApiKey",
  "aiApiKey",
  "baiduAppId",
  "baiduSecret",
  "youdaoAppKey",
  "youdaoAppSecret",
  "tencentSecretId",
  "tencentSecretKey",
  "volcAccessKeyId",
  "volcSecretKey",
  "aliyunAccessKeyId",
  "aliyunAccessKeySecret",
  "caiyunToken",
] as const;

export type TranslationSecretFieldKey = (typeof TRANSLATION_SECRET_FIELD_KEYS)[number];

export type TranslationProviderSecrets = Partial<Record<TranslationSecretFieldKey, string>> & {
  aiProfileKeys?: Record<string, string>;
  /** AI 服务商预设 id → API Key */
  aiVendorKeys?: Record<string, string>;
};

export type TranslationResponse =
  | { ok: true; translated: string; provider: TranslationProviderId }
  | { ok: false; message: string };
