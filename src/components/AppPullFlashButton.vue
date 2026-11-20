<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import Icon from "./Icon.vue";

export type AppPullFlashDoneArg = boolean | "abort";
export type AppPullFlashDone = (result: AppPullFlashDoneArg) => void;

const props = withDefaults(
  defineProps<{
    label: string;
    busy?: boolean;
    ariaLabel?: string;
  }>(),
  { busy: false },
);

const emit = defineEmits<{
  pull: [done: AppPullFlashDone];
}>();

type Phase = "idle" | "loading" | "success" | "fail";
const phase = ref<Phase>("idle");
let successTimer: ReturnType<typeof setTimeout> | null = null;

function clearSuccessTimer() {
  if (successTimer != null) {
    clearTimeout(successTimer);
    successTimer = null;
  }
}

const disabled = computed(() => phase.value === "loading" || props.busy);

const iconName = computed(() => {
  if (phase.value === "success") return "success";
  if (phase.value === "fail") return "fail";
  return "refresh";
});

function finish(result: AppPullFlashDoneArg) {
  if (result === "abort") {
    clearSuccessTimer();
    phase.value = "idle";
    return;
  }
  if (result) {
    phase.value = "success";
    clearSuccessTimer();
    successTimer = setTimeout(() => {
      phase.value = "idle";
      successTimer = null;
    }, 1000);
  } else {
    clearSuccessTimer();
    phase.value = "fail";
  }
}

function beginPull(action: (done: AppPullFlashDone) => void) {
  if (disabled.value) return;
  clearSuccessTimer();
  phase.value = "loading";
  action(finish);
}

function onClick() {
  beginPull((done) => emit("pull", done));
}

function clearStaleFailOnSilentSuccess(ok: boolean) {
  if (ok && phase.value === "fail") phase.value = "idle";
}

onBeforeUnmount(() => {
  clearSuccessTimer();
});

defineExpose({
  clearStaleFailOnSilentSuccess,
  /** Drive the same loading / success / fail flash from an external trigger (e.g. API key change). */
  beginPull,
});
</script>

<template>
  <button
    type="button"
    class="app-pull-flash-btn"
    :class="{
      success: phase === 'success',
      danger: phase === 'fail',
    }"
    :disabled="disabled"
    :aria-label="ariaLabel ?? label"
    @click="onClick"
  >
    <Icon
      :name="iconName"
      class="app-pull-flash-icon"
      :class="{ spinning: phase === 'loading' || busy }"
    />
    {{ label }}
  </button>
</template>

<style scoped>
.app-pull-flash-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex: none;
  white-space: nowrap;
}

.app-pull-flash-icon {
  --icon-size: 16px;
  width: 16px;
  height: 16px;
}

.app-pull-flash-icon.spinning {
  animation: app-pull-flash-spin 0.65s linear infinite;
}

@keyframes app-pull-flash-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
