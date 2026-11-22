<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  OCR_ASSET_CATALOG,
  installOcrAsset,
  listenAssetProgress,
  listOcrAssets,
  type OcrAssetInfo,
} from "../lib/ocr/assets";
import { neuralRuntimeStatus, type NeuralRuntimeStatus } from "../lib/render/assets";
import { notifyError, notifySuccess } from "../lib/appToast";
import { hideAppTip, showAppTip } from "../lib/appTip";
import {
  detectorAssetsOpen,
  downloadNetworkConfig,
  refreshTranslationPrereqs,
} from "../reader";
import AppModal from "./AppModal.vue";
import Icon from "./Icon.vue";
const { t } = useI18n();

const rows = ref<OcrAssetInfo[]>([]);
const busyId = ref<string | null>(null);
const progressPercent = ref<number | null>(null);
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
let unlistenProgress: (() => void) | null = null;

const runtimeReady = computed(() => runtimeStatus.value.ready);
const detectorRows = computed(() => rows.value.filter((row) => row.kind === "detector"));

async function refreshRuntime() {
  runtimeStatus.value = await neuralRuntimeStatus();
}

async function refresh() {
  rows.value = await listOcrAssets();
}

function meta(id: string) {
  return OCR_ASSET_CATALOG.find((item) => item.id === id);
}

function descriptionFor(id: string): string {
  const item = meta(id);
  return item ? t(item.descriptionKey) : "";
}

function formatBytes(value: number) {
  if (value <= 0) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(0)} MB`;
}

function installSizeText(row: OcrAssetInfo) {
  const item = meta(row.id);
  const approx = row.approxBytes || item?.approxBytes || 0;
  if (row.outdated || !row.installed || row.bytes <= 0) return formatBytes(approx);
  return formatBytes(row.bytes);
}

function assetButtonLabel(row: OcrAssetInfo) {
  if (row.outdated) return t("assetUpdate");
  if (row.installed) return t("assetInstalled");
  return t("assetInstall");
}

function assetActionDisabled(row: OcrAssetInfo) {
  if (busyId.value === row.id) return true;
  if (row.installed && !row.outdated) return true;
  if (!runtimeReady.value) return true;
  return false;
}

function assetInstallTip(row: OcrAssetInfo) {
  if (row.installed && !row.outdated) return "";
  if (!runtimeReady.value) return t("initRuntimeFirst");
  return "";
}

function showInstallTip(event: PointerEvent, row: OcrAssetInfo) {
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

async function install(id: string) {
  const row = rows.value.find((item) => item.id === id);
  if (busyId.value || !runtimeReady.value) return;
  if (row?.installed && !row.outdated) return;
  busyId.value = id;
  progressPercent.value = null;
  const updating = !!row?.outdated;
  try {
    await installOcrAsset(id, downloadNetworkConfig());
    await refresh();
    void refreshTranslationPrereqs();
    notifySuccess(updating ? t("assetUpdated") : t("assetInstalled"));
  } catch (error) {
    notifyError(String(error));
  } finally {
    busyId.value = null;
    progressPercent.value = null;
  }
}

watch(detectorAssetsOpen, (open) => {
  if (open) {
    void refreshRuntime();
    void refresh();
  } else {
    hideAppTip();
  }
});

onMounted(async () => {
  if (detectorAssetsOpen.value) {
    void refreshRuntime();
    void refresh();
  }
  unlistenProgress = await listenAssetProgress((event) => {
    if (event.id === busyId.value) progressPercent.value = event.percent;
  });
});

onUnmounted(() => {
  hideAppTip();
  unlistenProgress?.();
});
</script>

<template>
  <AppModal
    v-model="detectorAssetsOpen"
    :title="t('textDetectorAssetsTitle')"
    max-width="640px"
    panel-class="settings-dialog assets-dialog"
  >
    <div class="app-tab-pane assets-pane">
      <p class="settings-hint assets-top-hint">{{ t("textDetectorAssetsHint") }}</p>

      <div v-for="row in detectorRows" :key="row.id" class="settings-body asset-block">
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
                  <span v-if="busyId === row.id" class="busy-label">
                    <Icon name="loadingDotsRotate" />
                    <template v-if="progressPercent != null">{{ progressPercent }}%</template>
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
        <p class="settings-hint">{{ descriptionFor(row.id) }}</p>
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
