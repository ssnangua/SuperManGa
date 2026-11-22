<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  initNeuralRuntime,
  listenNeuralProgress,
  neuralRuntimeStatus,
  type NeuralRuntimeStatus,
} from "../lib/render/assets";
import {
  downloadNetworkConfig,
  refreshTranslationPrereqs,
  runtimeRestartOpen,
} from "../reader";
import { notifyError, notifySuccess } from "../lib/appToast";
import Icon from "./Icon.vue";
const { t } = useI18n();

const props = defineProps<{ active?: boolean }>();

const runtimeBusy = ref(false);
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
const runtimeOutdated = computed(() => runtimeStatus.value.outdated);

const runtimeSizeText = computed(() =>
  formatBytes(runtimeStatus.value.approxBytes || 83_000_000),
);

function readyLabel(status: NeuralRuntimeStatus) {
  if (status.accelerator === "cuda") return t("neuralRuntimeReadyCuda");
  if (status.accelerator === "coreml") return t("neuralRuntimeReadyCoreml");
  return t("neuralRuntimeReady");
}

const runtimeButtonLabel = computed(() => {
  if (runtimeBusy.value) return null;
  if (runtimeOutdated.value) return t("neuralRuntimeUpdate");
  if (runtimeReady.value) return readyLabel(runtimeStatus.value);
  return t("neuralRuntimeInit");
});

const runtimeHint = computed(() => {
  const status = runtimeStatus.value;
  const recAccel = status.recommendedAccelerator;
  const accel = status.accelerator;
  if (!status.ready) {
    if (recAccel === "cuda") return t("neuralRuntimeHintUpgradeGpu");
    if (recAccel === "coreml") return t("neuralRuntimeHintCoremlInit");
    if (status.cudaCapable) return t("neuralRuntimeHintCpuCudaTip");
    return t("neuralRuntimeHintCpu");
  }
  if (recAccel === "cuda" && accel !== "cuda") {
    return t("neuralRuntimeHintUpgradeGpu");
  }
  if (accel === "cuda") return t("neuralRuntimeHintCuda");
  if (accel === "coreml") return t("neuralRuntimeHintCoreml");
  if (status.cudaCapable) return t("neuralRuntimeHintCpuCudaTip");
  return t("neuralRuntimeHintCpu");
});

function formatBytes(value: number) {
  if (value <= 0) return "";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

async function refreshRuntime() {
  runtimeStatus.value = await neuralRuntimeStatus();
}

async function initRuntime() {
  if (runtimeBusy.value || (runtimeReady.value && !runtimeOutdated.value)) return;
  runtimeBusy.value = true;
  progressPercent.value = null;
  const wasOutdated = runtimeOutdated.value;
  try {
    runtimeStatus.value = await initNeuralRuntime(downloadNetworkConfig());
    void refreshTranslationPrereqs();
    if (wasOutdated) {
      runtimeRestartOpen.value = true;
    } else {
      notifySuccess(readyLabel(runtimeStatus.value));
    }
  } catch (error) {
    notifyError(String(error));
  } finally {
    runtimeBusy.value = false;
    progressPercent.value = null;
  }
}

watch(
  () => props.active,
  (active) => {
    if (active) void refreshRuntime();
  },
);

onMounted(async () => {
  unlistenProgress = await listenNeuralProgress((event) => {
    if (event.kind === "runtime") progressPercent.value = event.percent;
  });
  void refreshRuntime();
});

onUnmounted(() => {
  unlistenProgress?.();
});
</script>

<template>
  <div class="asset-row runtime-settings-row">
    <h3 class="settings-section-title">{{ t("neuralRuntime") }}</h3>
    <div class="asset-actions">
      <div class="action-with-size">
        <span class="size-hint muted">{{ runtimeSizeText }}</span>
        <button
          type="button"
          :disabled="runtimeBusy || (runtimeReady && !runtimeOutdated)"
          @click="initRuntime"
        >
          <span v-if="runtimeBusy" class="busy-label">
            <Icon name="loadingDotsRotate" />
            <template v-if="progressPercent != null">{{ progressPercent }}%</template>
            <template v-else>{{ t("neuralRuntimeInit") }}</template>
          </span>
          <template v-else>{{ runtimeButtonLabel }}</template>
        </button>
      </div>
    </div>
  </div>
  <p class="settings-hint neural-target-hint">{{ runtimeHint }}</p>
</template>
