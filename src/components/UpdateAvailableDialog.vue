<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed } from "vue";

import {
  dismissUpdateUi,
  openUpdateDownloadPage,
  updateUi,
} from "../lib/updateCheck";
import AppModal from "./AppModal.vue";
import LoadingDotsBounce from "./LoadingDotsBounce.vue";

const { t } = useI18n();

const checkingOpen = computed({
  get: () => updateUi.phase === "checking",
  set: (value) => {
    if (!value && updateUi.phase === "checking") dismissUpdateUi();
  },
});

const availableOpen = computed({
  get: () => updateUi.phase === "available",
  set: (value) => {
    if (!value && updateUi.phase === "available") dismissUpdateUi();
  },
});

const infoOpen = computed({
  get: () => updateUi.phase === "info",
  set: (value) => {
    if (!value && updateUi.phase === "info") dismissUpdateUi();
  },
});

async function goDownload() {
  const url = updateUi.htmlUrl;
  dismissUpdateUi();
  await openUpdateDownloadPage(url);
}
</script>

<template>
  <!-- Manual check: show immediately; cannot dismiss mid-request. -->
  <AppModal
    v-model="checkingOpen"
    :title="t('checkForUpdates')"
    :mask-closable="false"
    :close-on-esc="false"
    :show-close-button="false"
    :body-scroll="false"
    max-width="400px"
    panel-class="update-checking-dialog"
  >
    <p class="update-checking-text">
      <span>{{ t("updateChecking") }}</span>
      <LoadingDotsBounce />
    </p>
  </AppModal>

  <AppModal
    v-model="availableOpen"
    :title="t('updateAvailableTitle')"
    :mask-closable="true"
    :body-scroll="false"
    max-width="420px"
    panel-class="update-available-dialog"
  >
    <p class="update-available-text">
      {{ t("updateAvailablePrefix") }}{{ updateUi.latestVersion
      }}{{ t("updateAvailableSuffix") }}
    </p>
    <template #footer>
      <div class="app-modal-actions end">
        <button type="button" class="ghost" @click="dismissUpdateUi">{{ t("later") }}</button>
        <button type="button" class="primary" @click="goDownload">{{ t("goToDownload") }}</button>
      </div>
    </template>
  </AppModal>

  <AppModal
    v-model="infoOpen"
    :title="updateUi.infoTitle"
    :mask-closable="true"
    :body-scroll="false"
    max-width="400px"
    panel-class="update-info-dialog"
  >
    <p class="update-info-message">{{ updateUi.infoMessage }}</p>
    <details v-if="updateUi.infoDetail" class="update-info-detail">
      <summary>{{ t("errorDetails") }}</summary>
      <pre>{{ updateUi.infoDetail }}</pre>
    </details>
    <template #footer>
      <div class="app-modal-actions end">
        <button type="button" class="primary" @click="dismissUpdateUi">{{ t("ok") }}</button>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.update-checking-text {
  margin: 0;
  display: inline-flex;
  align-items: baseline;
  gap: 1px;
  font-size: 14px;
  line-height: 1.5;
}

.update-available-text,
.update-info-message {
  margin: 0;
  font-size: 13px;
}

.update-info-detail {
  font-size: 12px;
  color: var(--muted);
}

.update-info-detail summary {
  cursor: pointer;
  user-select: none;
  color: var(--muted);
}

.update-info-detail summary:hover {
  color: var(--ink);
}

.update-info-detail pre {
  margin: 8px 0 0;
  padding: 8px 10px;
  border-radius: var(--radius);
  background: var(--field);
  border: 1px solid var(--line);
  color: var(--ink);
  font-size: 12px;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 160px;
  overflow: auto;
  user-select: text;
}
</style>

<!-- Unscoped: panel-class lives on Teleported AppModal, outside this SFC. -->
<style>
.app-modal-panel.update-available-dialog button.primary:not(:disabled),
.app-modal-panel.update-info-dialog button.primary:not(:disabled) {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}

.app-modal-panel.update-available-dialog button.primary:hover:not(:disabled),
.app-modal-panel.update-info-dialog button.primary:hover:not(:disabled) {
  background: var(--accent-hover);
  color: #fff;
  border-color: var(--accent-hover);
}
</style>
