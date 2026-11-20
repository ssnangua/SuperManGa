<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { type MessageKey } from "../i18n";
import { notifyError, notifyInfo, notifySuccess } from "../lib/appToast";
import { getCacheDir, getDefaultDataDir, migrateCacheDir, setCacheDir } from "../lib/cacheDir";
import { sameFsPath } from "../lib/fsPath";
import { refreshDataManifest } from "../lib/render/assets";
import {
  ASSOC_GROUPS,
  applyFileAssociations,
  downloadNetworkConfig,
  globalBusy,
  globalBusyText,
  inTauri,
  refreshShellIntegration,
  setContextMenuEnabled,
  settings,
  settingsActiveTab,
  settingsOpen,
  type SettingsTabId,
} from "../reader";
import AppCheckbox from "./AppCheckbox.vue";
import AppModal from "./AppModal.vue";
import HexColorField from "./HexColorField.vue";
import Icon from "./Icon.vue";
import NeuralSettingsPanel from "./NeuralSettingsPanel.vue";
import ProxySettingsPanel from "./ProxySettingsPanel.vue";
import RadioGroup from "./RadioGroup.vue";
import SwitchToggle from "./SwitchToggle.vue";
import TranslationSettingsPanel from "./TranslationSettingsPanel.vue";
const { t } = useI18n();

const shellSupported = ref(false);
const assocBusy = ref(false);
const assocSelected = ref<string[]>([]);
const cacheDirDraft = ref("");
const tabScroller = ref<HTMLElement | null>(null);
const manifestBusy = ref(false);

function resetTabScroll() {
  void nextTick(() => {
    if (tabScroller.value) tabScroller.value.scrollTop = 0;
  });
}

const settingsTabs = computed(() => {
  const tabs: { id: SettingsTabId; label: string }[] = [
    { id: "general", label: t("settingsTabGeneral") },
    { id: "reading", label: t("settingsTabReading") },
    { id: "neural", label: t("settingsTabNeural") },
    { id: "translate", label: t("settingsTabTranslate") },
    { id: "proxy", label: t("settingsTabProxy") },
  ];
  if (shellSupported.value) {
    tabs.push({ id: "system", label: t("settingsTabSystem") });
  }
  return tabs;
});

const contextMenuOn = computed({
  get: () => settings.contextMenu,
  set: (value: boolean) => {
    void toggleContextMenu(value);
  },
});

const scrollTrackOptions = computed(() => [
  { id: "off", label: t("scrollTrackOff") },
  { id: "page", label: t("scrollTrackPage") },
  { id: "jump", label: t("scrollTrackJump") },
]);

const playScrollModeOptions = computed(() => [
  { id: "smooth", label: t("playScrollSmooth") },
  { id: "interval", label: t("playScrollInterval") },
]);

const playIntervalStepModeOptions = computed(() => [
  { id: "screen", label: t("playStepScreen") },
  { id: "custom", label: t("playStepCustom") },
]);

const playIntervalModel = computed({
  get: () =>
    settings.playScrollMode === "interval" ? settings.playIntervalMs : settings.playSmoothInterval,
  set: (value: number) => {
    if (settings.playScrollMode === "interval") settings.playIntervalMs = value;
    else settings.playSmoothInterval = value;
  },
});

const spreadAlignOptions = computed(() => [
  { id: "top", label: t("spreadAlignTop") },
  { id: "center", label: t("spreadAlignCenter") },
  { id: "bottom", label: t("spreadAlignBottom") },
]);

async function refreshCacheDir() {
  if (!inTauri()) return;
  try {
    const resolved = await getCacheDir();
    settings.cacheDir = resolved;
    cacheDirDraft.value = resolved;
  } catch {
    cacheDirDraft.value = settings.cacheDir || "";
  }
}

async function refreshManifestCatalog() {
  if (manifestBusy.value) return;
  if (!inTauri()) {
    notifyError(t("desktopOnly"));
    return;
  }
  manifestBusy.value = true;
  try {
    const result = await refreshDataManifest(downloadNetworkConfig());
    if (result?.updated) notifySuccess(t("refreshDataManifestDone"));
    else notifyInfo(t("refreshDataManifestUnchanged"));
  } catch (error) {
    notifyError(String(error));
  } finally {
    manifestBusy.value = false;
  }
}

async function pickCacheDir() {
  if (!inTauri()) {
    notifyError(t("desktopOnly"));
    return;
  }
  const selected = await open({ directory: true, multiple: false });
  if (typeof selected === "string") {
    cacheDirDraft.value = selected;
    await applyCacheDir(selected);
  }
}

async function applyCacheDir(next: string) {
  const trimmed = next.trim();
  const from = settings.cacheDir || (await getCacheDir());
  const defaultDir = await getDefaultDataDir();
  // Empty input restores the launch default (exe-adjacent / OS fallback).
  const target = trimmed || defaultDir;
  if (sameFsPath(target, from)) {
    cacheDirDraft.value = from;
    return;
  }
  globalBusy.value = true;
  globalBusyText.value = t("assetBusy");
  try {
    const resolved = await migrateCacheDir(from, target);
    // Pin only when different from default; equal path clears the locator.
    const finalPath = await setCacheDir(sameFsPath(resolved, defaultDir) ? "" : resolved);
    settings.cacheDir = finalPath;
    cacheDirDraft.value = finalPath;
    notifySuccess(t("cacheMoved"));
  } catch (error) {
    notifyError(String(error));
    cacheDirDraft.value = settings.cacheDir || from;
  } finally {
    globalBusy.value = false;
    globalBusyText.value = "";
  }
}

async function onCacheBlur() {
  await applyCacheDir(cacheDirDraft.value);
}

async function resetCacheDir() {
  const defaultDir = await getDefaultDataDir();
  cacheDirDraft.value = defaultDir;
  await applyCacheDir(defaultDir);
}

watch(settingsOpen, async (open) => {
  if (!open) return;
  resetTabScroll();
  if (!inTauri()) {
    shellSupported.value = false;
    if (settingsActiveTab.value === "system") settingsActiveTab.value = "general";
    return;
  }
  void refreshCacheDir();
  try {
    const shell = await refreshShellIntegration();
    shellSupported.value = shell.supported;
    settings.contextMenu = shell.contextMenu;
    settings.fileAssociations = shell.associations;
    assocSelected.value = [...shell.associations];
  } catch {
    shellSupported.value = false;
  }
  if (settingsActiveTab.value === "system" && !shellSupported.value) {
    settingsActiveTab.value = "general";
  }
});

watch(settingsActiveTab, () => {
  resetTabScroll();
});

function groupChecked(exts: readonly string[]) {
  return exts.every((ext) => assocSelected.value.includes(ext));
}

function groupSome(exts: readonly string[]) {
  const n = exts.filter((ext) => assocSelected.value.includes(ext)).length;
  return n > 0 && n < exts.length;
}

function toggleExt(ext: string, on: boolean) {
  if (on) {
    if (!assocSelected.value.includes(ext)) assocSelected.value = [...assocSelected.value, ext];
    return;
  }
  assocSelected.value = assocSelected.value.filter((item) => item !== ext);
}

function toggleGroup(exts: readonly string[]) {
  if (groupChecked(exts)) {
    assocSelected.value = assocSelected.value.filter((item) => !exts.includes(item));
    return;
  }
  assocSelected.value = [...new Set([...assocSelected.value, ...exts])];
}

function suggestAssoc() {
  assocSelected.value = [...ASSOC_GROUPS[0]!.exts];
}

function selectAllAssoc() {
  assocSelected.value = ASSOC_GROUPS.flatMap((group) => [...group.exts]);
}

function deselectAllAssoc() {
  assocSelected.value = [];
}

async function toggleContextMenu(enabled: boolean) {
  try {
    await setContextMenuEnabled(enabled);
  } catch (e) {
    notifyError(e instanceof Error ? e.message : String(e));
  }
}

async function applyAssociations() {
  if (!inTauri() || assocBusy.value) return;
  assocBusy.value = true;
  try {
    await applyFileAssociations(assocSelected.value);
    notifySuccess(t("fileAssocApplied"));
  } catch (e) {
    notifyError(e instanceof Error ? e.message : String(e));
  } finally {
    assocBusy.value = false;
  }
}
</script>

<template>
  <AppModal
    v-model="settingsOpen"
    :title="t('settingsTitle')"
    max-width="640px"
    panel-class="settings-dialog settings-dialog-main"
    :body-scroll="false"
    :mask-closable="false"
  >
    <div class="settings-layout">
      <div class="app-tabs" role="tablist">
        <button
          v-for="tab in settingsTabs"
          :key="tab.id"
          class="app-tab"
          type="button"
          role="tab"
          :class="{ on: settingsActiveTab === tab.id }"
          :aria-selected="settingsActiveTab === tab.id"
          @click="settingsActiveTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>

      <div ref="tabScroller" class="app-tab-scroller">
        <div v-if="settingsActiveTab === 'general'" class="app-tab-pane" role="tabpanel">
          <div class="settings-body">
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("autoLoadHistory") }}</span>
                <SwitchToggle v-model="settings.autoLoadHistory" />
              </div>
              <p class="settings-hint">{{ t("autoLoadHistoryHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("syncCurrentFile") }}</span>
                <SwitchToggle v-model="settings.syncCurrentFile" />
              </div>
              <p class="settings-hint">{{ t("syncCurrentFileHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("historyCount") }}</span>
                <input
                  v-model.number="settings.historyCount"
                  type="number"
                  min="0"
                  max="99"
                />
              </div>
              <p class="settings-hint">{{ t("historyCountHint") }}</p>
            </div>
          </div>
          <div class="settings-body">
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("readSubfolder") }}</span>
                <SwitchToggle v-model="settings.readSubfolder" />
              </div>
              <p class="settings-hint">{{ t("readSubfolderHint") }}</p>
            </div>
          </div>
          <div class="settings-body">
            <div class="form-row form-row-label-wide">
              <span>{{ t("cacheDir") }}</span>
              <div class="input-button-group">
                <input
                  v-model="cacheDirDraft"
                  type="text"
                  @keydown.enter.prevent="onCacheBlur"
                  @blur="onCacheBlur"
                />
                <button
                  type="button"
                  class="input-group-btn"
                  :title="t('chooseDirectory')"
                  @click="pickCacheDir"
                >
                  <Icon name="folderOpen" />
                </button>
                <button type="button" class="input-group-reset" @click="resetCacheDir">
                  {{ t("cacheDirReset") }}
                </button>
              </div>
            </div>
          </div>
          <div class="settings-body">
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("refreshDataManifest") }}</span>
                <button
                  type="button"
                  :disabled="manifestBusy"
                  @click="refreshManifestCatalog"
                >
                  <span v-if="manifestBusy" class="busy-label">
                    <Icon name="loadingDotsRotate" />
                    {{ t("refreshDataManifestAction") }}
                  </span>
                  <template v-else>{{ t("refreshDataManifestAction") }}</template>
                </button>
              </div>
              <p class="settings-hint">{{ t("refreshDataManifestHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("checkUpdateOnStartup") }}</span>
                <SwitchToggle v-model="settings.checkUpdateOnStartup" />
              </div>
            </div>
          </div>
        </div>

        <div v-else-if="settingsActiveTab === 'reading'" class="app-tab-pane" role="tabpanel">
          <div class="settings-body">
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("backgroundColor") }}</span>
                <HexColorField v-model="settings.backgroundColor" />
              </div>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("stagePadX") }}</span>
                <input
                  v-model.number="settings.stagePadX"
                  type="number"
                  min="0"
                  max="400"
                />
              </div>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("stagePadY") }}</span>
                <input
                  v-model.number="settings.stagePadY"
                  type="number"
                  min="0"
                  max="400"
                />
              </div>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("spreadGap") }}</span>
                <input
                  v-model.number="settings.spreadGap"
                  type="number"
                  min="0"
                  max="200"
                />
              </div>
              <p class="settings-hint">{{ t("spreadGapHint") }}</p>
            </div>
          </div>

          <div class="settings-body">
            <h3 class="settings-section-title">{{ t("settingsBlockScroll") }}</h3>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("smoothScroll") }}</span>
                <SwitchToggle v-model="settings.smoothScroll" />
              </div>
              <p class="settings-hint">{{ t("smoothScrollHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("scrollTrack") }}</span>
                <RadioGroup
                  v-model="settings.scrollTrack"
                  :options="scrollTrackOptions"
                  :aria-label="t('scrollTrack')"
                />
              </div>
              <p class="settings-hint">{{ t("scrollTrackHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("scrollDelta") }}</span>
                <input
                  v-model.number="settings.scrollDelta"
                  type="number"
                  min="1"
                  max="400"
                />
              </div>
              <p class="settings-hint">{{ t("scrollDeltaHint") }}</p>
            </div>
          </div>

          <div class="settings-body">
            <h3 class="settings-section-title">
              <Icon name="play" />
              {{ t("settingsBlockPlay") }}
            </h3>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("playScrollMode") }}</span>
                <RadioGroup
                  v-model="settings.playScrollMode"
                  :options="playScrollModeOptions"
                  :aria-label="t('playScrollMode')"
                />
              </div>
              <p class="settings-hint">{{ t("playScrollModeHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("playStep") }}</span>
                <input
                  v-if="settings.playScrollMode === 'smooth'"
                  v-model.number="settings.playSmoothStep"
                  type="number"
                  min="1"
                  max="400"
                />
                <div v-else class="play-step-controls">
                  <span class="play-step-slot">
                    <input
                      v-if="settings.playIntervalStepMode === 'custom'"
                      v-model.number="settings.playIntervalStep"
                      type="number"
                      min="1"
                      max="2000"
                    />
                  </span>
                  <RadioGroup
                    v-model="settings.playIntervalStepMode"
                    :options="playIntervalStepModeOptions"
                    :aria-label="t('playStep')"
                  />
                </div>
              </div>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("playInterval") }}</span>
                <input
                  v-model.number="playIntervalModel"
                  type="number"
                  min="1"
                  :max="settings.playScrollMode === 'interval' ? 10000 : 1000"
                />
              </div>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("playAfterScrollPage") }}</span>
                <input
                  v-model.number="settings.playAfterScrollPageMs"
                  type="number"
                  min="0"
                  max="60000"
                />
              </div>
              <p class="settings-hint">{{ t("playAfterScrollPageHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("playNoScrollPage") }}</span>
                <input
                  v-model.number="settings.playNoScrollPageMs"
                  type="number"
                  min="1"
                  max="60000"
                />
              </div>
              <p class="settings-hint">{{ t("playNoScrollPageHint") }}</p>
            </div>
          </div>

          <div class="settings-body">
            <h3 class="settings-section-title">
              <Icon name="modeDouble" />
              {{ t("settingsBlockSpread") }}
            </h3>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("spreadAlign") }}</span>
                <RadioGroup
                  v-model="settings.spreadAlign"
                  :options="spreadAlignOptions"
                  :aria-label="t('spreadAlign')"
                />
              </div>
              <p class="settings-hint">{{ t("spreadAlignHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("isolateCover") }}</span>
                <SwitchToggle v-model="settings.isolateCover" />
              </div>
            </div>
          </div>

          <div class="settings-body">
            <h3 class="settings-section-title">
              <Icon name="minimalist" />
              {{ t("settingsBlockMinimalist") }}
            </h3>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("showSeek") }}</span>
                <SwitchToggle v-model="settings.minimalistShowSeek" />
              </div>
              <p class="settings-hint">{{ t("showSeekHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("showPageNum") }}</span>
                <SwitchToggle v-model="settings.minimalistShowPage" />
              </div>
              <p class="settings-hint">{{ t("showPageHint") }}</p>
            </div>
          </div>

          <div class="settings-body">
            <h3 class="settings-section-title">
              <Icon name="full" />
              {{ t("settingsBlockFullscreen") }}
            </h3>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("showSeek") }}</span>
                <SwitchToggle v-model="settings.fullscreenShowSeek" />
              </div>
              <p class="settings-hint">{{ t("showSeekHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("showPageNum") }}</span>
                <SwitchToggle v-model="settings.fullscreenShowPage" />
              </div>
              <p class="settings-hint">{{ t("showPageHint") }}</p>
            </div>
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("showClock") }}</span>
                <SwitchToggle v-model="settings.fullscreenShowTime" />
              </div>
              <p class="settings-hint">{{ t("showClockHint") }}</p>
            </div>
          </div>
        </div>

        <NeuralSettingsPanel
          v-else-if="settingsActiveTab === 'neural'"
          :active="settingsOpen && settingsActiveTab === 'neural'"
        />
        <TranslationSettingsPanel v-else-if="settingsActiveTab === 'translate'" />
        <ProxySettingsPanel
          v-else-if="settingsActiveTab === 'proxy'"
          :active="settingsOpen && settingsActiveTab === 'proxy'"
        />

        <div
          v-else-if="settingsActiveTab === 'system' && shellSupported"
          class="app-tab-pane"
          role="tabpanel"
        >
          <div class="settings-body">
            <div class="settings-row">
              <div class="settings-row-main">
                <span class="settings-label">{{ t("contextMenuAdd") }}</span>
                <SwitchToggle v-model="contextMenuOn" />
              </div>
              <p class="settings-hint">{{ t("contextMenuHint") }}</p>
            </div>
          </div>
          <div class="settings-body">
            <h3 class="settings-section-title">{{ t("fileAssociations") }}</h3>
            <p class="settings-hint">{{ t("fileAssocHint") }}</p>
            <div class="assoc-groups">
              <div v-for="group in ASSOC_GROUPS" :key="group.id" class="assoc-group">
                <AppCheckbox
                  :model-value="groupChecked(group.exts)"
                  :indeterminate="groupSome(group.exts)"
                  :label="t(group.labelKey as MessageKey)"
                  @update:model-value="toggleGroup(group.exts)"
                />
                <div class="assoc-group-exts">
                  <AppCheckbox
                    v-for="ext in group.exts"
                    :key="ext"
                    :model-value="assocSelected.includes(ext)"
                    :label="`.${ext}`"
                    @update:model-value="toggleExt(ext, $event)"
                  />
                </div>
              </div>
            </div>
            <div class="assoc-actions">
              <button type="button" @click="suggestAssoc">{{ t("assocSuggest") }}</button>
              <button type="button" @click="selectAllAssoc">{{ t("selectAll") }}</button>
              <button type="button" @click="deselectAllAssoc">{{ t("deselectAll") }}</button>
              <button
                class="primary"
                type="button"
                :disabled="assocBusy"
                @click="applyAssociations"
              >
                {{ t("apply") }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </AppModal>
</template>

<style>
.app-modal-panel.settings-dialog,
.app-modal-panel.shortcuts-dialog,
.app-modal-panel.assets-dialog,
.app-modal-panel.shortcut-record-dialog {
  max-height: calc(100% - 32px);
}

.app-modal-backdrop .app-modal-panel.settings-dialog {
  background: var(--surface);
}

.app-modal-backdrop .app-modal-panel.settings-dialog-main {
  height: min(640px, calc(100vh - 48px));
  max-height: min(640px, calc(100vh - 48px));
}

.app-modal-panel.settings-dialog .app-modal-body {
  gap: 0;
}

.settings-layout {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
}

.app-tabs {
  display: flex;
  flex: none;
  align-items: stretch;
  gap: 4px;
  min-width: 0;
  border-bottom: 1px solid var(--line);
}

button.app-tab {
  box-sizing: border-box;
  border: none;
  border-bottom: 2px solid transparent;
  border-radius: 0;
  background: transparent;
  color: var(--chrome-ink);
  font-size: 14px;
  font-weight: 400;
  padding: 8px 12px;
  line-height: 1.2;
  white-space: nowrap;
}

button.app-tab:hover:not(:disabled) {
  color: var(--chrome-active);
  background: transparent;
  border-color: transparent;
  border-bottom-color: transparent;
}

button.app-tab.on:not(:disabled) {
  color: var(--accent);
  font-weight: 600;
  background: transparent;
  border-color: transparent;
  border-bottom-color: var(--accent);
}

.app-tab-scroller {
  flex: 1 1 auto;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  padding: 16px 12px 0 0;
  display: flex;
  flex-direction: column;
}

.app-tab-pane {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.settings-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  background: var(--bg);
  border-radius: 8px;
}

.settings-section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
}

.settings-section-title .asset-icon,
.settings-section-title .asset-icon svg {
  width: 16px;
  height: 16px;
}

.settings-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.settings-row-main {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: nowrap;
  gap: 16px;
  min-width: 0;
}

.settings-label {
  font-size: 14px;
  color: var(--ink);
  flex: 1 1 0;
  min-width: 0;
}

.settings-row-main > :last-child {
  flex: 0 0 auto;
  margin-left: auto;
}

.app-modal-panel.settings-dialog .settings-row-main > button {
  width: auto;
  white-space: nowrap;
}

.app-modal-panel.settings-dialog .settings-row-main .busy-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.app-modal-panel.settings-dialog
  .settings-row-main
  > :is(input[type="text"], input[type="password"], select, .app-custom-select) {
  width: 140px;
}

.app-modal-panel.settings-dialog .settings-row-main > input[type="number"] {
  width: 80px;
}

/* Proxy tab: remaining-width fields. */
.app-modal-panel.settings-dialog .proxy-settings-panel .proxy-text-input {
  flex: 1 1 65%;
  width: auto;
  min-width: 0;
  max-width: 100%;
}

.app-modal-panel.settings-dialog .proxy-settings-panel .proxy-stretch-input {
  flex: 1 1 auto;
  width: auto;
  min-width: 0;
  max-width: 100%;
}

.app-modal-panel.settings-dialog .proxy-settings-panel .proxy-text-input--port {
  width: 120px;
  max-width: 100%;
  flex: 0 0 120px;
}

.app-modal-panel.settings-dialog .proxy-settings-panel .proxy-field {
  flex: 1 1 65%;
  min-width: 0;
  max-width: 100%;
}

.app-modal-panel.settings-dialog .proxy-settings-panel .proxy-select {
  width: 100%;
}

.app-modal-panel .form-row .app-custom-select,
.app-modal-panel .form-row .settings-controls-row {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
  justify-self: stretch;
}

.settings-controls-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.settings-controls-row .app-custom-select,
.settings-controls-row .suggest-input {
  flex: 1 1 auto;
  min-width: 0;
}

.settings-controls-row > button {
  flex: 0 0 auto;
  white-space: nowrap;
}

.app-modal-panel.settings-dialog .proxy-settings-panel .proxy-test-suggest {
  flex: 1 1 auto;
  min-width: 0;
  width: auto;
  max-width: none;
}

.play-step-controls {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  flex: none;
}

.play-step-slot {
  display: flex;
  width: 80px;
  flex: none;
}

.play-step-slot > input[type="number"] {
  width: 80px;
}

.settings-hint {
  margin: 0;
  font-size: 12px;
  line-height: 1.45;
  color: var(--muted);
}

.settings-error {
  margin: 0;
  color: var(--danger);
  font-size: 12px;
}

.assoc-groups {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.assoc-group-exts {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
  padding: 8px 0 0 22px;
}

.assoc-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}

.app-modal-panel.settings-dialog .assoc-actions button {
  font-size: 14px;
  padding: 6px 14px;
  background: var(--field);
}

.app-modal-panel.settings-dialog button.primary:not(:disabled) {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}

.app-modal-panel.settings-dialog button.primary:hover:not(:disabled) {
  background: var(--accent-hover);
  color: #fff;
  border-color: var(--accent-hover);
}

.neural-settings,
.settings-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.neural-models {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* Beat `.app-modal-panel label { display:flex; space-between }` + fixed 180px inputs */
.app-modal-panel .form-row {
  display: grid;
  grid-template-columns: 7.5rem minmax(0, 1fr);
  gap: 12px;
  align-items: center;
  justify-content: stretch;
  font-size: 14px;
}

.app-modal-panel .form-row.form-row-label-wide {
  grid-template-columns: 11rem minmax(0, 1fr);
}

.app-modal-panel .form-row > span:first-child {
  color: var(--ink);
  line-height: 1.35;
  white-space: nowrap;
  text-align: left;
}

.app-modal-panel
  .form-row:not(.check)
  :is(
    input[type="text"],
    input[type="number"],
    input[type="password"],
    input[type="url"],
    select
  ) {
  width: 100%;
  max-width: none;
  min-width: 0;
  box-sizing: border-box;
}

.app-modal-panel .form-row .input-button-group,
.app-modal-panel .form-row .proxy-test-row,
.app-modal-panel .form-row .password-row,
.app-modal-panel .form-row .neural-hf-mirror-input,
.app-modal-panel .form-row .suggest-input {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
  justify-self: stretch;
}

.app-modal-panel .form-row > button,
.app-modal-panel .form-row > .action-with-size,
.app-modal-panel .form-row > .switch-toggle {
  width: auto;
  min-width: 0;
  justify-self: end;
  box-sizing: border-box;
}

.action-with-size {
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  min-width: 0;
}

.action-with-size .size-hint {
  flex: none;
  font-size: 12px;
  white-space: nowrap;
}

.asset-action-btn-wrap {
  display: inline-flex;
  max-width: 100%;
}

.asset-actions .action-with-size {
  width: 100%;
}

.app-modal-panel .form-row input[type="checkbox"] {
  justify-self: start;
  width: auto;
}

.app-modal-panel .form-row .port-input {
  max-width: 7rem;
  justify-self: start;
  width: 100%;
}

.app-modal-panel .form-row input.neural-target-size-input[type="number"] {
  width: 80px;
  max-width: 100%;
  justify-self: end;
}

.app-modal-panel .form-row .neural-hf-mirror-input .suggest-input-field {
  width: 100%;
}

.proxy-test-row {
  display: flex;
  gap: 8px;
  align-items: stretch;
  min-width: 0;
}

.proxy-test-row input {
  flex: 1 1 auto;
  min-width: 0;
}

.proxy-test-row button {
  flex: 0 0 auto;
  white-space: nowrap;
}

.password-row {
  display: flex;
  align-items: stretch;
  gap: 8px;
  min-width: 0;
  width: 100%;
}

.password-row-input {
  flex: 1 1 auto;
  min-width: 0;
}

.password-row .icon-only,
.proxy-test-row .connection-test-btn {
  flex: 0 0 auto;
}

.icon-only {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 6px;
  min-width: 36px;
}

.icon-only .asset-icon,
.icon-only .asset-icon svg {
  width: 16px;
  height: 16px;
}

.connection-test-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.connection-test-btn .asset-icon,
.connection-test-btn .asset-icon svg {
  width: 16px;
  height: 16px;
}

.connection-test-btn .asset-icon.spinning,
.connection-test-btn .spinning.asset-icon {
  animation: connection-test-spin 0.65s linear infinite;
}

@keyframes connection-test-spin {
  to {
    transform: rotate(360deg);
  }
}

.form-row-test {
  align-items: start;
}

.form-row-test > span:first-child {
  padding-top: 8px;
}

.app-modal-panel .runtime-form-row > button {
  justify-self: end;
  width: auto;
  white-space: nowrap;
}

.settings-sep {
  border: none;
  border-top: 1px solid var(--line);
  margin: 4px 0;
}

.settings-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.settings-block-title {
  font-size: 13px;
  color: var(--ink);
}

.settings-hint {
  font-size: 12px;
  color: var(--muted);
  line-height: 1.4;
}

.settings-hint-warning {
  color: var(--warning);
}

/* Sit hint under its control/title (8px), same as .settings-row gap; body gap keeps 16px to next. */
.settings-body > .form-row + .settings-hint,
.settings-body > .settings-section-title + .settings-hint {
  margin-top: -8px;
  margin-bottom: 0;
}

.assoc-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px 10px;
}

.assoc-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
