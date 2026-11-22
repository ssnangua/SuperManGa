<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { type MessageKey } from "../i18n";
import { installedOcrAssetIds } from "../lib/ocr/assets";
import {
  clampSourceLangForOcrEngine,
  normalizeOcrEngineId,
  ocrEngineMeta,
  ocrEngineSupportsSourceLang,
  type OcrEngineId,
} from "../lib/ocr/engines";
import {
  auxYoloAllowed,
  normalizeTextDetectorId,
  saberYoloRefineAllowed,
  textDetectorMeta,
} from "../lib/ocr/detectors";
import { buildTextDetectorSelectOptions } from "../lib/ocr/detectorSelectOptions";
import {
  buildOcrEngineSelectOptions,
  ocrEngineSelectNote,
} from "../lib/ocr/engineSelectOptions";
import { formatTranslationError } from "../lib/translation/errors";
import {
  ensureTextFontFamily,
  getCachedSystemFonts,
  listSystemFontsCached,
} from "../lib/translation/fonts";
import { buildTranslationProviderSelectOptions } from "../lib/translation/providerSelectOptions";
import {
  regionSuggestItems,
  TENCENT_REGION_OPTIONS,
  VOLC_REGION_OPTIONS,
} from "../lib/translation/regions";
import {
  TRANSLATION_TARGET_LANGS,
  type TranslationProviderId,
  type TranslationResponse,
  type TranslationTextAlign,
  type TranslationTextDirection,
} from "../lib/translation/types";
import { notifyError, notifySuccess, notifyWarning } from "../lib/appToast";
import {
  CHAT_API_PROVIDER_CUSTOM_ID,
  CHAT_API_PROVIDER_PRESETS,
  defaultChatApiProviderPreset,
  findChatProviderPresetByBaseUrl,
  isChatApiProviderCustomId,
  resolveChatProviderPresetIdFromBaseUrl,
  sortChatModelsForBaseUrl,
} from "../lib/translation/apiPresets";
import { secretsEncryptionStatus } from "../lib/translation/secrets";
import {
  detectorAssetsOpen,
  inTauri,
  ocrAssetsOpen,
  ortRuntimeReady,
  refreshTranslationPrereqs,
  settings,
} from "../reader";
import { langFlagCode } from "../lib/langFlags";
import AppCustomSelect, {
  type AppSelectOption,
  type AppSelectToggleOption,
} from "./AppCustomSelect.vue";
import AppPullFlashButton, { type AppPullFlashDone } from "./AppPullFlashButton.vue";
import HexColorField from "./HexColorField.vue";
import Icon from "./Icon.vue";
import RadioGroup from "./RadioGroup.vue";
import RuntimeSettingsBlock from "./RuntimeSettingsBlock.vue";
import SuggestInput, { type SuggestInputItem } from "./SuggestInput.vue";
import SwitchToggle from "./SwitchToggle.vue";
const { t, locale } = useI18n();

const tencentRegionItems = computed(() =>
  regionSuggestItems(TENCENT_REGION_OPTIONS, String(locale.value)),
);
const volcRegionItems = computed(() =>
  regionSuggestItems(VOLC_REGION_OPTIONS, String(locale.value)),
);

const TRANSLATION_TEST_DEFAULT = "Hello";
const translationTestSuggestions: readonly SuggestInputItem[] = [
  { id: "Hello" },
  { id: "你好" },
];
const translationTestText = ref(TRANSLATION_TEST_DEFAULT);
const translationTestLoading = ref(false);

const installedAssetIds = ref<string[]>([]);
const showSecret = ref<Record<string, boolean>>({});
const secretStorageHint = ref(t("secretStorageHintOs"));
const aiProviderExplicitId = ref("");
const aiModelOptions = ref<string[]>([]);
const aiModelsLoading = ref(false);
const aiPullBtnRef = ref<InstanceType<typeof AppPullFlashButton> | null>(null);
const systemFonts = ref<string[]>([...(getCachedSystemFonts() ?? [])]);
const systemFontsLoading = ref(false);

async function refreshInstalledOcrAssets() {
  installedAssetIds.value = await installedOcrAssetIds();
}

const currentTextDetector = computed(() =>
  normalizeTextDetectorId(settings.translation.textDetector),
);

const textDetectorOptions = computed<AppSelectOption[]>(() =>
  buildTextDetectorSelectOptions(installedAssetIds.value, ortRuntimeReady.value),
);

const textDetectorHint = computed(() => {
  const opt = textDetectorMeta(currentTextDetector.value);
  return t(opt.hintKey as MessageKey);
});

const auxYoloPackInstalled = computed(() =>
  installedAssetIds.value.includes("detector-yolo"),
);
const refinePackInstalled = computed(() =>
  installedAssetIds.value.includes("detector-saber-yolo"),
);
const auxYoloOn = computed(
  () => settings.translation.enableAuxYolo && auxYoloAllowed(currentTextDetector.value),
);
const refineOn = computed(
  () =>
    settings.translation.enableSaberYoloRefine && saberYoloRefineAllowed(currentTextDetector.value),
);
const auxYoloDisabled = computed(
  () =>
    !auxYoloAllowed(currentTextDetector.value)
    || !ortRuntimeReady.value
    || !auxYoloPackInstalled.value,
);
const refineDisabled = computed(
  () =>
    !saberYoloRefineAllowed(currentTextDetector.value)
    || !ortRuntimeReady.value
    || !refinePackInstalled.value,
);
const textDetectorToggleOptions = computed<AppSelectToggleOption[]>(() => {
  const auxMissing = !auxYoloPackInstalled.value;
  const refineMissing = !refinePackInstalled.value;
  return [
    {
      id: "aux-yolo",
      label: t("textDetectorAuxYolo"),
      labelSuffix: auxMissing ? t("modelNotInstalled") : undefined,
      packMissing: auxMissing,
      description: t("textDetectorAuxYoloHint"),
      disabled: auxYoloDisabled.value,
      on: auxYoloOn.value,
      badge: t("textDetectorAuxSuffix"),
    },
    {
      id: "saber-yolo-refine",
      label: t("textDetectorRefine"),
      labelSuffix: refineMissing ? t("modelNotInstalled") : undefined,
      packMissing: refineMissing,
      description: t("textDetectorRefineHint"),
      disabled: refineDisabled.value,
      on: refineOn.value,
      badge: t("textDetectorRefineSuffix"),
    },
  ];
});

function onTextDetector(id: string) {
  const next = normalizeTextDetectorId(id);
  if (textDetectorOptions.value.find((item) => item.id === next)?.disabled) return;
  settings.translation.textDetector = next;
  if (!auxYoloAllowed(next)) settings.translation.enableAuxYolo = false;
  if (!saberYoloRefineAllowed(next)) settings.translation.enableSaberYoloRefine = false;
}

function onTextDetectorToggle(id: string) {
  if (id === "aux-yolo") {
    if (auxYoloDisabled.value) return;
    settings.translation.enableAuxYolo = !settings.translation.enableAuxYolo;
    return;
  }
  if (id === "saber-yolo-refine") {
    if (refineDisabled.value) return;
    settings.translation.enableSaberYoloRefine = !settings.translation.enableSaberYoloRefine;
  }
}

const ocrHint = computed(() => {
  const id = settings.translation.ocrEngine as OcrEngineId;
  return ocrEngineSelectNote(ocrEngineMeta(id));
});

const ocrEngineOptions = computed<AppSelectOption[]>(() =>
  buildOcrEngineSelectOptions(installedAssetIds.value, ortRuntimeReady.value),
);

watch([ocrAssetsOpen, detectorAssetsOpen], () => {
  void refreshInstalledOcrAssets();
  void refreshTranslationPrereqs();
});

function applySystemFonts(fonts: readonly string[]) {
  systemFonts.value = [...fonts];
  settings.translation.textFontFamily = ensureTextFontFamily(
    settings.translation.textFontFamily,
    fonts,
  );
}

function paintFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

async function loadSystemFonts() {
  if (!inTauri()) {
    systemFontsLoading.value = false;
    return;
  }
  try {
    // Let the short loading panel paint before the IPC round-trip / list mount.
    await paintFrame();
    const fonts = await listSystemFontsCached();
    applySystemFonts(fonts);
  } catch {
    systemFonts.value = [];
  } finally {
    systemFontsLoading.value = false;
  }
}

function onFontSelectOpenChange(isOpen: boolean) {
  if (!isOpen) return;
  const cached = getCachedSystemFonts();
  if (cached?.length) {
    applySystemFonts(cached);
    systemFontsLoading.value = false;
    return;
  }
  if (systemFonts.value.length > 0) {
    systemFontsLoading.value = false;
    return;
  }
  // Sync before paint so the first open frame shows the short loading panel.
  systemFontsLoading.value = true;
  void loadSystemFonts();
}

/** Resolve fonts for the closed trigger label without flashing the dropdown loader. */
async function prefetchSystemFonts() {
  if (!inTauri()) return;
  const cached = getCachedSystemFonts();
  if (cached?.length) {
    applySystemFonts(cached);
    return;
  }
  try {
    const fonts = await listSystemFontsCached();
    applySystemFonts(fonts);
  } catch {
    /* keep empty; dropdown open can retry with loading UI */
  }
}

onMounted(async () => {
  void refreshInstalledOcrAssets();
  // Fill empty textFontFamily with default heiti once fonts are known;
  // ensureTextFontFamily keeps any already-saved valid face.
  void prefetchSystemFonts();
  if (settings.translation.provider === "ai") {
    ensureDefaultAiVendor();
    const id = aiVendorId.value;
    // Restore first —never save empty flat fields over populated vendor slots.
    if (id) restoreAiVendorSlot(id);
  }
  void secretsEncryptionStatus().then((status) => {
    secretStorageHint.value = status.available ? t("secretStorageHintOs") : t("secretStorageHintFallback");
  });
});

const sourceLangOptions = computed<AppSelectOption[]>(() => {
  const engine = normalizeOcrEngineId(settings.translation.ocrEngine);
  const langs = TRANSLATION_TARGET_LANGS.filter((opt) =>
    ocrEngineSupportsSourceLang(engine, opt.code),
  ).map((opt) => ({
    id: opt.code,
    label: t(opt.labelKey as MessageKey),
    flag: langFlagCode(opt.code),
  }));
  return [{ id: "auto", label: t("langAuto"), icon: "language" }, ...langs];
});

const targetLangOptions = computed<AppSelectOption[]>(() =>
  TRANSLATION_TARGET_LANGS.map((opt) => ({
    id: opt.code,
    label: t(opt.labelKey as MessageKey),
    flag: langFlagCode(opt.code),
  })),
);

const providerOptions = computed<AppSelectOption[]>(() =>
  buildTranslationProviderSelectOptions(),
);

const fontOptions = computed<AppSelectOption[]>(() =>
  systemFonts.value.map((name) => ({
    id: name,
    label: name,
    labelFontFamily: name,
  })),
);

const textDirectionOptions = computed(() => [
  { id: "auto", label: t("textDirectionAuto") },
  { id: "horizontal", label: t("textDirectionHorizontal") },
  { id: "vertical", label: t("textDirectionVertical") },
]);

const textAlignOptions = computed(() => [
  { id: "start", label: t("textAlignStart") },
  { id: "center", label: t("textAlignCenter") },
  { id: "end", label: t("textAlignEnd") },
]);

const textDirectionModel = computed({
  get: () => settings.translation.textDirection,
  set: (id: string) => {
    settings.translation.textDirection = id as TranslationTextDirection;
  },
});

const textAlignModel = computed({
  get: () => settings.translation.textAlign,
  set: (id: string) => {
    settings.translation.textAlign = id as TranslationTextAlign;
  },
});

function onTextFontFamily(id: string) {
  settings.translation.textFontFamily = id;
}

const aiVendorOptions = computed<AppSelectOption[]>(() =>
  CHAT_API_PROVIDER_PRESETS.map((p) => ({
    id: p.id,
    label: t(p.labelKey),
    description: p.descKey ? t(p.descKey) : p.baseUrl,
  })),
);

const aiVendorId = computed(() => {
  if (aiProviderExplicitId.value) return aiProviderExplicitId.value;
  return resolveChatProviderPresetIdFromBaseUrl(settings.translation.aiBaseUrl);
});

watch(
  () => settings.translation.aiBaseUrl,
  (url) => {
    const prevVendorId = aiProviderExplicitId.value;
    let nextVendorId = "";
    const hit = findChatProviderPresetByBaseUrl(url);
    if (hit) nextVendorId = hit.id;
    else if (url.trim()) nextVendorId = CHAT_API_PROVIDER_CUSTOM_ID;
    else if (prevVendorId && !isChatApiProviderCustomId(prevVendorId)) {
      nextVendorId = CHAT_API_PROVIDER_CUSTOM_ID;
    }

    if (prevVendorId && nextVendorId && prevVendorId !== nextVendorId) {
      saveAiVendorSlot(prevVendorId);
      restoreAiVendorSlot(nextVendorId);
      aiModelOptions.value = [];
    } else if (!prevVendorId && nextVendorId) {
      // First bind after load: pull key/model from the vendor slot into flat fields.
      restoreAiVendorSlot(nextVendorId);
    }
    aiProviderExplicitId.value = nextVendorId;
  },
  { immediate: true },
);

watch(
  () => [settings.translation.aiApiKey, settings.translation.aiModel] as const,
  () => {
    if (settings.translation.provider !== "ai") return;
    saveAiVendorSlot();
  },
);

function saveAiVendorSlot(vendorId = aiVendorId.value) {
  if (!vendorId) return;
  if (!settings.translation.aiVendorKeys) settings.translation.aiVendorKeys = {};
  if (!settings.translation.aiVendorModels) settings.translation.aiVendorModels = {};
  const key = settings.translation.aiApiKey.trim();
  // Only write non-empty values; clearing the input should not wipe the saved slot
  // until the user switches vendor or persists an intentional empty override via sync.
  if (key) settings.translation.aiVendorKeys[vendorId] = key;
  const model = settings.translation.aiModel.trim();
  if (model) settings.translation.aiVendorModels[vendorId] = model;
}

function restoreAiVendorSlot(vendorId: string) {
  settings.translation.aiApiKey = settings.translation.aiVendorKeys?.[vendorId]?.trim() || "";
  settings.translation.aiModel = settings.translation.aiVendorModels?.[vendorId]?.trim() || "";
}

function ensureDefaultAiVendor() {
  if (settings.translation.aiBaseUrl.trim() || aiProviderExplicitId.value) return;
  const preset = defaultChatApiProviderPreset();
  aiProviderExplicitId.value = preset.id;
  if (preset.baseUrl) settings.translation.aiBaseUrl = preset.baseUrl;
  restoreAiVendorSlot(preset.id);
}

function onAiVendor(id: string) {
  if (id === aiVendorId.value) return;
  saveAiVendorSlot();
  aiProviderExplicitId.value = id;
  aiModelOptions.value = [];
  if (isChatApiProviderCustomId(id)) {
    settings.translation.aiBaseUrl = "";
  } else {
    const hit = CHAT_API_PROVIDER_PRESETS.find((p) => p.id === id && !p.custom);
    if (hit?.baseUrl) settings.translation.aiBaseUrl = hit.baseUrl;
  }
  restoreAiVendorSlot(id);
}

async function refreshAiModels(opts?: { pullDone?: AppPullFlashDone }) {
  const pullDone = opts?.pullDone;
  if (!inTauri() || !settings.translation.aiBaseUrl.trim()) {
    pullDone?.(false);
    return;
  }
  aiModelsLoading.value = true;
  let ok = false;
  try {
    const models = await invoke<string[]>("list_ai_models", {
      baseUrl: settings.translation.aiBaseUrl,
      apiKey: settings.translation.aiApiKey,
    });
    aiModelOptions.value = sortChatModelsForBaseUrl(settings.translation.aiBaseUrl, models);
    ok = true;
    const cur = settings.translation.aiModel.trim();
    if (aiModelOptions.value.length > 0 && (!cur || !aiModelOptions.value.includes(cur))) {
      settings.translation.aiModel = aiModelOptions.value[0]!;
    }
    saveAiVendorSlot();
  } catch (error) {
    aiModelOptions.value = [];
    ok = false;
    notifyError(formatTranslationError(error));
  } finally {
    aiModelsLoading.value = false;
    if (pullDone) pullDone(ok);
    else aiPullBtnRef.value?.clearStaleFailOnSilentSuccess(ok);
  }
}

/** Prefer driving the pull-button flash when auto-pulling (API key / focus). */
function triggerAiModelPull() {
  const btn = aiPullBtnRef.value;
  if (btn) {
    btn.beginPull((done) => {
      void refreshAiModels({ pullDone: done });
    });
    return;
  }
  void refreshAiModels();
}

function onAiModelFocus() {
  if (aiModelsLoading.value || aiModelOptions.value.length > 0) return;
  triggerAiModelPull();
}

function onAiApiKeyChange() {
  if (settings.translation.provider !== "ai") return;
  if (!settings.translation.aiBaseUrl.trim()) return;
  triggerAiModelPull();
}

function toggleSecret(id: string) {
  showSecret.value = { ...showSecret.value, [id]: !showSecret.value[id] };
}

async function runTranslationTest(opts?: { pullDone?: AppPullFlashDone }) {
  const pullDone = opts?.pullDone;
  if (!inTauri()) {
    notifyError(t("desktopOnly"));
    pullDone?.(false);
    return;
  }
  const text = translationTestText.value.trim();
  if (!text) {
    notifyWarning(t("trErrEmptyInput"));
    pullDone?.("abort");
    return;
  }
  translationTestLoading.value = true;
  let ok = false;
  try {
    const result = await invoke<TranslationResponse>("translate_text_cmd", {
      args: {
        text,
        settings: settings.translation,
        sourceLang: "auto",
      },
    });
    if (!result.ok) {
      notifyError(formatTranslationError(result.message));
    } else {
      notifySuccess(result.translated, 2000);
      ok = true;
    }
  } catch (error) {
    notifyError(formatTranslationError(error));
  } finally {
    translationTestLoading.value = false;
    pullDone?.(ok);
  }
}

function onOcrEngine(id: string) {
  const engine = normalizeOcrEngineId(id);
  settings.translation.ocrEngine = engine;
  settings.translation.sourceLang = clampSourceLangForOcrEngine(
    engine,
    settings.translation.sourceLang,
  );
}

function onSourceLang(id: string) {
  settings.translation.sourceLang = id;
}

function onTargetLang(id: string) {
  settings.translation.targetLang = id;
}

function onProvider(id: string) {
  settings.translation.provider = id as TranslationProviderId;
  if (id === "ai") ensureDefaultAiVendor();
  else aiModelOptions.value = [];
}
</script>

<template>
  <div class="app-tab-pane translation-settings-panel" role="tabpanel">
    <div class="settings-body neural-settings">
      <RuntimeSettingsBlock :active="true" />
    </div>

    <div class="settings-body">
      <h3 class="settings-section-title">{{ t("ocrTranslateSection") }}</h3>
      <div class="form-row form-row-label-wide">
        <span>{{ t("textDetector") }}</span>
        <div class="settings-controls-row">
          <AppCustomSelect
            :model-value="currentTextDetector"
            :options="textDetectorOptions"
            :toggle-options="textDetectorToggleOptions"
            :ariaLabel="t('textDetector')"
            :scroll-max-height="320"
            :panel-min-width="360"
            @update:model-value="onTextDetector"
            @toggle="onTextDetectorToggle"
          />
          <button type="button" @click="detectorAssetsOpen = true">
            {{ t("textDetectorManage") }}
          </button>
        </div>
      </div>
      <p class="settings-hint">{{ textDetectorHint }}</p>
      <div class="form-row form-row-label-wide">
        <span>{{ t("ocrEngine") }}</span>
        <div class="settings-controls-row">
          <AppCustomSelect
            :model-value="settings.translation.ocrEngine"
            :options="ocrEngineOptions"
            :ariaLabel="t('ocrEngine')"
            :scroll-max-height="320"
            :panel-min-width="360"
            @update:model-value="onOcrEngine"
          />
          <button type="button" @click="ocrAssetsOpen = true">
            {{ t("ocrEngineManage") }}
          </button>
        </div>
      </div>
      <p class="settings-hint">{{ ocrHint }}</p>
      <div class="form-row form-row-label-wide">
        <span>{{ t("ocrSourceLang") }}</span>
        <AppCustomSelect
          :model-value="settings.translation.sourceLang"
          :options="sourceLangOptions"
          :ariaLabel="t('ocrSourceLang')"
          :scroll-max-height="280"
          @update:model-value="onSourceLang"
        />
      </div>
    </div>

    <div class="settings-body">
      <h3 class="settings-section-title">{{ t("settingsTabTranslate") }}</h3>
      <div class="form-row form-row-label-wide">
        <span>{{ t("translationTargetLang") }}</span>
        <AppCustomSelect
          :model-value="settings.translation.targetLang"
          :options="targetLangOptions"
          :ariaLabel="t('translationTargetLang')"
          :scroll-max-height="280"
          @update:model-value="onTargetLang"
        />
      </div>
      <div class="form-row form-row-label-wide">
        <span>{{ t("translationProvider") }}</span>
        <AppCustomSelect
          :model-value="settings.translation.provider"
          :options="providerOptions"
          :ariaLabel="t('translationProvider')"
          :scroll-max-height="320"
          :panel-min-width="360"
          @update:model-value="onProvider"
        />
      </div>
      <template v-if="settings.translation.provider === 'deepl'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("deeplApiKey") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.deeplApiKey"
              :type="showSecret.deepl ? 'text' : 'password'"
              autocomplete="off"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.deepl ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('deepl')"
            >
              <Icon :name="showSecret.deepl ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
      </template>
      <template v-else-if="settings.translation.provider === 'ai'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("aiVendor") }}</span>
          <AppCustomSelect
            :model-value="aiVendorId"
            :options="aiVendorOptions"
            :ariaLabel="t('aiVendor')"
            :scroll-max-height="320"
            :panel-min-width="360"
            @update:model-value="onAiVendor"
          />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("aiBaseUrl") }}</span>
          <input v-model="settings.translation.aiBaseUrl" type="text" autocomplete="off" />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("aiApiKey") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.aiApiKey"
              :type="showSecret.ai ? 'text' : 'password'"
              autocomplete="off"
              @change="onAiApiKeyChange"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.ai ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('ai')"
            >
              <Icon :name="showSecret.ai ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
        <div class="form-row form-row-label-wide">
          <span>{{ t("aiModel") }}</span>
          <div class="settings-controls-row">
            <SuggestInput
              :model-value="settings.translation.aiModel"
              :items="aiModelOptions.map((id) => ({ id }))"
              :placeholder="t('aiModel')"
              :aria-label="t('aiModel')"
              :filter-placeholder="t('aiModelFilterPlaceholder')"
              :scroll-max-height="260"
              @update:model-value="settings.translation.aiModel = $event"
              @focus="onAiModelFocus"
            />
            <AppPullFlashButton
              ref="aiPullBtnRef"
              :label="t('aiPullModels')"
              :busy="aiModelsLoading"
              @pull="(done) => void refreshAiModels({ pullDone: done })"
            />
          </div>
        </div>
      </template>
      <template v-else-if="settings.translation.provider === 'caiyun'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("caiyunToken") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.caiyunToken"
              :type="showSecret.caiyun ? 'text' : 'password'"
              autocomplete="off"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.caiyun ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('caiyun')"
            >
              <Icon :name="showSecret.caiyun ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
      </template>
      <template v-else-if="settings.translation.provider === 'baidu'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("baiduAppId") }}</span>
          <input v-model="settings.translation.baiduAppId" type="text" autocomplete="off" />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("baiduSecret") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.baiduSecret"
              :type="showSecret.baidu ? 'text' : 'password'"
              autocomplete="off"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.baidu ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('baidu')"
            >
              <Icon :name="showSecret.baidu ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
      </template>
      <template v-else-if="settings.translation.provider === 'youdao'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("youdaoAppKey") }}</span>
          <input v-model="settings.translation.youdaoAppKey" type="text" autocomplete="off" />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("youdaoAppSecret") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.youdaoAppSecret"
              :type="showSecret.youdao ? 'text' : 'password'"
              autocomplete="off"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.youdao ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('youdao')"
            >
              <Icon :name="showSecret.youdao ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
      </template>
      <template v-else-if="settings.translation.provider === 'tencent'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("tencentSecretId") }}</span>
          <input v-model="settings.translation.tencentSecretId" type="text" autocomplete="off" />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("tencentSecretKey") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.tencentSecretKey"
              :type="showSecret.tencent ? 'text' : 'password'"
              autocomplete="off"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.tencent ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('tencent')"
            >
              <Icon :name="showSecret.tencent ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
        <div class="form-row form-row-label-wide">
          <span>{{ t("tencentRegion") }}</span>
          <SuggestInput
            :model-value="settings.translation.tencentRegion"
            :items="tencentRegionItems"
            :placeholder="t('tencentRegion')"
            :aria-label="t('tencentRegion')"
            :scroll-max-height="260"
            @update:model-value="settings.translation.tencentRegion = $event"
          />
        </div>
      </template>
      <template v-else-if="settings.translation.provider === 'volcengine'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("volcAccessKeyId") }}</span>
          <input v-model="settings.translation.volcAccessKeyId" type="text" autocomplete="off" />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("volcSecretKey") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.volcSecretKey"
              :type="showSecret.volc ? 'text' : 'password'"
              autocomplete="new-password"
              spellcheck="false"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.volc ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('volc')"
            >
              <Icon :name="showSecret.volc ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
        <div class="form-row form-row-label-wide">
          <span>{{ t("volcRegion") }}</span>
          <SuggestInput
            :model-value="settings.translation.volcRegion"
            :items="volcRegionItems"
            :placeholder="t('volcRegion')"
            :aria-label="t('volcRegion')"
            :scroll-max-height="220"
            @update:model-value="settings.translation.volcRegion = $event"
          />
        </div>
      </template>
      <template v-else-if="settings.translation.provider === 'aliyun'">
        <div class="form-row form-row-label-wide">
          <span>{{ t("aliyunAccessKeyId") }}</span>
          <input
            v-model="settings.translation.aliyunAccessKeyId"
            type="text"
            autocomplete="off"
          />
        </div>
        <div class="form-row form-row-label-wide">
          <span>{{ t("aliyunAccessKeySecret") }}</span>
          <div class="input-button-group">
            <input
              v-model="settings.translation.aliyunAccessKeySecret"
              :type="showSecret.aliyun ? 'text' : 'password'"
              autocomplete="off"
            />
            <button
              type="button"
              class="input-group-btn"
              :title="showSecret.aliyun ? t('hidePassword') : t('showPassword')"
              @click="toggleSecret('aliyun')"
            >
              <Icon :name="showSecret.aliyun ? 'eyeOpen' : 'eyeClose'" />
            </button>
          </div>
        </div>
        <p class="settings-hint">{{ secretStorageHint }}</p>
      </template>
      <div class="form-row form-row-label-wide">
        <span>{{ t("translationTest") }}</span>
        <div class="settings-controls-row">
          <SuggestInput
            v-model="translationTestText"
            :items="translationTestSuggestions"
            :placeholder="TRANSLATION_TEST_DEFAULT"
            :aria-label="t('translationTest')"
            :scroll-max-height="180"
          />
          <AppPullFlashButton
            :label="t('translationTestAction')"
            :busy="translationTestLoading"
            @pull="(done) => void runTranslationTest({ pullDone: done })"
          />
        </div>
      </div>
    </div>

    <div class="settings-body text-style-settings">
      <h3 class="settings-section-title">{{ t("textSettingsSection") }}</h3>
      <div class="form-row form-row-label-wide">
        <span>{{ t("textFontFamily") }}</span>
        <AppCustomSelect
          :model-value="settings.translation.textFontFamily"
          :options="fontOptions"
          :ariaLabel="t('textFontFamily')"
          :scroll-max-height="320"
          :panel-min-width="360"
          :loading="systemFontsLoading"
          :filter-placeholder="t('textFontFilterPlaceholder')"
          @update:model-value="onTextFontFamily"
          @open-change="onFontSelectOpenChange"
        />
      </div>
      <div class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textFontSize") }}</span>
          <div class="text-style-controls">
            <input
              v-if="!settings.translation.textAutoFontSize"
              v-model.number="settings.translation.textFontSize"
              type="number"
              min="1"
              step="1"
            />
            <label class="text-style-switch">
              <span>{{ t("textAutoFontSize") }}</span>
              <SwitchToggle v-model="settings.translation.textAutoFontSize" />
            </label>
          </div>
        </div>
      </div>
      <div class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textDirection") }}</span>
          <RadioGroup
            v-model="textDirectionModel"
            :options="textDirectionOptions"
            :aria-label="t('textDirection')"
          />
        </div>
      </div>
      <div class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textAlign") }}</span>
          <RadioGroup
            v-model="textAlignModel"
            :options="textAlignOptions"
            :aria-label="t('textAlign')"
          />
        </div>
      </div>
      <div class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textLineSpacing") }}</span>
          <input
            v-model.number="settings.translation.textLineSpacing"
            type="number"
            min="0.1"
            step="0.1"
          />
        </div>
      </div>
      <div class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textColor") }}</span>
          <div class="text-style-controls">
            <HexColorField
              v-if="!settings.translation.textAutoColor"
              v-model="settings.translation.textColor"
            />
            <label class="text-style-switch">
              <span>{{ t("textAutoColor") }}</span>
              <SwitchToggle v-model="settings.translation.textAutoColor" />
            </label>
          </div>
        </div>
      </div>
      <div class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textStroke") }}</span>
          <SwitchToggle v-model="settings.translation.textStrokeEnabled" />
        </div>
      </div>
      <div v-if="settings.translation.textStrokeEnabled" class="settings-row">
        <div class="settings-row-main">
          <span class="settings-label">{{ t("textStrokeWidth") }}</span>
          <div class="text-style-controls">
            <input
              v-if="!settings.translation.textStrokeAutoScale"
              v-model.number="settings.translation.textStrokeWidth"
              type="number"
              min="1"
              step="1"
            />
            <label class="text-style-switch">
              <span>{{ t("textStrokeAutoScale") }}</span>
              <SwitchToggle v-model="settings.translation.textStrokeAutoScale" />
            </label>
          </div>
        </div>
      </div>
    </div>

    <div class="settings-body neural-settings">
      <div class="form-row">
        <span>{{ t("bookPreTranslate") }}</span>
        <SwitchToggle
          v-model="settings.translation.bookPreTranslate"
          class="neural-switch"
          :aria-label="t('bookPreTranslate')"
        />
      </div>
      <p class="settings-hint neural-target-hint">{{ t("bookPreTranslateHint") }}</p>
    </div>
  </div>
</template>

<style scoped>
.text-style-controls {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
  margin-left: auto;
  min-width: 0;
}

.text-style-controls > input[type="number"] {
  width: 80px;
}

.text-style-switch {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
  font-size: 13px;
  color: var(--muted, #888);
}
</style>
