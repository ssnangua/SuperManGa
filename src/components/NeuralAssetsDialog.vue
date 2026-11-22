<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import {
  assetDownloadPercentFor,
  assetJobKey,
  isAssetDownloading,
} from "../lib/assetDownload";
import {
  IMAGE_ASSET_CATALOG,
  installImageAsset,
  listImageAssets,
  neuralRuntimeStatus,
  type ImageAssetInfo,
  type NeuralRuntimeStatus,
} from "../lib/render/assets";
import { notifyError, notifySuccess } from "../lib/appToast";
import { hideAppTip, showAppTip } from "../lib/appTip";
import { neuralAssetsOpen, downloadNetworkConfig } from "../reader";
import AppModal from "./AppModal.vue";
import Icon from "./Icon.vue";

const { t } = useI18n();
const rows = ref<ImageAssetInfo[]>([]);
const runtimeStatus = ref<NeuralRuntimeStatus>({
  ready: false,
  outdated: false,
  version: null,
  libraryPath: null,
  bytes: 0,
  approxBytes: 83_000_000,
  flavor: "cpu",
  recommendedFlavor: "cpu",
  accelerator: "none",
  recommendedAccelerator: "none",
  cudaCapable: false,
  gpu: false,
});

function modelJob(id: string) {
  return assetJobKey("model", id);
}

function isModelBusy(id: string) {
  return isAssetDownloading(modelJob(id));
}

function modelProgress(id: string) {
  return assetDownloadPercentFor(modelJob(id));
}

const runtimeReady = computed(() => runtimeStatus.value.ready);
const modelRows = computed(() => rows.value.filter((row) => row.kind === "neural"));

function meta(id: string) {
  return IMAGE_ASSET_CATALOG.find((item) => item.id === id);
}

function formatBytes(value: number) {
  if (value <= 0) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function installSizeText(row: ImageAssetInfo) {
  const item = meta(row.id);
  const approx = row.approxBytes || item?.approxBytes || 0;
  if (row.outdated || !row.installed || row.bytes <= 0) return formatBytes(approx);
  return formatBytes(row.bytes);
}

function assetButtonLabel(row: ImageAssetInfo) {
  if (row.outdated) return t("assetUpdate");
  if (row.installed) return t("assetInstalled");
  return t("assetInstall");
}

function assetActionDisabled(row: ImageAssetInfo) {
  if (isModelBusy(row.id)) return true;
  if (row.installed && !row.outdated) return true;
  if (!runtimeReady.value) return true;
  return false;
}

function assetInstallTip(row: ImageAssetInfo) {
  if (row.installed && !row.outdated) return "";
  if (!runtimeReady.value) return t("initRuntimeFirst");
  return "";
}

function showInstallTip(event: PointerEvent, row: ImageAssetInfo) {
  const text = assetInstallTip(row);
  const target = event.currentTarget;
  if (!text || !(target instanceof HTMLElement)) {
    hideAppTip();
    return;
  }
  const rect = target.getBoundingClientRect();
  showAppTip({
    text,
    x: rect.left + rect.width / 2,
    y: rect.top,
    placement: "above",
    gap: 8,
  });
}

async function refreshRuntime() {
  runtimeStatus.value = await neuralRuntimeStatus();
}

async function refresh() {
  rows.value = await listImageAssets();
}

async function install(id: string) {
  const row = rows.value.find((item) => item.id === id);
  if (!runtimeReady.value) return;
  if (row?.installed && !row.outdated) return;
  const alreadyBusy = isModelBusy(id);
  const updating = !!row?.outdated;
  try {
    await installImageAsset(id, downloadNetworkConfig());
    await refresh();
    if (alreadyBusy) return;
    notifySuccess(updating ? t("assetUpdated") : t("assetInstalled"));
  } catch (error) {
    if (!alreadyBusy) notifyError(String(error));
  }
}

watch(neuralAssetsOpen, (open) => {
  if (open) {
    void refreshRuntime();
    void refresh();
  } else {
    hideAppTip();
  }
});

onMounted(() => {
  if (neuralAssetsOpen.value) {
    void refreshRuntime();
    void refresh();
  }
});

onUnmounted(() => {
  hideAppTip();
});
</script>

<template>
  <AppModal
    v-model="neuralAssetsOpen"
    :title="t('neuralAssetsTitle')"
    max-width="640px"
    panel-class="settings-dialog assets-dialog"
  >
    <div class="app-tab-pane assets-pane">
      <p class="settings-hint assets-top-hint">{{ t("neuralAssetsHint") }}</p>

      <div v-for="row in modelRows" :key="row.id" class="settings-body asset-block">
        <div class="asset-row">
          <h3 class="settings-section-title">
            {{ meta(row.id) ? t(meta(row.id)!.nameKey) : row.id }}
          </h3>
          <div class="asset-actions">
            <div class="action-with-size">
              <span class="size-hint muted">{{ installSizeText(row) }}</span>
              <span
                class="asset-action-btn-wrap"
                @pointerenter="showInstallTip($event, row)"
                @pointerleave="hideAppTip"
              >
                <button type="button" :disabled="assetActionDisabled(row)" @click="install(row.id)">
                  <span v-if="isModelBusy(row.id)" class="busy-label">
                    <Icon name="loadingDotsRotate" />
                    <template v-if="modelProgress(row.id) != null">{{ modelProgress(row.id) }}%</template>
                    <template v-else>{{
                      row.outdated ? t("assetUpdating") : t("assetInstalling")
                    }}</template>
                  </span>
                  <span v-else-if="row.outdated" class="btn-with-icon">
                    <Icon name="download" />{{ t("assetUpdate") }}
                  </span>
                  <span v-else-if="!row.installed" class="btn-with-icon">
                    <Icon name="download" />{{ t("assetInstall") }}
                  </span>
                  <span v-else>{{ assetButtonLabel(row) }}</span>
                </button>
              </span>
            </div>
          </div>
        </div>
        <p class="settings-hint">{{ meta(row.id) ? t(meta(row.id)!.descriptionKey) : "" }}</p>
      </div>
    </div>
  </AppModal>
</template>

<style>
.asset-row {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  justify-content: space-between;
  padding: 0;
  border-top: none;
}

.asset-row .settings-section-title {
  flex: 1 1 auto;
  min-width: 0;
}

.runtime-settings-row,
.neural-models .asset-row {
  align-items: center;
}

.neural-models .asset-row:last-child {
  padding-bottom: 0;
}

.asset-copy {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.asset-copy p {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.4;
}

.asset-copy .muted {
  color: var(--muted);
  font-size: 12px;
}

.assets-pane {
  gap: 12px;
  /* Keep cards clear of the scrollbar. */
  padding-right: 10px;
}

.assets-top-hint {
  margin: 0;
  padding: 0 2px;
}

.app-modal-panel.assets-dialog .app-modal-body {
  gap: 0;
  /* Breathing room between scrollport edge / scrollbar and content. */
  padding-right: 4px;
}

.app-modal-panel.assets-dialog .asset-block {
  gap: 8px;
}

.app-modal-panel.assets-dialog .asset-block .asset-row {
  align-items: center;
}

.app-modal-panel.assets-dialog .asset-block .settings-section-title {
  flex: 1 1 auto;
  min-width: 0;
}

.app-modal-panel.assets-dialog .asset-block .settings-hint {
  margin: 0;
}

.app-modal-panel.assets-dialog .asset-block .asset-lang-tags {
  margin-top: 0;
  width: 100%;
}

.asset-actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 0 0 auto;
}

.busy-label {
  display: inline-flex;
  align-items: flex-end;
  justify-content: center;
  gap: 6px;
}

.busy-label .asset-icon,
.busy-label .asset-icon svg {
  width: 22px;
  height: 14px;
}

.btn-with-icon {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.btn-with-icon .asset-icon,
.btn-with-icon .asset-icon svg {
  width: 14px;
  height: 14px;
}
</style>
