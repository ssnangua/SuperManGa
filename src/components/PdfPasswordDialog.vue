<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, ref, watch } from "vue";

import {
  pdfPasswordPrompt,
  resolvePdfPasswordPrompt,
} from "../reader";
import AppModal from "./AppModal.vue";

const { t } = useI18n();
const SHOW_PASSWORD_KEY = "supermanga.pdfPromptShowPassword";

const open = computed({
  get: () => pdfPasswordPrompt.value != null,
  set: (value) => {
    if (!value) resolvePdfPasswordPrompt(null);
  },
});

const password = ref("");
const revealPassword = ref(false);
const inputEl = ref<HTMLInputElement | null>(null);

const message = computed(() =>
  pdfPasswordPrompt.value?.wrong ? t("pdfPasswordWrong") : t("pdfPasswordRequired"),
);

function readRevealPreference() {
  try {
    return localStorage.getItem(SHOW_PASSWORD_KEY) === "1";
  } catch {
    return false;
  }
}

function persistRevealPreference(reveal: boolean) {
  try {
    localStorage.setItem(SHOW_PASSWORD_KEY, reveal ? "1" : "0");
  } catch {
    // ignore
  }
}

watch(open, async (isOpen) => {
  if (!isOpen) return;
  password.value = "";
  revealPassword.value = readRevealPreference();
  await nextTick();
  const el = inputEl.value;
  el?.focus();
  el?.select();
});

watch(revealPassword, (reveal) => {
  if (!open.value) return;
  persistRevealPreference(reveal);
});

function submit() {
  resolvePdfPasswordPrompt(password.value);
}

function cancel() {
  resolvePdfPasswordPrompt(null);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    submit();
  }
}
</script>

<template>
  <AppModal
    v-model="open"
    :title="t('pdfPasswordTitle')"
    :mask-closable="false"
    :close-on-esc="true"
    :body-scroll="false"
    max-width="440px"
    panel-class="pdf-password-dialog"
  >
    <p class="pdf-password-msg">{{ message }}</p>
    <input
      ref="inputEl"
      v-model="password"
      class="pdf-password-input"
      :type="revealPassword ? 'text' : 'password'"
      :placeholder="t('pdfPasswordPlaceholder')"
      autocomplete="off"
      spellcheck="false"
      :aria-label="t('pdfPasswordPlaceholder')"
      @keydown="onKeydown"
    />
    <template #footer>
      <div class="app-modal-actions">
        <label class="checkbox pdf-password-reveal">
          <input v-model="revealPassword" type="checkbox" />
          <span class="checkbox-label">{{ t("revealPassword") }}</span>
        </label>
        <div class="app-modal-actions-end">
          <button type="button" class="ghost" @click="cancel">{{ t("cancel") }}</button>
          <button type="button" class="primary" @click="submit">{{ t("ok") }}</button>
        </div>
      </div>
    </template>
  </AppModal>
</template>

<style>
.app-modal-panel.pdf-password-dialog {
  max-height: min(90vh, 320px);
}

.app-modal-panel.pdf-password-dialog .app-modal-body {
  gap: 12px;
}

.app-modal-panel.pdf-password-dialog .pdf-password-msg {
  margin: 0;
  font-size: 14px;
  line-height: 1.55;
  word-break: break-word;
}

.app-modal-panel.pdf-password-dialog .pdf-password-input {
  width: 100%;
  box-sizing: border-box;
  padding: 8px 10px;
  font-size: 14px;
  line-height: 1.4;
  border-radius: 6px;
  border: 1px solid var(--line);
  background: var(--field);
  color: var(--ink);
}

.app-modal-panel.pdf-password-dialog .pdf-password-input:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
}

.app-modal-panel.pdf-password-dialog .pdf-password-reveal {
  flex-shrink: 0;
  font-size: 13px;
}

.app-modal-panel.pdf-password-dialog button.primary:not(:disabled) {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}

.app-modal-panel.pdf-password-dialog button.primary:hover:not(:disabled) {
  background: var(--accent-hover);
  color: #fff;
  border-color: var(--accent-hover);
}
</style>
