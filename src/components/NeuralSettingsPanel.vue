<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, onMounted, ref, watch } from "vue";
import SwitchToggle from "./SwitchToggle.vue";

import {
  clearNeuralResults,
  listImageAssets,
  neuralResultsStatus,
} from "../lib/render/assets";
import {
  neuralAssetsOpen,
  setRenderProfile,
  settings,
} from "../reader";
import { notifyError, notifySuccess } from "../lib/appToast";
import {
  NEURAL_TARGET_SIZE_DEFAULT,
  NEURAL_TARGET_SIZE_MAX,
  NEURAL_TARGET_SIZE_MIN,
  clampNeuralTargetSize,
  clearNeuralUpscaleCache,
  neuralBatchPaused,
} from "../lib/render/neural";
import { isNeuralProfile, neuralAssetId } from "../lib/render/profile";
import RuntimeSettingsBlock from "./RuntimeSettingsBlock.vue";

const { t } = useI18n();
const props = defineProps<{ active: boolean }>();

const resultsBytes = ref(0);

function formatBytes(value: number) {
  if (value <= 0) return "";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

const hasResultsCache = computed(() => resultsBytes.value > 0);

async function ensureNeuralProfileInstalled() {
  if (!isNeuralProfile(settings.renderProfile)) return;
  const id = neuralAssetId(settings.renderProfile);
  const rows = await listImageAssets();
  if (!rows.some((row) => row.id === id && row.installed)) {
    setRenderProfile("bicubic");
  }
}

async function refreshResults() {
  try {
    const status = await neuralResultsStatus();
    resultsBytes.value = status.bytes;
  } catch {
    resultsBytes.value = 0;
  }
}

async function refreshAll() {
  await ensureNeuralProfileInstalled();
  await refreshResults();
}

function onTargetSizeBlur() {
  settings.neuralTargetSize = clampNeuralTargetSize(settings.neuralTargetSize);
}

async function onClearResults() {
  try {
    await clearNeuralResults();
    clearNeuralUpscaleCache();
    await refreshResults();
    notifySuccess(t("neuralResultsCleared"));
  } catch (error) {
    notifyError(String(error));
  }
}

const bookPreUpscale = computed({
  get: () => !neuralBatchPaused.value,
  set: (on) => {
    neuralBatchPaused.value = !on;
  },
});

watch(
  () => props.active,
  (active) => {
    if (active) void refreshAll();
  },
);

watch(neuralAssetsOpen, (open) => {
  if (!open && props.active) void ensureNeuralProfileInstalled();
});

onMounted(() => {
  if (props.active) void refreshAll();
});
</script>

<template>
  <div class="app-tab-pane neural-settings-panel" role="tabpanel">
    <div class="settings-body neural-settings">
      <RuntimeSettingsBlock :active="props.active" />
    </div>

    <div class="settings-body neural-settings neural-models">
      <div class="asset-row">
        <h3 class="settings-section-title">{{ t("neuralSectionModels") }}</h3>
        <div class="asset-actions">
          <button type="button" @click="neuralAssetsOpen = true">
            {{ t("neuralModelsManage") }}
          </button>
        </div>
      </div>
    </div>

    <div class="settings-body neural-settings">
      <h3 class="settings-section-title">{{ t("neuralSectionBehavior") }}</h3>
      <div class="form-row">
        <span>{{ t("neuralTargetSize") }}</span>
        <input
          v-model.number="settings.neuralTargetSize"
          class="neural-target-size-input"
          type="number"
          :min="NEURAL_TARGET_SIZE_MIN"
          :max="NEURAL_TARGET_SIZE_MAX"
          :step="64"
          :placeholder="String(NEURAL_TARGET_SIZE_DEFAULT)"
          :title="t('neuralTargetSizeHint')"
          @keydown.enter.prevent="onTargetSizeBlur"
          @blur="onTargetSizeBlur"
        />
      </div>
      <p class="settings-hint neural-target-hint">{{ t("neuralTargetSizeHint") }}</p>

      <div class="form-row">
        <span>{{ t("neuralBookPreUpscale") }}</span>
        <SwitchToggle
          v-model="bookPreUpscale"
          class="neural-switch"
          :aria-label="t('neuralBookPreUpscale')"
        />
      </div>
      <p class="settings-hint neural-target-hint">{{ t("neuralBookPreUpscaleHint") }}</p>

      <div class="form-row form-row-label-wide">
        <span>{{ t("neuralResultsSize") }}</span>
        <div class="action-with-size">
          <span v-if="hasResultsCache" class="size-hint muted">{{ formatBytes(resultsBytes) }}</span>
          <button
            type="button"
            class="hover-warning"
            :disabled="!hasResultsCache"
            @click="onClearResults"
          >
            {{ t("neuralClearResults") }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
