import type { MessageKey } from "../../i18n";

export type ChatApiProviderPreset = {
  id: string;
  labelKey: MessageKey;
  baseUrl: string;
  descKey?: MessageKey;
  custom?: boolean;
};

export const CHAT_API_PROVIDER_CUSTOM_ID = "custom-openai-compat";

export const CHAT_API_PROVIDER_PRESETS: readonly ChatApiProviderPreset[] = [
  // Saber default AI vendor for translation — keep first among cloud presets.
  {
    id: "siliconflow",
    labelKey: "aiPresetSiliconflow",
    baseUrl: "https://api.siliconflow.cn/v1",
  },
  {
    id: "deepseek",
    labelKey: "aiPresetDeepseek",
    baseUrl: "https://api.deepseek.com/v1",
  },
  {
    id: "dashscope",
    labelKey: "aiPresetDashscope",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
  },
  {
    id: "zhipu",
    labelKey: "aiPresetZhipu",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
  },
  {
    id: "moonshot",
    labelKey: "aiPresetMoonshot",
    baseUrl: "https://api.moonshot.cn/v1",
  },
  {
    id: "local-lmstudio",
    labelKey: "aiPresetLmstudio",
    baseUrl: "http://127.0.0.1:1234/v1",
  },
  {
    id: "local-ollama",
    labelKey: "aiPresetOllama",
    baseUrl: "http://127.0.0.1:11434/v1",
  },
  {
    id: "agnes",
    labelKey: "aiPresetAgnes",
    baseUrl: "https://apihub.agnes-ai.com/v1",
  },
  {
    id: "minimax",
    labelKey: "aiPresetMinimax",
    baseUrl: "https://api.minimaxi.com/v1",
  },
  {
    id: "mimo",
    labelKey: "aiPresetMimo",
    baseUrl: "https://api.xiaomimimo.com/v1",
  },
  {
    id: "openai",
    labelKey: "aiPresetOpenai",
    baseUrl: "https://api.openai.com/v1",
  },
  {
    id: "openrouter",
    labelKey: "aiPresetOpenrouter",
    baseUrl: "https://openrouter.ai/api/v1",
  },
  {
    id: "google-gemini",
    labelKey: "aiPresetGemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
  },
  {
    id: CHAT_API_PROVIDER_CUSTOM_ID,
    labelKey: "aiPresetCustom",
    baseUrl: "",
    descKey: "aiPresetCustomHint",
    custom: true,
  },
];

export function normalizeChatPresetBaseUrl(url: string): string {
  return url.trim().replace(/\/+$/, "");
}

export function findChatProviderPresetByBaseUrl(
  baseUrl: string,
): ChatApiProviderPreset | undefined {
  const n = normalizeChatPresetBaseUrl(baseUrl);
  if (!n) return undefined;
  return CHAT_API_PROVIDER_PRESETS.find(
    (p) => !p.custom && normalizeChatPresetBaseUrl(p.baseUrl) === n,
  );
}

export function resolveChatProviderPresetIdFromBaseUrl(baseUrl: string): string {
  const hit = findChatProviderPresetByBaseUrl(baseUrl);
  if (hit) return hit.id;
  if (baseUrl.trim()) return CHAT_API_PROVIDER_CUSTOM_ID;
  return "";
}

export function isChatApiProviderCustomId(id: string): boolean {
  return id === CHAT_API_PROVIDER_CUSTOM_ID;
}

export function defaultChatApiProviderPreset(): ChatApiProviderPreset {
  return (
    CHAT_API_PROVIDER_PRESETS.find((p) => p.id === "siliconflow") ??
    CHAT_API_PROVIDER_PRESETS.find((p) => !p.custom) ??
    CHAT_API_PROVIDER_PRESETS[0]!
  );
}

export function mimoApiLikely(baseUrl: string): boolean {
  const u = normalizeChatPresetBaseUrl(baseUrl).toLowerCase();
  return u.includes("xiaomimimo.com") || u.includes("mimo.mi.com");
}

export function sortChatModelsForBaseUrl(baseUrl: string, models: readonly string[]): string[] {
  if (!mimoApiLikely(baseUrl)) return [...models];
  return models.filter((id) => {
    const m = id.toLowerCase();
    return !m.includes("-tts") && !m.includes("-asr");
  });
}
