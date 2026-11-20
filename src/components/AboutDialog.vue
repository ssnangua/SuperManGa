<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { about, aboutOpen } from "../reader";
import AppModal from "./AppModal.vue";
const { t } = useI18n();

function close() {
  aboutOpen.value = false;
}
</script>

<template>
  <AppModal v-model="aboutOpen" :title="t('appName')" max-width="440px" panel-class="about-dialog">
    <p class="about-version">{{ t("version") }} {{ about?.version }}</p>
    <p class="about-blurb">{{ t("aboutBlurb") }}</p>
    <template #footer>
      <div class="about-actions">
        <button type="button" @click="close">{{ t("close") }}</button>
      </div>
    </template>
  </AppModal>
</template>

<style>
.app-modal-panel.about-dialog .about-version {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
}

.app-modal-panel.about-dialog .about-blurb {
  margin: 0;
  font-size: 13px;
  line-height: 1.6;
  color: var(--ink);
}

.app-modal-panel.about-dialog .about-actions {
  display: flex;
  justify-content: flex-end;
}

.app-modal-panel.about-dialog .about-actions button {
  font-size: 14px;
  padding: 6px 14px;
  background: var(--field);
}
</style>
