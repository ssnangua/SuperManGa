<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed } from "vue";

import { relaunchApp, runtimeRestartOpen } from "../reader";
import AppModal from "./AppModal.vue";

const { t } = useI18n();
const open = computed({
  get: () => runtimeRestartOpen.value,
  set: (value) => {
    if (!value) runtimeRestartOpen.value = false;
  },
});

function later() {
  runtimeRestartOpen.value = false;
}

async function restart() {
  await relaunchApp();
}
</script>

<template>
  <AppModal
    v-model="open"
    :title="t('prompt')"
    :mask-closable="false"
    :close-on-esc="false"
    :show-close-button="false"
    :body-scroll="false"
    max-width="400px"
    panel-class="restart-required-dialog"
  >
    <p>{{ t("neuralRuntimeUpdatedRestart") }}</p>
    <template #footer>
      <div class="app-modal-actions end">
        <button type="button" class="ghost" @click="later">{{ t("later") }}</button>
        <button type="button" class="primary" @click="restart">{{ t("restartApp") }}</button>
      </div>
    </template>
  </AppModal>
</template>

<style>
.app-modal-panel.restart-required-dialog .app-modal-body {
  gap: 0;
}

.app-modal-panel.restart-required-dialog p {
  margin: 0;
  font-size: 14px;
  line-height: 1.5;
}

.app-modal-panel.restart-required-dialog button.primary:not(:disabled) {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}

.app-modal-panel.restart-required-dialog button.primary:hover:not(:disabled) {
  background: var(--accent-hover);
  color: #fff;
  border-color: var(--accent-hover);
}
</style>
