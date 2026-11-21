<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed } from "vue";

import { appToastItems, dismissAppToast, type AppToastKind } from "../lib/appToast";
import { maskModalOpen, maskModalToastZIndex } from "../lib/maskModal";
import {
  chromeOverlay,
  opening,
} from "../reader";
import Icon from "./Icon.vue";
import LoadingDotsBounce from "./LoadingDotsBounce.vue";

const { t } = useI18n();

/** Keep toasts readable above mask modals / their popovers. */
const toastHostStyle = computed(() =>
  maskModalOpen.value ? { zIndex: maskModalToastZIndex() } : undefined,
);
/** File open only — extract progress lives on the seek bar (warning segments). */
const centerStatus = computed(() => {
  if (opening.value) {
    return { key: "opening", text: t("opening"), loading: true };
  }
  return null;
});

function iconName(kind: AppToastKind): string {
  switch (kind) {
    case "success":
      return "success";
    case "warning":
      return "warning";
    case "error":
      return "fail";
    case "info":
      return "info";
    default:
      return "";
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="centerStatus"
      class="app-toast-host app-toast-host--center"
      :style="toastHostStyle"
      aria-live="polite"
    >
      <div
        :key="centerStatus.key"
        class="app-toast-item app-toast-item--none"
        :class="{
          'app-toast-item--no-icon': !centerStatus.loading,
          'app-toast-item--loading': centerStatus.loading,
        }"
        role="status"
      >
        <span class="app-toast-msg">{{ centerStatus.text }}</span>
        <LoadingDotsBounce v-if="centerStatus.loading" />
      </div>
    </div>

    <div
      class="app-toast-host app-toast-host--top"
      :class="{ overlay: chromeOverlay }"
      :style="toastHostStyle"
      aria-live="polite"
    >
      <div class="app-toast-stack">
        <TransitionGroup name="app-toast">
          <div
            v-for="item in appToastItems"
            :key="item.id"
            class="app-toast-item"
            :class="[
              `app-toast-item--${item.kind}`,
              { 'app-toast-item--closable': item.showClose },
              { 'app-toast-item--no-icon': !iconName(item.kind) },
            ]"
            :role="item.kind === 'error' ? 'alert' : 'status'"
          >
            <span v-if="iconName(item.kind)" class="app-toast-icon" aria-hidden="true">
              <Icon :name="iconName(item.kind)" />
            </span>
            <span class="app-toast-msg">{{ item.message }}</span>
            <button
              v-if="item.showClose"
              type="button"
              class="app-toast-close"
              :aria-label="t('close')"
              @click.stop="dismissAppToast(item.id)"
            >
              <span class="app-toast-close-icon" aria-hidden="true">
                <Icon name="close" />
              </span>
            </button>
          </div>
        </TransitionGroup>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.app-toast-host {
  position: fixed;
  z-index: var(--z-toast);
  display: flex;
  pointer-events: none;
  box-sizing: border-box;
}

.app-toast-host--top {
  top: 32px;
  left: 0;
  right: 0;
  justify-content: center;
  padding: 12px 12px 0;
}

.app-toast-host--top.overlay {
  top: 0;
}

.app-toast-host--center {
  inset: 0;
  align-items: center;
  justify-content: center;
}

.app-toast-stack {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  width: auto;
  max-width: min(520px, 100%);
  pointer-events: none;
}

.app-toast-item {
  pointer-events: auto;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 4px;
  width: fit-content;
  max-width: min(520px, 100%);
  box-sizing: border-box;
  padding: 4px 10px;
  border-radius: 4px;
  background: var(--control-bg);
  box-shadow:
    0 4px 18px color-mix(in srgb, var(--ink) 12%, transparent),
    0 0 1px color-mix(in srgb, var(--ink) 8%, transparent);
  border: 1px solid var(--line);
  color: var(--ink);
}

.app-toast-item--closable {
  padding-right: 4px;
}

.app-toast-item--no-icon {
  padding-left: 12px;
  padding-right: 12px;
}

.app-toast-item--loading {
  align-items: baseline;
  gap: 1px;
}

.app-toast-icon {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: inherit;
}

.app-toast-icon :deep(.asset-icon),
.app-toast-icon :deep(svg) {
  width: 20px;
  height: 20px;
  display: block;
}

.app-toast-msg {
  flex: 0 1 auto;
  min-width: 0;
  font-size: 13px;
  line-height: 22px;
  word-break: break-word;
  white-space: pre-line;
  text-align: center;
  user-select: text;
  cursor: text;
}

.app-toast-item--closable .app-toast-msg {
  flex: 1 1 auto;
}

.app-toast-close {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  margin: 0;
  padding: 0;
  border: none;
  outline: none;
  border-radius: 6px;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}

.app-toast-close:hover,
.app-toast-close:focus-visible {
  border: none;
  background: transparent;
  color: var(--ink);
}

/* 12px glyph inside 28px hit target (not global --icon-size 16px). */
.app-toast-close-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  height: 12px;
}

.app-toast-close-icon :deep(.asset-icon),
.app-toast-close-icon :deep(svg) {
  width: 12px;
  height: 12px;
  display: block;
}

.app-toast-item--success {
  color: var(--success);
  border-color: color-mix(in srgb, var(--success) 30%, transparent);
  background: var(--success-bg);
}

.app-toast-item--warning {
  color: var(--warning);
  border-color: color-mix(in srgb, var(--warning) 30%, transparent);
  background: var(--warning-bg);
}

.app-toast-item--error {
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 30%, transparent);
  background: var(--danger-bg);
}

.app-toast-item--info {
  color: var(--info);
  border-color: color-mix(in srgb, var(--info) 30%, transparent);
  background: var(--info-bg);
}

.app-toast-item--none {
  color: var(--ink);
}

.app-toast-enter-active,
.app-toast-leave-active {
  transition:
    transform 0.28s cubic-bezier(0.22, 1, 0.36, 1),
    opacity 0.22s ease;
}

.app-toast-enter-from,
.app-toast-leave-to {
  opacity: 0;
  transform: translateY(-14px);
}

.app-toast-move {
  transition: transform 0.24s ease;
}
</style>
