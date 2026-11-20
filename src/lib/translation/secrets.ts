import { invoke } from "@tauri-apps/api/core";
import { resolveChatProviderPresetIdFromBaseUrl } from "./apiPresets";
import { mergeTranslationSettings } from "./defaults";
import {
  TRANSLATION_SECRET_FIELD_KEYS,
  type TranslationProviderSecrets,
  type TranslationSettings,
} from "./types";

function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Volc Secret Access Key is standard Base64 and often ends with `=` / `==`.
 * Some store/autofill paths drop trailing padding; restore it from length mod 4.
 * Only touches alphabet-only strings (no `=` yet).
 */
export function restoreBase64Padding(raw: string): string {
  const t = raw.trim();
  if (!t || !/^[A-Za-z0-9+/]+$/.test(t)) return t;
  const mod = t.length % 4;
  if (mod === 2) return `${t}==`;
  if (mod === 3) return `${t}=`;
  return t;
}

function normalizeSecretField(key: string, value: string): string {
  const t = value.trim();
  if (!t) return t;
  if (key === "volcSecretKey") return restoreBase64Padding(t);
  return t;
}

function resolveAiVendorId(settings: TranslationSettings): string {
  return resolveChatProviderPresetIdFromBaseUrl(settings.aiBaseUrl);
}

/** Keep flat AI key / model mirrored into the current vendor slots. */
export function syncAiApiKeyIntoVendorSlot(settings: TranslationSettings): TranslationSettings {
  const next = mergeTranslationSettings(settings);
  const vendorId = resolveAiVendorId(next);
  if (vendorId) {
    const keyMap = { ...next.aiVendorKeys };
    const key = next.aiApiKey.trim();
    // Persist non-empty only — empty flat fields after vault strip must not erase slots.
    if (key) keyMap[vendorId] = key;
    next.aiVendorKeys = keyMap;

    const modelMap = { ...next.aiVendorModels };
    const model = next.aiModel.trim();
    if (model) modelMap[vendorId] = model;
    next.aiVendorModels = modelMap;
  }
  return next;
}

export function applyAiVendorKey(settings: TranslationSettings, vendorId: string): TranslationSettings {
  const next = mergeTranslationSettings(settings);
  next.aiApiKey = next.aiVendorKeys[vendorId]?.trim() || "";
  return next;
}

export function stripTranslationSecretsForDisk(settings: TranslationSettings): TranslationSettings {
  const next = syncAiApiKeyIntoVendorSlot(settings);
  for (const k of TRANSLATION_SECRET_FIELD_KEYS) next[k] = "";
  next.aiVendorKeys = {};
  next.aiProfiles = next.aiProfiles.map((p) => ({
    ...p,
    endpoint: { ...p.endpoint, apiKey: "" },
  }));
  return next;
}

export function collectTranslationSecrets(settings: TranslationSettings): TranslationProviderSecrets {
  const synced = syncAiApiKeyIntoVendorSlot(settings);
  const out: TranslationProviderSecrets = {};
  for (const k of TRANSLATION_SECRET_FIELD_KEYS) {
    const v = synced[k] ? normalizeSecretField(k, synced[k]!) : "";
    if (v) out[k] = v;
  }
  const aiProfileKeys: Record<string, string> = {};
  for (const p of synced.aiProfiles) {
    const key = p.endpoint.apiKey.trim();
    if (key) aiProfileKeys[p.id] = key;
  }
  if (Object.keys(aiProfileKeys).length) out.aiProfileKeys = aiProfileKeys;
  if (Object.keys(synced.aiVendorKeys).length) out.aiVendorKeys = { ...synced.aiVendorKeys };
  return out;
}

export function translationHasPlaintextSecrets(settings: Partial<TranslationSettings> | null | undefined): boolean {
  if (!settings) return false;
  for (const k of TRANSLATION_SECRET_FIELD_KEYS) {
    const v = settings[k];
    if (typeof v === "string" && v.trim()) return true;
  }
  if (settings.aiVendorKeys && typeof settings.aiVendorKeys === "object") {
    for (const value of Object.values(settings.aiVendorKeys)) {
      if (typeof value === "string" && value.trim()) return true;
    }
  }
  if (Array.isArray(settings.aiProfiles)) {
    for (const p of settings.aiProfiles) {
      if (p?.endpoint?.apiKey?.trim()) return true;
    }
  }
  return false;
}

export function applyTranslationSecrets(
  settings: TranslationSettings,
  secrets: TranslationProviderSecrets | null | undefined,
): TranslationSettings {
  const next = mergeTranslationSettings(settings);
  if (!secrets) return next;
  for (const k of TRANSLATION_SECRET_FIELD_KEYS) {
    const v = secrets[k];
    if (typeof v === "string" && v.trim()) next[k] = normalizeSecretField(k, v);
  }
  const profileKeys = secrets.aiProfileKeys;
  if (profileKeys) {
    for (const p of next.aiProfiles) {
      const vault = profileKeys[p.id];
      if (typeof vault === "string" && vault.trim()) p.endpoint.apiKey = vault;
    }
  }
  if (secrets.aiVendorKeys && typeof secrets.aiVendorKeys === "object") {
    next.aiVendorKeys = { ...next.aiVendorKeys, ...secrets.aiVendorKeys };
  }
  const vendorId = resolveAiVendorId(next);
  if (vendorId && next.aiVendorKeys[vendorId]?.trim()) {
    next.aiApiKey = next.aiVendorKeys[vendorId]!;
  } else {
    const active = next.aiProfiles.find((p) => p.id === next.activeAiProfileId) ?? next.aiProfiles[0];
    if (active && !next.aiApiKey.trim() && active.endpoint.apiKey.trim()) {
      next.aiApiKey = active.endpoint.apiKey;
    }
  }
  if (vendorId && next.aiVendorModels[vendorId]?.trim()) {
    // Prefer the per-vendor slot so switching vendors survives restarts.
    next.aiModel = next.aiVendorModels[vendorId]!;
  } else {
    const active = next.aiProfiles.find((p) => p.id === next.activeAiProfileId) ?? next.aiProfiles[0];
    if (active && !next.aiModel.trim() && active.endpoint.model.trim()) {
      next.aiModel = active.endpoint.model;
    }
  }
  if (next.aiApiKey.trim()) {
    const active = next.aiProfiles.find((p) => p.id === next.activeAiProfileId) ?? next.aiProfiles[0];
    if (active) active.endpoint.apiKey = next.aiApiKey;
    if (vendorId) next.aiVendorKeys[vendorId] = next.aiApiKey;
  }
  if (next.aiModel.trim() && vendorId) {
    next.aiVendorModels[vendorId] = next.aiModel;
    const active = next.aiProfiles.find((p) => p.id === next.activeAiProfileId) ?? next.aiProfiles[0];
    if (active) active.endpoint.model = next.aiModel;
  }
  return next;
}

function parseStringMap(raw: unknown): Record<string, string> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, string> = {};
  for (const [id, v] of Object.entries(raw as Record<string, unknown>)) {
    if (id.trim() && typeof v === "string" && v.trim()) out[id] = v;
  }
  return out;
}

export function parseTranslationSecretsBlob(raw: string): TranslationProviderSecrets {
  const t = raw.trim();
  if (!t) return {};
  try {
    const o = JSON.parse(t) as unknown;
    if (!o || typeof o !== "object") return {};
    const src = o as Record<string, unknown>;
    const out: TranslationProviderSecrets = {};
    for (const k of TRANSLATION_SECRET_FIELD_KEYS) {
      const v = src[k];
      if (typeof v === "string" && v.trim()) out[k] = v;
    }
    const profileKeys = parseStringMap(src.aiProfileKeys);
    if (Object.keys(profileKeys).length) out.aiProfileKeys = profileKeys;
    const vendorKeys = parseStringMap(src.aiVendorKeys);
    if (Object.keys(vendorKeys).length) out.aiVendorKeys = vendorKeys;
    return out;
  } catch {
    return {};
  }
}

export function serializeTranslationSecrets(secrets: TranslationProviderSecrets): string {
  const clean: TranslationProviderSecrets = {};
  for (const k of TRANSLATION_SECRET_FIELD_KEYS) {
    const v = secrets[k] ? normalizeSecretField(k, secrets[k]!) : "";
    if (v) clean[k] = v;
  }
  if (secrets.aiProfileKeys) {
    const map = parseStringMap(secrets.aiProfileKeys);
    if (Object.keys(map).length) clean.aiProfileKeys = map;
  }
  if (secrets.aiVendorKeys) {
    const map = parseStringMap(secrets.aiVendorKeys);
    if (Object.keys(map).length) clean.aiVendorKeys = map;
  }
  return Object.keys(clean).length ? JSON.stringify(clean) : "";
}

export async function readTranslationSecretsVault(): Promise<string> {
  if (!inTauri()) return "";
  return invoke<string>("secrets_translation_get");
}

export async function writeTranslationSecretsVault(payload: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("secrets_translation_set", { value: payload });
}

export async function hydrateTranslationSecretsFromVault(
  settings: TranslationSettings,
): Promise<TranslationSettings> {
  try {
    const raw = await readTranslationSecretsVault();
    return applyTranslationSecrets(settings, parseTranslationSecretsBlob(raw));
  } catch {
    return settings;
  }
}

export async function persistTranslationSecretsToVault(settings: TranslationSettings): Promise<void> {
  await writeTranslationSecretsVault(serializeTranslationSecrets(collectTranslationSecrets(settings)));
}

export async function secretsEncryptionStatus(): Promise<{ available: boolean; backend: string }> {
  if (!inTauri()) return { available: false, backend: "none" };
  try {
    return await invoke<{ available: boolean; backend: string }>("secrets_encryption_status");
  } catch {
    return { available: false, backend: "none" };
  }
}
