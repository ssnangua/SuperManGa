<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useTemplateRef,
  watch,
} from "vue";
import { type AppLangTag } from "./AppLangTags.vue";
import AppSelectOptionRow from "./AppSelectOptionRow.vue";
import Icon from "./Icon.vue";
import LoadingDotsBounce from "./LoadingDotsBounce.vue";
import "../lib/langFlag.css";

export type AppSelectTag = AppLangTag;

export type AppSelectOption = {
  id: string;
  label: string;
  description?: string;
  tags?: readonly AppSelectTag[];
  disabled?: boolean;
  /** ISO 3166-1 alpha-2 on the flag sprite. */
  flag?: string;
  /** Fallback Icon.vue name when there is no flag (e.g. auto-detect). */
  icon?: string;
  /** Preview the label with this CSS font-family (e.g. system font picker). */
  labelFontFamily?: string;
};

/** Independent checkbox rows rendered under the primary list (does not close panel). */
export type AppSelectToggleOption = AppSelectOption & {
  on?: boolean;
  /** Short trigger badge when on (joined with " + ", e.g. 辅助 / 纠错). */
  badge?: string;
};

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: readonly AppSelectOption[];
    ariaLabel: string;
    /** Override trigger text (e.g. primary + aux/refine summary). */
    triggerLabel?: string;
    /** Optional toggle rows below a separator (e.g. aux / refine). */
    toggleOptions?: readonly AppSelectToggleOption[];
    /** Middle list max height (px). */
    scrollMaxHeight?: number;
    /** Dropdown panel min width (px). Defaults to the trigger width. */
    panelMinWidth?: number;
    /** Compact panel with centered bounce loader (e.g. system fonts). */
    loading?: boolean;
    /** When set, show a sticky filter field above the scrollable list. */
    filterPlaceholder?: string;
  }>(),
  {
    triggerLabel: "",
    toggleOptions: () => [],
    scrollMaxHeight: 220,
    panelMinWidth: 0,
    loading: false,
    filterPlaceholder: "",
  },
);

const emit = defineEmits<{
  "update:modelValue": [id: string];
  toggle: [id: string];
  "open-change": [open: boolean];
}>();

const open = ref(false);
const filterQuery = ref("");
const filterInputRef = useTemplateRef<HTMLInputElement>("filterInputRef");
const triggerRef = useTemplateRef<HTMLButtonElement>("triggerRef");
const panelRef = useTemplateRef<HTMLElement>("panelRef");
const scrollRef = useTemplateRef<HTMLElement>("scrollRef");
const hasYScroll = ref(false);
const posLeft = ref(0);
const posTop = ref(0);
const panelWidth = ref(160);
let scrollObserver: ResizeObserver | null = null;

function syncScrollGutter() {
  const el = scrollRef.value;
  if (!el) {
    hasYScroll.value = false;
    return;
  }
  hasYScroll.value = el.scrollHeight > el.clientHeight + 1;
}

const selected = computed(
  () => props.options.find((o) => o.id === props.modelValue) ?? null,
);

const displayLabel = computed(() => {
  const custom = props.triggerLabel.trim();
  if (custom) return custom;
  const base = selected.value?.label || props.modelValue;
  const badges = props.toggleOptions
    .filter((o) => o.on && o.badge?.trim())
    .map((o) => o.badge!.trim());
  return badges.length ? [base, ...badges].join(" + ") : base;
});
const displayFlag = computed(() => selected.value?.flag);
const displayIcon = computed(() => selected.value?.icon);
const displayLabelFont = computed(() => {
  // Only preview the selected face when using the normal label (not a custom trigger).
  if (props.triggerLabel.trim()) return "";
  return selected.value?.labelFontFamily?.trim() || "";
});

const filterable = computed(() => Boolean(props.filterPlaceholder.trim()));

const visibleOptions = computed(() => {
  const q = filterQuery.value.trim().toLowerCase();
  if (!q) return props.options;
  return props.options.filter((o) => {
    if (o.label.toLowerCase().includes(q)) return true;
    if (o.id.toLowerCase().includes(q)) return true;
    return false;
  });
});

function applyPanelPosition(margin = 8, gap = 4) {
  const trig = triggerRef.value;
  const panel = panelRef.value;
  if (!trig || !panel) return;
  const r = trig.getBoundingClientRect();
  const h = panel.offsetHeight;
  const w = panel.offsetWidth;
  if (h < 1 || w < 1) return;

  const spaceBelow = window.innerHeight - margin - r.bottom - gap;
  const spaceAbove = r.top - margin - gap;

  let top: number;
  if (h <= spaceBelow) top = r.bottom + gap;
  else if (h <= spaceAbove) top = r.top - h - gap;
  else if (spaceAbove >= spaceBelow) top = Math.max(margin, r.top - h - gap);
  else top = Math.min(r.bottom + gap, window.innerHeight - margin - h);

  posTop.value = Math.min(
    Math.max(margin, top),
    Math.max(margin, window.innerHeight - h - margin),
  );
  const maxX = Math.max(margin, window.innerWidth - w - margin);
  posLeft.value = Math.min(Math.max(margin, r.left), maxX);
}

async function positionPanel() {
  const trig = triggerRef.value;
  if (!trig) return;
  const r = trig.getBoundingClientRect();
  panelWidth.value = Math.min(
    Math.max(r.width, props.panelMinWidth, 140),
    Math.max(160, window.innerWidth - 16),
  );
  posLeft.value = r.left;
  posTop.value = r.bottom + 4;
  await nextTick();
  applyPanelPosition();
  requestAnimationFrame(() => applyPanelPosition());
}

function setOpen(next: boolean) {
  if (open.value === next) return;
  open.value = next;
  emit("open-change", next);
  if (next) {
    void positionPanel();
  } else {
    filterQuery.value = "";
  }
}

function toggle() {
  setOpen(!open.value);
}

function close() {
  setOpen(false);
}

function select(id: string) {
  const hit = props.options.find((o) => o.id === id);
  if (hit?.disabled) return;
  emit("update:modelValue", id);
  close();
}

function onToggle(id: string) {
  const hit = props.toggleOptions.find((o) => o.id === id);
  if (hit?.disabled) return;
  emit("toggle", id);
}

function onDocPointerDown(ev: PointerEvent) {
  if (!open.value) return;
  const t = ev.target as Node | null;
  if (t && panelRef.value?.contains(t)) return;
  if (t && triggerRef.value?.contains(t)) return;
  close();
}

function onKey(ev: KeyboardEvent) {
  if (!open.value) return;
  if (ev.key === "Escape") {
    ev.preventDefault();
    ev.stopPropagation();
    close();
  }
}

watch(open, async (v) => {
  if (!v) {
    hasYScroll.value = false;
    scrollObserver?.disconnect();
    scrollObserver = null;
    return;
  }
  await nextTick();
  await positionPanel();
  if (!props.loading && filterable.value) {
    filterInputRef.value?.focus();
  }
  const selectedEl = panelRef.value?.querySelector<HTMLElement>(
    ".app-custom-select-option.on",
  );
  // Center so long lists (e.g. fonts) land on the current value, not only "nearest".
  selectedEl?.scrollIntoView({ block: "center" });
  syncScrollGutter();
  const el = scrollRef.value;
  if (el && typeof ResizeObserver !== "undefined") {
    scrollObserver?.disconnect();
    scrollObserver = new ResizeObserver(() => syncScrollGutter());
    scrollObserver.observe(el);
  }
});

watch(
  () =>
    [props.loading, props.options.length, props.toggleOptions.length, filterQuery.value] as const,
  async () => {
    if (!open.value) return;
    await nextTick();
    await positionPanel();
    syncScrollGutter();
    if (!props.loading && filterable.value) {
      filterInputRef.value?.focus();
    }
  },
);

onMounted(() => {
  document.addEventListener("pointerdown", onDocPointerDown, true);
  document.addEventListener("keydown", onKey, true);
  window.addEventListener("resize", close);
  window.addEventListener("blur", close);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocPointerDown, true);
  document.removeEventListener("keydown", onKey, true);
  window.removeEventListener("resize", close);
  window.removeEventListener("blur", close);
  scrollObserver?.disconnect();
  scrollObserver = null;
});
</script>

<template>
  <div class="app-custom-select">
    <button
      ref="triggerRef"
      type="button"
      class="app-custom-select-trigger"
      :aria-expanded="open"
      aria-haspopup="listbox"
      :aria-label="ariaLabel"
      @click.stop="toggle"
    >
      <span v-if="displayFlag" class="lang-flag" :data-cc="displayFlag" aria-hidden="true" />
      <Icon v-else-if="displayIcon" :name="displayIcon" class="app-custom-select-lead-icon" />
      <span
        class="app-custom-select-label"
        :style="displayLabelFont ? { fontFamily: `'${displayLabelFont}', sans-serif` } : undefined"
        >{{ displayLabel }}</span
      >
      <Icon name="foldChevron" class="app-custom-select-chevron" />
    </button>
    <Teleport to="body">
      <div
        v-if="open"
        ref="panelRef"
        class="app-custom-select-panel"
        role="listbox"
        :style="{
          left: `${posLeft}px`,
          top: `${posTop}px`,
          width: `${panelWidth}px`,
        }"
        @click.stop
      >
        <div
          v-if="loading"
          class="app-custom-select-loading"
          role="status"
          aria-live="polite"
        >
          <LoadingDotsBounce :ellipsis="false" :size="28" />
        </div>
        <template v-else>
          <div v-if="filterable" class="app-custom-select-header">
            <input
              ref="filterInputRef"
              v-model="filterQuery"
              class="app-custom-select-filter"
              type="search"
              autocomplete="off"
              spellcheck="false"
              :placeholder="filterPlaceholder"
              :aria-label="filterPlaceholder"
              @keydown.stop
            />
          </div>
          <div
            ref="scrollRef"
            class="app-custom-select-scroll"
            :class="{ 'has-y-scroll': hasYScroll }"
            :style="{ maxHeight: `${scrollMaxHeight}px` }"
          >
            <button
              v-for="opt in visibleOptions"
              :key="opt.id"
              type="button"
              role="option"
              class="app-custom-select-option"
              :class="{
                on: opt.id === modelValue,
                'app-custom-select-option--stacked': !!(
                  opt.tags?.length || opt.description?.trim()
                ),
              }"
              :aria-selected="opt.id === modelValue"
              :disabled="opt.disabled"
              @click="select(opt.id)"
            >
              <AppSelectOptionRow :option="opt" />
            </button>
          </div>
          <div v-if="toggleOptions.length" class="app-custom-select-footer">
            <div class="app-custom-select-sep" aria-hidden="true" />
            <button
              v-for="opt in toggleOptions"
              :key="opt.id"
              type="button"
              class="app-custom-select-option"
              :class="{
                on: !!opt.on,
                'app-custom-select-option--stacked': !!(
                  opt.tags?.length || opt.description?.trim()
                ),
              }"
              :disabled="opt.disabled"
              @click="onToggle(opt.id)"
            >
              <AppSelectOptionRow :option="opt" />
            </button>
          </div>
        </template>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.app-custom-select {
  box-sizing: border-box;
  display: block;
  width: 100%;
  min-width: 0;
}

.app-custom-select-trigger {
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
  width: 100%;
  max-width: 100%;
  min-height: 32px;
  margin: 0;
  padding: 4px 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--field);
  color: var(--ink);
  font-size: 14px;
  line-height: 1.2;
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
}

.app-custom-select-trigger:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--field);
}

.app-custom-select-trigger:focus,
.app-custom-select-trigger:focus-visible,
.app-custom-select-trigger[aria-expanded="true"] {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--field);
  outline: none;
}

.app-custom-select-lead-icon {
  --icon-size: 20px;
  flex: none;
  width: 20px;
  height: 20px;
  color: var(--muted);
}

.app-custom-select-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}

/* Fold chevron points down; rotate 180° when open. */
.app-custom-select-chevron {
  --icon-size: 10px;
  flex-shrink: 0;
  width: 10px;
  height: 10px;
  margin-left: auto;
  color: color-mix(in srgb, var(--muted) 85%, var(--ink));
  transition: transform 0.22s ease;
}

.app-custom-select-trigger[aria-expanded="true"] .app-custom-select-chevron {
  transform: rotate(180deg);
  color: currentColor;
}

.app-custom-select-chevron :deep(svg) {
  width: 10px;
  height: 10px;
  display: block;
}

.app-custom-select-chevron :deep(svg path) {
  fill: currentColor;
}

.app-custom-select-panel {
  position: fixed;
  z-index: var(--z-popover);
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  gap: 0;
  padding: 4px;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--menu, var(--control-bg));
  box-shadow: var(--menu-shadow, 0 8px 24px color-mix(in srgb, var(--ink) 18%, transparent));
}

.app-custom-select-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  width: 100%;
  min-height: 40px;
  height: 40px;
  padding: 8px;
  color: var(--muted);
}

.app-custom-select-header {
  flex: none;
  padding: 2px 2px 6px;
}

.app-custom-select-filter {
  box-sizing: border-box;
  display: block;
  width: 100%;
  min-width: 0;
  height: 30px;
  margin: 0;
  padding: 4px 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--field);
  color: var(--ink);
  font-size: 13px;
  line-height: 1.2;
}

.app-custom-select-filter:focus,
.app-custom-select-filter:focus-visible {
  border-color: var(--accent);
  outline: none;
}

.app-custom-select-scroll {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow: auto;
  min-height: 0;
  flex: 1 1 auto;
  box-sizing: border-box;
}

/* Match panel padding (4px): keep option fills clear of the scrollbar track. */
.app-custom-select-scroll.has-y-scroll {
  padding-right: 4px;
}

.app-custom-select-footer {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: none;
}

.app-custom-select-sep {
  height: 1px;
  margin: 4px 6px;
  background: var(--line);
}

.app-custom-select-option {
  display: flex;
  flex: 0 0 auto;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 2px;
  width: 100%;
  min-height: 40px;
  margin: 0;
  padding: 8px 12px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--ink);
  font-size: 13px;
  line-height: 1.4;
  text-align: left;
  white-space: normal;
  cursor: pointer;
}

.app-custom-select-option--stacked {
  gap: 6px;
  padding: 8px 10px;
  height: auto;
  min-height: 40px;
}

.app-custom-select-option.on :deep(.app-select-option-row-desc) {
  color: color-mix(in srgb, var(--list-item-fg-active) 72%, transparent);
}

.app-custom-select-option:disabled {
  opacity: 0.45;
  cursor: default;
}

/* Hover ≠ selected; selected keeps active bg while hovered. */
.app-custom-select-option:hover:not(:disabled, .on),
.app-custom-select-option:focus-visible:not(.on) {
  border: none;
  background: var(--list-item-bg-hover);
  color: var(--ink);
}

.app-custom-select-option:disabled:hover,
.app-custom-select-option:disabled:focus-visible {
  border: none;
  background: transparent;
  color: var(--ink);
}

.app-custom-select-option.on {
  border: none;
  background: var(--list-item-bg-active);
  color: var(--list-item-fg-active);
}

.app-custom-select-option.on:hover:not(:disabled) {
  border: none;
  background: var(--list-item-bg-active);
  color: var(--list-item-fg-active);
}
</style>
