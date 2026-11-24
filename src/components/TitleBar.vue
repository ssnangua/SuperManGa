<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { locales, type Locale, type MessageKey } from "../i18n";
import Icon from "./Icon.vue";
import BarMenu from "./BarMenu.vue";
import RangeSlider from "./RangeSlider.vue";
import {
  isNeuralProfile,
  neuralProfileForAsset,
  type ClassicProfile,
  type RealtimeProfile,
  type RenderProfile,
} from "../lib/render/profile";
import { IMAGE_ASSET_CATALOG, installedNeuralIds } from "../lib/render/assets";
import {
  clampSourceLangForOcrEngine,
  normalizeOcrEngineId,
  ocrEngineMeta,
  ocrEngineSupportsSourceLang,
  type OcrEngineId,
} from "../lib/ocr/engines";
import { buildOcrEngineSelectOptions } from "../lib/ocr/engineSelectOptions";
import {
  auxYoloAllowed,
  normalizeTextDetectorId,
  saberYoloRefineAllowed,
  textDetectorMeta,
  type TextDetectorId,
} from "../lib/ocr/detectors";
import { buildTextDetectorSelectOptions } from "../lib/ocr/detectorSelectOptions";
import { modelMenuDisabledTitle } from "../lib/ocr/modelMenuTip";
import { buildTranslationProviderSelectOptions } from "../lib/translation/providerSelectOptions";
import { langFlagCode } from "../lib/langFlags";
import "../lib/langFlag.css";
import {
  TRANSLATION_TARGET_LANGS,
  type TranslationProviderId,
} from "../lib/translation/types";
import AppSelectOptionRow from "./AppSelectOptionRow.vue";
import {
  clearHistory,
  closeComic,
  colorAdjustOpen,
  comic,
  copyImage,
  exitApp,
  globalBusy,
  history,
  normalizeColorPercent,
  openArchiveDialog,
  openComic,
  openFolderDialog,
  openNewWindow,
  openImageAssets,
  openImageInfo,
  magnifierOn,
  toggleMagnifier,
  openSettings as openSettingsDialog,
  openTranslationSettings,
  canEnableTranslation,
  installedOcrIds,
  ocrEnabled,
  ortRuntimeReady,
  pageIndex,
  refreshTranslationPrereqs,
  revealCurrent,
  retranslateCurrentPage,
  saveImage,
  setLocale,
  setRenderProfile,
  setIsolateCover,
  setSplitDoublePages,
  isolateCover,
  splitDoublePages,
  settings,
  settingsOpen,
  toggleDevtools,
  openLogs,
  toggleTheme,
  toggleTranslationMode,
  shortcutDisplay,
  shortcutsOpen,
  showAbout,
  chromeOverlay,
  chromeTop,
  type HistoryItem,
} from "../reader";
import { maskModalOpen } from "../lib/maskModal";
import { isMacDesktop } from "../lib/platform";
import {
  acceleratorToDisplayText,
  FIXED_DEVTOOLS_ACCEL,
  FIXED_LOGS_ACCEL,
} from "../lib/shortcuts";
import { checkForUpdates, openGithubRepo } from "../lib/updateCheck";
const { t, locale } = useI18n();
const mac = isMacDesktop();
const logsShortcutLabel = acceleratorToDisplayText(FIXED_LOGS_ACCEL, mac);
const devtoolsShortcutLabel = acceleratorToDisplayText(FIXED_DEVTOOLS_ACCEL, mac);

type MenuName = "file" | "image" | "color" | "translate" | "info" | "help";
type ColorKey = "brightness" | "contrast" | "saturation";

const props = defineProps<{ revealed?: boolean }>();
const overlayShown = computed(() => props.revealed !== false);

const open = ref<MenuName | null>(null);
const colorRows: { key: ColorKey; labelKey: "brightness" | "contrast" | "saturation" }[] = [
  { key: "brightness", labelKey: "brightness" },
  { key: "contrast", labelKey: "contrast" },
  { key: "saturation", labelKey: "saturation" },
];
const colorAdjusted = computed(
  () => settings.brightness !== 100 || settings.contrast !== 100 || settings.saturation !== 100,
);
const editingColor = ref<ColorKey | null>(null);
const colorDraft = ref("");
const colorInput = ref<HTMLInputElement | null>(null);
const neuralIds = ref<string[]>([]);
const localeLabels: Record<Locale, MessageKey> = {
  "zh-CN": "localeZhCN",
  "zh-TW": "localeZhTW",
  en: "localeEn",
  ja: "localeJa",
  ko: "localeKo",
};

const classicItems: { id: ClassicProfile; label: MessageKey }[] = [
  { id: "bilinear", label: "renderBilinear" },
  { id: "nearest", label: "renderNearest" },
  { id: "bicubic", label: "renderBicubic" },
  { id: "lanczos", label: "renderLanczos" },
  { id: "average", label: "renderAverage" },
  { id: "averageSharpWeak", label: "renderAverageSharpWeak" },
  { id: "averageSharpStrong", label: "renderAverageSharpStrong" },
];

const realtimeItems: { id: RealtimeProfile; label: MessageKey }[] = [
  { id: "anime4kWeak", label: "renderAnime4kWeak" },
  { id: "anime4kStrong", label: "renderAnime4kStrong" },
];

const neuralItems = computed(() => {
  const installed = new Set(neuralIds.value);
  const runtimeOk = ortRuntimeReady.value;
  return IMAGE_ASSET_CATALOG.filter((item) => item.kind === "neural").map((meta) => {
    const modelOk = installed.has(meta.id);
    const selectable = runtimeOk && modelOk;
    return {
      id: neuralProfileForAsset(meta.id),
      assetId: meta.id,
      labelKey: meta.nameKey as MessageKey,
      installed: modelOk,
      selectable,
      disabledTitle: modelMenuDisabledTitle({
        runtimeReady: runtimeOk,
        modelInstalled: modelOk,
      }),
    };
  });
});

async function refreshNeuralMenu() {
  await refreshTranslationPrereqs();
  neuralIds.value = await installedNeuralIds();
}

function chooseRender(profile: RenderProfile) {
  if (isNeuralProfile(profile)) {
    const item = neuralItems.value.find((entry) => entry.id === profile);
    if (!item?.selectable) return;
  }
  closeMenus();
  setRenderProfile(profile);
}

const historyOpen = ref(false);
const languageOpen = ref(false);
type TranslateSub = "detector" | "engine" | "source" | "target" | "provider" | null;
const translateSub = ref<TranslateSub>(null);
const translationReady = computed(() => canEnableTranslation());

const currentOcrEngine = computed(() =>
  normalizeOcrEngineId(settings.translation.ocrEngine),
);
const currentOcrEngineLabel = computed(() =>
  t(ocrEngineMeta(currentOcrEngine.value).nameKey as MessageKey),
);

const currentTextDetector = computed(() =>
  normalizeTextDetectorId(settings.translation.textDetector),
);
const textDetectorMenuOptions = computed(() =>
  buildTextDetectorSelectOptions(installedOcrIds.value, ortRuntimeReady.value),
);
const currentTextDetectorMenuLabel = computed(() => {
  const primary = t(textDetectorMeta(currentTextDetector.value).nameKey as MessageKey);
  if (currentTextDetector.value === "off") return primary;
  const parts = [primary];
  if (settings.translation.enableAuxYolo && auxYoloAllowed(currentTextDetector.value)) {
    parts.push(t("textDetectorAuxSuffix"));
  }
  let label = parts.length > 1 ? `${parts[0]} + ${parts[1]}` : parts[0]!;
  if (settings.translation.enableSaberYoloRefine && saberYoloRefineAllowed(currentTextDetector.value)) {
    label = `${label} + ${t("textDetectorRefineSuffix")}`;
  }
  return label;
});
const auxYoloMenuOn = computed(
  () => settings.translation.enableAuxYolo && auxYoloAllowed(currentTextDetector.value),
);
const refineMenuOn = computed(
  () =>
    settings.translation.enableSaberYoloRefine && saberYoloRefineAllowed(currentTextDetector.value),
);
const auxYoloPackInstalled = computed(() => installedOcrIds.value.includes("detector-yolo"));
const refinePackInstalled = computed(() =>
  installedOcrIds.value.includes("detector-saber-yolo"),
);
const auxYoloMenuDisabled = computed(
  () =>
    !auxYoloAllowed(currentTextDetector.value)
    || !ortRuntimeReady.value
    || !auxYoloPackInstalled.value,
);
const refineMenuDisabled = computed(
  () =>
    !saberYoloRefineAllowed(currentTextDetector.value)
    || !ortRuntimeReady.value
    || !refinePackInstalled.value,
);
const auxYoloMenuOption = computed(() => {
  const missing = !auxYoloPackInstalled.value;
  return {
    id: "aux-yolo",
    label: t("textDetectorAuxYolo"),
    labelSuffix: missing ? t("modelNotInstalled") : undefined,
    packMissing: missing,
    description: t("textDetectorAuxYoloHint"),
    disabled: auxYoloMenuDisabled.value,
  };
});
const refineMenuOption = computed(() => {
  const missing = !refinePackInstalled.value;
  return {
    id: "saber-yolo-refine",
    label: t("textDetectorRefine"),
    labelSuffix: missing ? t("modelNotInstalled") : undefined,
    packMissing: missing,
    description: t("textDetectorRefineHint"),
    disabled: refineMenuDisabled.value,
  };
});
const currentSourceLang = computed(() => settings.translation.sourceLang || "auto");
const currentSourceLangLabel = computed(() => {
  const code = currentSourceLang.value;
  if (code === "auto") return t("langAuto");
  const opt = TRANSLATION_TARGET_LANGS.find((lang) => lang.code === code);
  return opt ? t(opt.labelKey as MessageKey) : code;
});
const currentSourceFlag = computed(() => langFlagCode(currentSourceLang.value));
const currentTargetLang = computed(() => settings.translation.targetLang);
const currentTargetLangLabel = computed(() => {
  const opt = TRANSLATION_TARGET_LANGS.find((lang) => lang.code === currentTargetLang.value);
  return opt ? t(opt.labelKey as MessageKey) : currentTargetLang.value;
});
const currentTargetFlag = computed(() => langFlagCode(currentTargetLang.value));
const currentProviderLabel = computed(() =>
  t(`provider_${settings.translation.provider}` as MessageKey),
);

const ocrEngineMenuOptions = computed(() =>
  buildOcrEngineSelectOptions(installedOcrIds.value, ortRuntimeReady.value),
);

function chooseTextDetector(id: TextDetectorId) {
  if (textDetectorMenuOptions.value.find((item) => item.id === id)?.disabled) return;
  settings.translation.textDetector = id;
  if (!auxYoloAllowed(id)) settings.translation.enableAuxYolo = false;
  if (!saberYoloRefineAllowed(id)) settings.translation.enableSaberYoloRefine = false;
  closeMenus();
}

function toggleAuxYolo() {
  if (auxYoloMenuDisabled.value) return;
  settings.translation.enableAuxYolo = !settings.translation.enableAuxYolo;
}

function toggleSaberYoloRefine() {
  if (refineMenuDisabled.value) return;
  settings.translation.enableSaberYoloRefine = !settings.translation.enableSaberYoloRefine;
}

const sourceLangMenuItems = computed(() => {
  const engine = currentOcrEngine.value;
  const langs = TRANSLATION_TARGET_LANGS.filter((opt) =>
    ocrEngineSupportsSourceLang(engine, opt.code),
  ).map((opt) => ({
    id: opt.code,
    label: t(opt.labelKey as MessageKey),
    flag: langFlagCode(opt.code),
  }));
  return [{ id: "auto", label: t("langAuto"), flag: undefined as string | undefined }, ...langs];
});

const targetLangMenuItems = computed(() =>
  TRANSLATION_TARGET_LANGS.map((opt) => ({
    id: opt.code,
    label: t(opt.labelKey as MessageKey),
    flag: langFlagCode(opt.code),
  })),
);

const providerMenuOptions = computed(() => buildTranslationProviderSelectOptions());

function showTranslateSub(name: Exclude<TranslateSub, null>) {
  translateSub.value = name;
  if (name === "engine" || name === "detector") void refreshTranslationPrereqs();
}

function toggleTranslateSub(name: Exclude<TranslateSub, null>) {
  translateSub.value = translateSub.value === name ? null : name;
  if (translateSub.value === "engine" || translateSub.value === "detector") {
    void refreshTranslationPrereqs();
  }
}

function chooseOcrEngine(id: OcrEngineId) {
  if (ocrEngineMenuOptions.value.find((item) => item.id === id)?.disabled) return;
  settings.translation.ocrEngine = id;
  settings.translation.sourceLang = clampSourceLangForOcrEngine(
    id,
    settings.translation.sourceLang,
  );
  closeMenus();
}

function chooseSourceLang(code: string) {
  settings.translation.sourceLang = clampSourceLangForOcrEngine(currentOcrEngine.value, code);
  closeMenus();
}

function chooseTargetLang(code: string) {
  settings.translation.targetLang = code;
  closeMenus();
}

function chooseProvider(id: TranslationProviderId) {
  settings.translation.provider = id;
  closeMenus();
}

function fileName(item: HistoryItem) {
  const name = item.path.split(/[/\\]/).pop();
  return name || item.title;
}

function closeMenus() {
  open.value = null;
  historyOpen.value = false;
  languageOpen.value = false;
  translateSub.value = null;
}

watch(overlayShown, (shown) => {
  if (!shown) closeMenus();
});

const chromeBlocked = computed(() => maskModalOpen.value || globalBusy.value);

watch(chromeBlocked, (blocked) => {
  if (blocked) closeMenus();
});

function toggle(name: MenuName) {
  if (chromeBlocked.value) return;
  open.value = open.value === name ? null : name;
  if (name === "image" && open.value === "image") void refreshNeuralMenu();
  if (name === "translate" && open.value === "translate") void refreshTranslationPrereqs();
  if (name !== "file") {
    historyOpen.value = false;
    languageOpen.value = false;
  }
  if (name !== "translate") translateSub.value = null;
}

watch(settingsOpen, (isOpen, wasOpen) => {
  if (wasOpen && !isOpen) void refreshNeuralMenu();
});

watch(open, (name) => {
  colorAdjustOpen.value = name === "color";
  if (name !== "color") editingColor.value = null;
});

watch(colorAdjustOpen, (isOpen) => {
  if (isOpen && open.value !== "color") open.value = "color";
  if (!isOpen && open.value === "color") open.value = null;
});

function enter(name: MenuName) {
  if (chromeBlocked.value) return;
  if (open.value) {
    open.value = name;
    if (name === "image") void refreshNeuralMenu();
    if (name === "translate") void refreshTranslationPrereqs();
    if (name !== "file") {
      historyOpen.value = false;
      languageOpen.value = false;
    }
    if (name !== "translate") translateSub.value = null;
  }
}

function setColorInput(el: unknown) {
  colorInput.value = el instanceof HTMLInputElement ? el : null;
}

function beginColorEdit(key: ColorKey) {
  colorDraft.value = String(settings[key]);
  editingColor.value = key;
  void nextTick(() => {
    colorInput.value?.focus();
    colorInput.value?.select();
  });
}

function applyColorFromEvent(event?: Event) {
  const key = editingColor.value;
  if (!key) return;
  const raw = event?.target instanceof HTMLInputElement ? event.target.value : colorDraft.value;
  settings[key] = normalizeColorPercent(raw);
}

function commitColorEdit(event?: Event) {
  applyColorFromEvent(event);
  editingColor.value = null;
}

function cancelColorEdit() {
  editingColor.value = null;
}

function onColorKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    cancelColorEdit();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    commitColorEdit(event);
    return;
  }
  if (event.key === "ArrowUp" || event.key === "ArrowDown") {
    void nextTick(() => {
      const key = editingColor.value;
      if (!key || !(event.target instanceof HTMLInputElement)) return;
      settings[key] = normalizeColorPercent(event.target.value);
    });
  }
}

function resetColorValue(key: ColorKey) {
  settings[key] = 100;
  if (editingColor.value === key) colorDraft.value = "100";
}

function dismissMenusOutside() {
  closeMenus();
  // Overlay chrome: outside click closes the menu and collapses the top bar.
  if (chromeOverlay.value) chromeTop.value = false;
}

function onPointerDown(event: PointerEvent) {
  const target = event.target;
  if (!(target instanceof Node)) {
    dismissMenusOutside();
    return;
  }
  const bar = document.querySelector(".menubar");
  if (!bar || !bar.contains(target)) {
    dismissMenusOutside();
    return;
  }
  if (target instanceof Element && target.closest(".menu-label, .bar-menu")) return;
  closeMenus();
}

function run(action: () => void) {
  closeMenus();
  action();
}

function toggleSplitDoublePages() {
  setSplitDoublePages(!splitDoublePages.value);
}

function toggleIsolateCover() {
  setIsolateCover(!isolateCover.value);
}

async function openHistory(item: HistoryItem) {
  closeMenus();
  await openComic(item.path);
}

function openSettings() {
  closeMenus();
  openSettingsDialog();
}

function openShortcuts() {
  closeMenus();
  shortcutsOpen.value = true;
}

function showHistorySub() {
  languageOpen.value = false;
  historyOpen.value = true;
}

function toggleHistorySub() {
  languageOpen.value = false;
  historyOpen.value = !historyOpen.value;
}

function showLanguageSub() {
  historyOpen.value = false;
  languageOpen.value = true;
}

function toggleLanguageSub() {
  historyOpen.value = false;
  languageOpen.value = !languageOpen.value;
}

watch([open, historyOpen, languageOpen, translateSub], () => {
  void clampMenus();
});

async function clampMenus() {
  await nextTick();
  const pad = 8;
  const menus = [...document.querySelectorAll<HTMLElement>(".menubar .bar-menu")];
  for (const menu of menus) {
    menu.style.maxWidth = "";
    menu.style.maxHeight = "";
    menu.classList.remove("is-clamped", "flip");
    for (const child of menu.children) {
      if (child instanceof HTMLElement && child.classList.contains("history-list")) {
        child.style.maxHeight = "";
      }
    }
  }
  await nextTick();
  for (const menu of menus) {
    if (!menu.classList.contains("sub")) continue;
    const host = menu.offsetParent;
    if (!(host instanceof HTMLElement)) continue;
    const hostRect = host.getBoundingClientRect();
    const viewport = Math.max(0, window.innerWidth - pad * 2);
    const spaceRight = Math.max(0, window.innerWidth - pad - hostRect.right);
    const spaceLeft = Math.max(0, hostRect.left - pad);
    const needed = menu.getBoundingClientRect().width;
    const preferLeft = spaceRight < needed && spaceLeft > spaceRight;
    if (preferLeft) menu.classList.add("flip");
    const space = preferLeft ? spaceLeft : spaceRight;
    const width = Math.min(needed, space || viewport, viewport);
    if (width + 1 < needed) {
      menu.style.maxWidth = `${Math.floor(width)}px`;
      menu.classList.add("is-clamped");
    }
  }
  await nextTick();
  for (const menu of menus) {
    const rect = menu.getBoundingClientRect();
    if (rect.bottom <= window.innerHeight - pad) continue;
    const available = Math.floor(Math.max(120, window.innerHeight - pad - rect.top));
    const list = [...menu.children].find(
      (el): el is HTMLElement => el instanceof HTMLElement && el.classList.contains("history-list"),
    );
    if (list) {
      const foot = [...menu.children].find(
        (el): el is HTMLElement => el instanceof HTMLElement && el.classList.contains("history-foot"),
      );
      const styles = getComputedStyle(menu);
      const chrome =
        (parseFloat(styles.paddingTop) || 0) +
        (parseFloat(styles.paddingBottom) || 0) +
        (parseFloat(styles.borderTopWidth) || 0) +
        (parseFloat(styles.borderBottomWidth) || 0);
      const footH = foot?.offsetHeight ?? 0;
      list.style.maxHeight = `${Math.max(48, Math.floor(available - footH - chrome))}px`;
      continue;
    }
    menu.style.maxHeight = `${available}px`;
    menu.classList.add("is-clamped");
  }
}

onMounted(() => {
  document.addEventListener("pointerdown", onPointerDown);
  window.addEventListener("resize", clampMenus);
  window.addEventListener("blur", closeMenus);
  void refreshNeuralMenu();
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onPointerDown);
  window.removeEventListener("resize", clampMenus);
  window.removeEventListener("blur", closeMenus);
});
</script>

<template>
  <header class="menubar" :class="{ reveal: overlayShown, 'menu-open': !!open }">
    <div class="menubar-leading">
      <div class="menu-slot" @mouseenter="enter('file')">
        <button
          type="button"
          class="menu-label"
          :class="{ open: open === 'file' }"
          @click="toggle('file')"
        ><Icon name="menu" /><span>{{ t('menuFile') }}</span></button>
        <BarMenu v-if="open === 'file'">
          <button type="button" @click="run(openArchiveDialog)"><Icon name="open" /><span class="label">{{ t('openArchive') }}</span><span class="key">{{ shortcutDisplay('openArchive') }}</span></button>
          <button type="button" @click="run(openFolderDialog)"><Icon name="folderOpen" /><span class="label">{{ t('openFolder') }}</span><span class="key">{{ shortcutDisplay('openFolder') }}</span></button>
          <div class="has-sub" @mouseenter="showHistorySub" @mouseleave="historyOpen = false">
            <button type="button" @click="toggleHistorySub"><span class="menu-ico" aria-hidden="true"></span><span class="label">{{ t('history') }}</span><span class="key"><Icon name="expand" /></span></button>
            <BarMenu v-show="historyOpen" sub class="history-menu">
              <button v-if="history.length === 0" type="button" disabled>{{ t('historyEmpty') }}</button>
              <template v-else>
                <div class="history-list">
                  <button
                    v-for="(item, index) in history"
                    :key="item.path"
                    type="button"
                    :title="item.path"
                    @click="openHistory(item)"
                  >
                    <span class="label">{{ index + 1 }}: {{ fileName(item) }}</span>
                  </button>
                </div>
                <div class="history-foot">
                  <div class="sep"></div>
                  <button type="button" @click="run(() => { void clearHistory(); })"><span class="label">{{ t('clearHistory') }}</span></button>
                </div>
              </template>
            </BarMenu>
          </div>
          <div class="sep"></div>
          <button type="button" :disabled="!comic" @click="run(() => { void revealCurrent(); })"><span class="menu-ico" aria-hidden="true"></span><span class="label">{{ t('showInExplorer') }}</span></button>
          <button type="button" :disabled="!comic" @click="run(() => { void closeComic(); })"><span class="menu-ico" aria-hidden="true"></span><span class="label">{{ t('closeComic') }}</span><span class="key">{{ shortcutDisplay('closeComic') }}</span></button>
          <div class="sep"></div>
          <div class="has-sub" @mouseenter="showLanguageSub" @mouseleave="languageOpen = false">
            <button type="button" @click="toggleLanguageSub"><Icon name="language" /><span class="label">{{ t('language') }}</span><span class="key"><Icon name="expand" /></span></button>
            <BarMenu v-show="languageOpen" sub>
              <button
                v-for="item in locales"
                :key="item"
                type="button"
                class="choice"
                :class="{ on: locale === item }"
                @click="run(() => { void setLocale(item); })"
              ><span class="mark"><Icon v-if="locale === item" name="checked" /></span><span class="label">{{ t(localeLabels[item]) }}</span></button>
            </BarMenu>
          </div>
          <button type="button" @click="openSettings"><Icon name="setting" /><span class="label">{{ t('settings') }}</span><span class="key">{{ shortcutDisplay('settings') }}</span></button>
          <button type="button" @click="openShortcuts"><Icon name="keyboard" /><span class="label">{{ t('shortcuts') }}</span></button>
          <div class="sep"></div>
          <button type="button" @click="run(openNewWindow)"><Icon name="newWindow" /><span class="label">{{ t('openNewWindow') }}</span><span class="key">{{ shortcutDisplay('openNewWindow') }}</span></button>
          <button type="button" @click="run(() => { void exitApp(); })"><span class="menu-ico" aria-hidden="true"></span><span class="label">{{ t('closeWindow') }}</span><span class="key">{{ shortcutDisplay('closeWindow') }}</span></button>
        </BarMenu>
      </div>
      <div class="menu-slot" @mouseenter="enter('image')">
        <button
          type="button"
          class="menu-label"
          :class="{ open: open === 'image' }"
          @click="toggle('image')"
        ><Icon name="image" /><span>{{ t('image') }}</span></button>
        <BarMenu v-if="open === 'image'">
          <button
            v-for="item in classicItems"
            :key="item.id"
            type="button"
            class="choice"
            :class="{ on: settings.renderProfile === item.id }"
            @click="chooseRender(item.id)"
          ><span class="mark"><Icon v-if="settings.renderProfile === item.id" name="checked" /></span><span class="label">{{ t(item.label) }}</span></button>
          <div class="sep"></div>
          <button
            v-for="item in realtimeItems"
            :key="item.id"
            type="button"
            class="choice"
            :class="{ on: settings.renderProfile === item.id }"
            @click="chooseRender(item.id)"
          ><span class="mark"><Icon v-if="settings.renderProfile === item.id" name="checked" /></span><span class="label">{{ t(item.label) }}</span></button>
          <div class="sep"></div>
          <button
            v-for="item in neuralItems"
            :key="item.id"
            type="button"
            class="choice"
            :class="{ on: item.selectable && settings.renderProfile === item.id }"
            :disabled="!item.selectable"
            :title="item.disabledTitle"
            @click="chooseRender(item.id)"
          ><Icon name="enhance" /><span class="label">{{ t(item.labelKey) }}<span
            v-if="!item.installed"
            class="menu-model-missing"
          >{{ t('modelNotInstalled') }}</span></span></button>
          <button type="button" @click="run(openImageAssets)"><Icon name="setting" /><span class="label">{{ t('imageAssetsConfig') }}</span></button>
        </BarMenu>
      </div>
      <div class="menu-slot" @mouseenter="enter('color')">
        <button
          type="button"
          class="menu-label"
          :class="{ open: open === 'color', on: colorAdjusted }"
          @click="toggle('color')"
        ><Icon name="palette" /><span>{{ t('colorAdjust') }}</span></button>
        <BarMenu
          v-if="open === 'color'"
          class="color-adjust-menu"
          role="dialog"
          :aria-label="t('colorAdjust')"
          @click.stop
        >
          <table class="color-adjust-table">
            <tbody>
              <tr v-for="row in colorRows" :key="row.key">
                <th scope="row">{{ t(row.labelKey) }}</th>
                <td class="range">
                  <RangeSlider
                    :min="0"
                    :max="200"
                    :step="1"
                    tip-suffix="%"
                    :model-value="settings[row.key]"
                    @update:model-value="settings[row.key] = normalizeColorPercent($event)"
                  />
                </td>
                <td class="value">
                  <button
                    v-if="editingColor !== row.key"
                    type="button"
                    class="color-value"
                    @click="beginColorEdit(row.key)"
                  >
                    <span class="color-value-number">{{ settings[row.key] }}</span>
                    <span class="suffix">%</span>
                  </button>
                  <span v-else class="color-value-editing">
                    <input
                      :ref="setColorInput"
                      v-model="colorDraft"
                      type="number"
                      min="0"
                      max="200"
                      step="1"
                      @change="applyColorFromEvent"
                      @blur="commitColorEdit"
                      @keydown="onColorKeydown"
                    />
                    <span class="suffix">%</span>
                  </span>
                </td>
                <td class="reset">
                  <button type="button" class="color-adjust-reset" @click="resetColorValue(row.key)">
                    {{ t('colorAdjustReset') }}
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </BarMenu>
      </div>
      <div class="menu-slot" @mouseenter="enter('translate')">
        <button
          type="button"
          class="menu-label"
          :class="{ open: open === 'translate', on: ocrEnabled }"
          @click="toggle('translate')"
        ><Icon name="translate" /><span>{{ t('menuTranslate') }}</span></button>
        <BarMenu v-if="open === 'translate'">
          <button
            type="button"
            class="choice"
            :class="{ on: ocrEnabled }"
            :disabled="!comic || (!ocrEnabled && !translationReady)"
            :title="!comic || ocrEnabled || translationReady ? undefined : t('translationUnavailable')"
            @click="run(() => { void toggleTranslationMode(); })"
          >
            <span class="mark"><Icon v-if="ocrEnabled" name="checked" /></span>
            <span class="label">{{ t('enableTranslation') }}</span>
            <span class="key">{{ shortcutDisplay('ocrTranslate') }}</span>
          </button>
          <button
            type="button"
            :disabled="!comic || !translationReady"
            :title="!comic || translationReady ? undefined : t('translationUnavailable')"
            @click="run(() => { void retranslateCurrentPage(); })"
          >
            <span class="menu-ico" aria-hidden="true"></span>
            <span class="label">{{ t('retranslatePage') }}</span>
            <span class="key">{{ shortcutDisplay('ocrRetranslate') }}</span>
          </button>
          <div class="sep"></div>
          <div
            class="has-sub"
            @mouseenter="showTranslateSub('detector')"
            @mouseleave="translateSub = null"
          >
            <button type="button" @click="toggleTranslateSub('detector')">
              <span class="menu-ico" aria-hidden="true"></span>
              <span class="label menu-current">
                <span class="menu-current-prefix">{{ t('textDetectorMenu') }}：</span>
                <span>{{ currentTextDetectorMenuLabel }}</span>
              </span>
              <span class="key"><Icon name="expand" /></span>
            </button>
            <BarMenu v-show="translateSub === 'detector'" sub class="bar-menu-rich">
              <button
                v-for="item in textDetectorMenuOptions"
                :key="item.id"
                type="button"
                class="choice choice-rich"
                :class="{ on: currentTextDetector === item.id }"
                :disabled="item.disabled"
                :title="item.disabled
                  ? modelMenuDisabledTitle({
                      runtimeReady: ortRuntimeReady,
                      modelInstalled: !item.packMissing,
                    })
                  : undefined"
                @click="chooseTextDetector(item.id as TextDetectorId)"
              >
                <span class="mark"><Icon v-if="currentTextDetector === item.id" name="checked" /></span>
                <AppSelectOptionRow :option="item" />
              </button>
              <div class="sep"></div>
              <button
                type="button"
                class="choice choice-rich"
                :class="{ on: auxYoloMenuOn }"
                :disabled="auxYoloMenuDisabled"
                :title="auxYoloAllowed(currentTextDetector) && auxYoloMenuDisabled
                  ? modelMenuDisabledTitle({
                      runtimeReady: ortRuntimeReady,
                      modelInstalled: auxYoloPackInstalled,
                    })
                  : undefined"
                @click="toggleAuxYolo"
              >
                <span class="mark"><Icon v-if="auxYoloMenuOn" name="checked" /></span>
                <AppSelectOptionRow :option="auxYoloMenuOption" />
              </button>
              <button
                type="button"
                class="choice choice-rich"
                :class="{ on: refineMenuOn }"
                :disabled="refineMenuDisabled"
                :title="saberYoloRefineAllowed(currentTextDetector) && refineMenuDisabled
                  ? modelMenuDisabledTitle({
                      runtimeReady: ortRuntimeReady,
                      modelInstalled: refinePackInstalled,
                    })
                  : undefined"
                @click="toggleSaberYoloRefine"
              >
                <span class="mark"><Icon v-if="refineMenuOn" name="checked" /></span>
                <AppSelectOptionRow :option="refineMenuOption" />
              </button>
            </BarMenu>
          </div>
          <div class="sep"></div>
          <div
            class="has-sub"
            @mouseenter="showTranslateSub('engine')"
            @mouseleave="translateSub = null"
          >
            <button type="button" @click="toggleTranslateSub('engine')">
              <span class="menu-ico" aria-hidden="true"></span>
              <span class="label menu-current">
                <span class="menu-current-prefix">{{ t('ocrEngine') }}：</span>
                <span>{{ currentOcrEngineLabel }}</span>
              </span>
              <span class="key"><Icon name="expand" /></span>
            </button>
            <BarMenu v-show="translateSub === 'engine'" sub class="bar-menu-rich">
              <button
                v-for="item in ocrEngineMenuOptions"
                :key="item.id"
                type="button"
                class="choice choice-rich"
                :class="{ on: currentOcrEngine === item.id }"
                :disabled="item.disabled"
                :title="item.disabled
                  ? modelMenuDisabledTitle({
                      runtimeReady: ortRuntimeReady,
                      modelInstalled: !item.packMissing,
                    })
                  : undefined"
                @click="chooseOcrEngine(item.id as OcrEngineId)"
              >
                <span class="mark"><Icon v-if="currentOcrEngine === item.id" name="checked" /></span>
                <AppSelectOptionRow :option="item" />
              </button>
            </BarMenu>
          </div>
          <div
            class="has-sub"
            @mouseenter="showTranslateSub('source')"
            @mouseleave="translateSub = null"
          >
            <button type="button" @click="toggleTranslateSub('source')">
              <span class="menu-ico" aria-hidden="true"></span>
              <span class="label menu-current">
                <span class="menu-current-prefix">{{ t('ocrSourceLang') }}：</span>
                <span
                  v-if="currentSourceFlag"
                  class="lang-flag"
                  :data-cc="currentSourceFlag"
                  aria-hidden="true"
                />
                <Icon v-else name="language" class="menu-lang-icon" />
                <span>{{ currentSourceLangLabel }}</span>
              </span>
              <span class="key"><Icon name="expand" /></span>
            </button>
            <BarMenu v-show="translateSub === 'source'" sub>
              <button
                v-for="item in sourceLangMenuItems"
                :key="item.id"
                type="button"
                class="choice"
                :class="{ on: currentSourceLang === item.id }"
                @click="chooseSourceLang(item.id)"
              >
                <span class="mark"><Icon v-if="currentSourceLang === item.id" name="checked" /></span>
                <span
                  v-if="item.flag"
                  class="lang-flag"
                  :data-cc="item.flag"
                  aria-hidden="true"
                />
                <Icon v-else name="language" class="menu-lang-icon" />
                <span class="label">{{ item.label }}</span>
              </button>
            </BarMenu>
          </div>
          <div class="sep"></div>
          <div
            class="has-sub"
            @mouseenter="showTranslateSub('target')"
            @mouseleave="translateSub = null"
          >
            <button type="button" @click="toggleTranslateSub('target')">
              <span class="menu-ico" aria-hidden="true"></span>
              <span class="label menu-current">
                <span class="menu-current-prefix">{{ t('translationTargetLang') }}：</span>
                <span
                  v-if="currentTargetFlag"
                  class="lang-flag"
                  :data-cc="currentTargetFlag"
                  aria-hidden="true"
                />
                <span>{{ currentTargetLangLabel }}</span>
              </span>
              <span class="key"><Icon name="expand" /></span>
            </button>
            <BarMenu v-show="translateSub === 'target'" sub>
              <button
                v-for="item in targetLangMenuItems"
                :key="item.id"
                type="button"
                class="choice"
                :class="{ on: currentTargetLang === item.id }"
                @click="chooseTargetLang(item.id)"
              >
                <span class="mark"><Icon v-if="currentTargetLang === item.id" name="checked" /></span>
                <span
                  v-if="item.flag"
                  class="lang-flag"
                  :data-cc="item.flag"
                  aria-hidden="true"
                />
                <span class="label">{{ item.label }}</span>
              </button>
            </BarMenu>
          </div>
          <div
            class="has-sub"
            @mouseenter="showTranslateSub('provider')"
            @mouseleave="translateSub = null"
          >
            <button type="button" @click="toggleTranslateSub('provider')">
              <span class="menu-ico" aria-hidden="true"></span>
              <span class="label menu-current">
                <span class="menu-current-prefix">{{ t('translationProvider') }}：</span>
                <span>{{ currentProviderLabel }}</span>
              </span>
              <span class="key"><Icon name="expand" /></span>
            </button>
            <BarMenu v-show="translateSub === 'provider'" sub class="bar-menu-rich">
              <button
                v-for="item in providerMenuOptions"
                :key="item.id"
                type="button"
                class="choice choice-rich"
                :class="{ on: settings.translation.provider === item.id }"
                @click="chooseProvider(item.id as TranslationProviderId)"
              >
                <span class="mark">
                  <Icon v-if="settings.translation.provider === item.id" name="checked" />
                </span>
                <AppSelectOptionRow :option="item" />
              </button>
            </BarMenu>
          </div>
          <div class="sep"></div>
          <button type="button" @click="run(openTranslationSettings)">
            <Icon name="setting" />
            <span class="label">{{ t('translationSettings') }}</span>
          </button>
        </BarMenu>
      </div>
      <div class="menu-slot" @mouseenter="enter('info')">
        <button
          type="button"
          class="menu-label"
          :class="{ open: open === 'info' }"
          @click="toggle('info')"
        ><Icon name="tools" /><span>{{ t('menuInfo') }}</span></button>
        <BarMenu v-if="open === 'info'">
          <button
            type="button"
            :class="{ on: magnifierOn }"
            :disabled="!comic"
            @click="run(toggleMagnifier)"
          >
            <Icon name="lens" />
            <span class="label">{{ t('magnifier') }}</span>
            <span class="key">{{ shortcutDisplay('magnifier') }}</span>
          </button>
          <div class="sep"></div>
          <button
            type="button"
            :class="{ on: isolateCover && settings.readingMode === 'double' }"
            :disabled="!comic || settings.readingMode !== 'double'"
            :title="settings.readingMode !== 'double' ? t('isolateCoverDoubleOnly') : undefined"
            @click="run(toggleIsolateCover)"
          >
            <Icon name="modeDouble" />
            <span class="label">{{ t('isolateCover') }}</span>
          </button>
          <button
            type="button"
            :class="{ on: splitDoublePages }"
            :disabled="!comic"
            :title="t('splitDoublePagesHint')"
            @click="run(toggleSplitDoublePages)"
          >
            <Icon name="splitDouble" />
            <span class="label">{{ t('splitDoublePages') }}</span>
          </button>
          <div class="sep"></div>
          <button type="button" :disabled="!comic" @click="run(openImageInfo)"><Icon name="info" /><span class="label">{{ t('imageInfo') }}</span></button>
          <button type="button" :disabled="!comic" @click="run(() => { void copyImage(); })"><Icon name="copy" /><span class="label">{{ t('copyImage') }}</span><span class="key">{{ shortcutDisplay('copyImage') }}</span></button>
          <button type="button" :disabled="!comic" @click="run(() => { void saveImage(pageIndex); })"><Icon name="save" /><span class="label">{{ t('saveImageAs') }}</span><span class="key">{{ shortcutDisplay('saveImage') }}</span></button>
        </BarMenu>
      </div>
      <div class="menu-slot" @mouseenter="enter('help')">
        <button
          type="button"
          class="menu-label"
          :class="{ open: open === 'help' }"
          @click="toggle('help')"
        ><Icon name="help" /><span>{{ t('menuHelp') }}</span></button>
        <BarMenu v-if="open === 'help'">
          <button type="button" @click="run(() => { void openLogs(); })"><Icon name="terminal" /><span class="label">{{ t('logs') }}</span><span class="key">{{ logsShortcutLabel }}</span></button>
          <button type="button" @click="run(() => { void toggleDevtools(); })"><Icon name="devtools" /><span class="label">{{ t('developerTools') }}</span><span class="key">{{ devtoolsShortcutLabel }}</span></button>
          <div class="sep"></div>
          <button type="button" @click="run(() => { void checkForUpdates(); })"><Icon name="refresh" /><span class="label">{{ t('checkForUpdates') }}</span></button>
          <button type="button" @click="run(() => { void openGithubRepo(); })"><Icon name="github" /><span class="label">{{ t('github') }}</span></button>
          <button type="button" @click="run(() => { void showAbout(); })"><Icon name="info" /><span class="label">{{ t('about') }}</span></button>
        </BarMenu>
      </div>
    </div>
    <div class="menubar-trailing">
      <button
        type="button"
        class="menu-label theme"
        :aria-label="settings.theme === 'light' ? t('themeLight') : t('themeDark')"
        @click="run(toggleTheme)"
      >
        <Icon :name="settings.theme === 'light' ? 'themeLight' : 'themeDark'" />
        <span>{{ settings.theme === 'light' ? t('themeLight') : t('themeDark') }}</span>
      </button>
    </div>
  </header>
</template>

<style>
.menubar {
  flex: 0 0 32px;
  height: 32px;
  display: flex;
  align-items: stretch;
  justify-content: space-between;
  min-width: 0;
  background: var(--chrome);
  color: var(--chrome-ink);
  position: relative;
  z-index: var(--z-titlebar);
  border-bottom: 1px solid var(--line);
}

.menubar.menu-open {
  z-index: var(--z-menu);
}

.menubar-leading,
.menubar-trailing {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: stretch;
  flex: 0 0 auto;
}

.menu-slot {
  position: relative;
  display: flex;
}

.menubar button {
  border: none;
  background: transparent;
  border-radius: 0;
  color: inherit;
}

.menubar button:hover:not(:disabled),
.menubar button.open {
  color: var(--chrome-active);
  background: var(--chrome-hover);
}

.menubar .menu-label {
  height: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  line-height: 1;
}

.menubar button.menu-label.on,
.menubar button.menu-label.on:hover,
.menubar button.menu-label.on.open {
  color: var(--accent);
}

.menubar .menu-label .asset-icon,
.menubar .menu-label .asset-icon svg {
  width: 14px;
  height: 14px;
}

.bar-menu.color-adjust-menu {
  width: min(420px, calc(100vw - 16px));
  min-width: 320px;
  padding: 10px 12px;
}

.bar-menu.color-adjust-menu button {
  width: auto;
  justify-content: center;
  white-space: nowrap;
}

.color-adjust-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: auto;
  font-size: 12px;
}

.color-adjust-table th,
.color-adjust-table td {
  padding: 2px 0;
  vertical-align: middle;
}

.color-adjust-table th {
  width: 1%;
  padding-right: 10px;
  white-space: nowrap;
  text-align: right;
  font-weight: 500;
  color: var(--muted);
}

.color-adjust-table td.range {
  width: 100%;
  padding-right: 10px;
}

.color-adjust-table td.range .range-slider {
  display: block;
  width: 100%;
  min-width: 120px;
}

.color-adjust-table td.value {
  width: 1%;
  padding-right: 10px;
  white-space: nowrap;
}

.bar-menu.color-adjust-menu .color-adjust-table td.value .color-value,
.bar-menu.color-adjust-menu .color-adjust-table td.value .color-value-editing {
  width: auto;
  min-width: 0;
  max-width: none;
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
  padding: 0;
  background: transparent;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 22px;
  color: inherit;
}

.bar-menu.color-adjust-menu .color-adjust-table td.value .color-value:hover,
.bar-menu.color-adjust-menu
  .color-adjust-table
  td.value
  .color-value:hover
  .color-value-number {
  color: var(--chrome-active);
  background: transparent;
  font-weight: inherit;
}

.color-adjust-table td.value .color-value-number,
.color-adjust-table td.value input[type="number"] {
  box-sizing: border-box;
  display: inline-block;
  width: calc(3ch + 8px);
  height: 22px;
  margin: 0;
  padding: 0 4px;
  font: inherit;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 22px;
  text-align: right;
}

.color-adjust-table td.value input[type="number"] {
  border: none;
  border-radius: 3px;
  background: color-mix(in srgb, var(--chrome-active) 12%, transparent);
  color: var(--chrome-active);
  appearance: textfield;
}

.color-adjust-table td.value input[type="number"]::-webkit-inner-spin-button,
.color-adjust-table td.value input[type="number"]::-webkit-outer-spin-button {
  appearance: none;
  display: none;
  margin: 0;
}

.color-adjust-table td.value .suffix {
  margin-left: 0;
  color: var(--muted);
}

.color-adjust-table td.reset {
  width: 1%;
  white-space: nowrap;
}

.bar-menu.color-adjust-menu .color-adjust-reset {
  padding: 2px 8px;
  font-size: 12px;
  line-height: 18px;
  border-radius: var(--radius);
  background: color-mix(in srgb, var(--chrome-active) 10%, transparent);
  color: inherit;
}

.bar-menu.color-adjust-menu .color-adjust-reset:hover:not(:disabled) {
  color: var(--chrome-active);
  background: color-mix(in srgb, var(--chrome-active) 18%, transparent);
}

.menu-model-missing {
  font-weight: 400;
  color: var(--muted);
}
</style>
