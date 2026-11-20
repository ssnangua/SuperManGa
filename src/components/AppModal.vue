<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, onBeforeUnmount, reactive, ref, useId, useSlots, watch } from "vue";

import { maskModalZIndex, registerMaskModal } from "../lib/maskModal";
import Icon from "./Icon.vue";

const { t } = useI18n();
const INSET = 8;
let anonMaskSeq = 0;

const props = withDefaults(
  defineProps<{
    title?: string;
    maxWidth?: string;
    /** Show dimmed backdrop. When false, the panel floats without blocking the page. */
    mask?: boolean;
    maskClosable?: boolean;
    /** When false, Esc is swallowed while this modal is topmost (does not close lower ones). */
    closeOnEsc?: boolean;
    /**
     * Stable stack identity (survives HMR remount). Defaults to `panelClass`, then a
     * per-instance anonymous key.
     */
    maskKey?: string;
    /** Drag the panel by its header. */
    draggable?: boolean;
    showCloseButton?: boolean;
    bodyScroll?: boolean;
    panelClass?: string;
    ariaLabel?: string;
  }>(),
  {
    title: "",
    maxWidth: "520px",
    mask: true,
    maskClosable: true,
    closeOnEsc: true,
    maskKey: "",
    draggable: false,
    showCloseButton: true,
    bodyScroll: true,
    panelClass: "",
    ariaLabel: "",
  },
);

const modelValue = defineModel<boolean>({ default: false });
const slots = useSlots();
const titleId = useId();
const panelEl = ref<HTMLElement | null>(null);
const pos = reactive({ x: 0, y: 0, ready: false });
const anonMaskKey = `anon-mask-${++anonMaskSeq}`;
let drag: { ox: number; oy: number; px: number; py: number } | null = null;
let unregisterMask: (() => void) | null = null;

const resolvedMaskKey = computed(
  () => props.maskKey.trim() || props.panelClass.trim() || anonMaskKey,
);

function syncMaskRegistration(open: boolean) {
  unregisterMask?.();
  unregisterMask = null;
  if (!open || !props.mask) return;
  unregisterMask = registerMaskModal({
    key: resolvedMaskKey.value,
    close: () => {
      modelValue.value = false;
    },
    closeOnEsc: props.closeOnEsc,
  });
}

const hasTitle = computed(() => Boolean(props.title.trim()));
const showHeader = computed(
  () => hasTitle.value || props.showCloseButton || Boolean(slots.headerActions),
);

const backdropStyle = computed(() => ({
  zIndex: props.mask ? maskModalZIndex(resolvedMaskKey.value) : undefined,
}));

const panelStyle = computed(() => {
  const style: Record<string, string> = { maxWidth: props.maxWidth };
  if (props.draggable && pos.ready) {
    style.left = `${pos.x}px`;
    style.top = `${pos.y}px`;
  }
  return style;
});

function close() {
  modelValue.value = false;
}

function onMaskClick() {
  if (props.mask && props.maskClosable) close();
}

function clampPos(x: number, y: number) {
  const el = panelEl.value;
  const width = el?.offsetWidth ?? 280;
  const height = el?.offsetHeight ?? 180;
  const maxX = Math.max(INSET, window.innerWidth - width - INSET);
  const maxY = Math.max(INSET, window.innerHeight - height - INSET);
  pos.x = Math.min(maxX, Math.max(INSET, x));
  pos.y = Math.min(maxY, Math.max(INSET, y));
}

function centerPanel() {
  const el = panelEl.value;
  const width = el?.offsetWidth ?? 280;
  const height = el?.offsetHeight ?? 180;
  clampPos((window.innerWidth - width) / 2, (window.innerHeight - height) / 2);
  pos.ready = true;
}

function onHeaderPointerDown(event: PointerEvent) {
  if (!props.draggable || !pos.ready || event.button !== 0) return;
  const target = event.target;
  if (target instanceof HTMLElement && target.closest("button")) return;
  drag = { ox: event.clientX, oy: event.clientY, px: pos.x, py: pos.y };
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", onPointerUp);
  window.addEventListener("pointercancel", onPointerUp);
}

function onPointerMove(event: PointerEvent) {
  if (!drag) return;
  clampPos(drag.px + (event.clientX - drag.ox), drag.py + (event.clientY - drag.oy));
}

function onPointerUp() {
  drag = null;
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerup", onPointerUp);
  window.removeEventListener("pointercancel", onPointerUp);
}

function onViewportChange() {
  if (!props.draggable || !pos.ready) return;
  clampPos(pos.x, pos.y);
}

async function prepareDragLayout() {
  if (!props.draggable || !modelValue.value) return;
  pos.ready = false;
  await nextTick();
  centerPanel();
}

watch(
  modelValue,
  (open) => {
    syncMaskRegistration(open);
    if (!open) {
      onPointerUp();
      pos.ready = false;
      window.removeEventListener("resize", onViewportChange);
      return;
    }
    if (props.draggable) {
      window.addEventListener("resize", onViewportChange);
      void prepareDragLayout();
    }
  },
  { immediate: true },
);

watch(
  () => props.draggable,
  (enabled) => {
    if (!enabled) {
      onPointerUp();
      pos.ready = false;
      window.removeEventListener("resize", onViewportChange);
      return;
    }
    if (modelValue.value) {
      window.addEventListener("resize", onViewportChange);
      void prepareDragLayout();
    }
  },
);

onBeforeUnmount(() => {
  unregisterMask?.();
  unregisterMask = null;
  onPointerUp();
  window.removeEventListener("resize", onViewportChange);
});
</script>

<template>
  <Teleport to="body">
    <Transition name="app-modal">
      <div
        v-if="modelValue"
        class="app-modal-backdrop"
        :class="[panelClass, { 'no-mask': !mask }]"
        :style="backdropStyle"
        data-fullscreen-sidebar-float
        role="dialog"
        :aria-modal="mask ? 'true' : 'false'"
        :aria-label="hasTitle ? undefined : (ariaLabel || t('close'))"
        :aria-labelledby="hasTitle ? titleId : undefined"
        tabindex="-1"
        @click.self="onMaskClick"
      >
        <section
          ref="panelEl"
          class="app-modal-panel"
          :class="[panelClass, { floating: draggable && pos.ready }]"
          :style="panelStyle"
          @click.stop
        >
          <header
            v-if="showHeader"
            class="app-modal-header"
            :class="{ 'no-title': !hasTitle, grab: draggable }"
            @pointerdown="onHeaderPointerDown"
          >
            <h2 v-if="hasTitle" :id="titleId" class="app-modal-title">{{ title }}</h2>
            <div v-if="slots.headerActions || showCloseButton" class="app-modal-header-end">
              <slot name="headerActions" />
              <button
                v-if="showCloseButton"
                type="button"
                class="icon-btn app-modal-close"
                :aria-label="t('close')"
                :title="t('close')"
                @click="close"
              >
                <Icon name="close" />
              </button>
            </div>
          </header>
          <div class="app-modal-body" :class="{ 'no-scroll': !bodyScroll }">
            <slot />
          </div>
          <footer v-if="slots.footer" class="app-modal-footer">
            <slot name="footer" />
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.app-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: var(--modal);
  outline: none;
}

.app-modal-backdrop.no-mask {
  padding: 0;
  background: transparent;
  pointer-events: none;
}

.app-modal-enter-active,
.app-modal-leave-active {
  transition: opacity 0.22s ease;
}

.app-modal-enter-from,
.app-modal-leave-to {
  opacity: 0;
}

.app-modal-enter-active .app-modal-panel,
.app-modal-leave-active .app-modal-panel {
  transform-origin: center center;
  transition:
    transform 0.22s ease-out,
    opacity 0.2s ease-out;
}

.app-modal-enter-from .app-modal-panel {
  transform: scale(0.9);
  opacity: 0;
}

.app-modal-leave-to .app-modal-panel {
  transform: scale(0.96);
  opacity: 0;
}

.app-modal-panel {
  position: relative;
  width: 100%;
  max-height: min(90vh, 720px);
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  border-radius: 12px;
  border: 1px solid var(--line);
  background: var(--dialog);
  color: var(--ink);
  box-shadow: var(--menu-shadow);
  pointer-events: auto;
}

.app-modal-panel.floating {
  position: fixed;
  width: max-content;
  max-height: calc(100vh - 16px);
  margin: 0;
}

.app-modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex: none;
  min-width: 0;
}

.app-modal-header.grab {
  cursor: move;
  touch-action: none;
  user-select: none;
}

.app-modal-header.no-title {
  justify-content: flex-end;
  min-height: 0;
}

.app-modal-title {
  margin: 0;
  min-width: 0;
  font-size: 18px;
  font-weight: 600;
}

.app-modal-header-end {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: none;
}

.app-modal-close {
  margin: -4px -6px -4px 0;
}

/* icon-btn defaults to --icon-size-sm (12px); modal close uses the normal glyph. */
.app-modal-close :deep(.asset-icon),
.app-modal-close :deep(.asset-icon svg) {
  width: var(--icon-size);
  height: var(--icon-size);
}

.app-modal-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.app-modal-body.no-scroll {
  overflow: hidden;
}

.app-modal-footer {
  flex: none;
  width: 100%;
  min-width: 0;
}
</style>

<style>
/* Slot content (dialogs) — unscoped so labels inside default slot match. */
.app-modal-panel label {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: center;
}

.app-modal-panel label.check {
  justify-content: flex-start;
}

.app-modal-panel
  label:not(.check)
  :is(input[type="text"], input[type="number"], select) {
  width: 180px;
}

.app-modal-panel p {
  margin: 0;
}
</style>

