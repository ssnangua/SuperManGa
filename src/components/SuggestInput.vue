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

export type SuggestInputItem = {
  id: string;
  label?: string;
  description?: string;
};

const props = withDefaults(
  defineProps<{
    modelValue: string;
    items: readonly SuggestInputItem[];
    placeholder?: string;
    ariaLabel?: string;
    inputClass?: string;
    scrollMaxHeight?: number;
    /** When set, show a sticky filter field above the scrollable list. */
    filterPlaceholder?: string;
  }>(),
  {
    placeholder: "",
    ariaLabel: "",
    inputClass: "",
    scrollMaxHeight: 220,
    filterPlaceholder: "",
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  focus: [];
}>();

const focused = ref(false);
const filterQuery = ref("");
const inputRef = useTemplateRef<HTMLInputElement>("inputRef");
const filterInputRef = useTemplateRef<HTMLInputElement>("filterInputRef");
const panelRef = useTemplateRef<HTMLElement>("panelRef");
const posLeft = ref(0);
const posTop = ref(0);
const panelWidth = ref(160);

const filterable = computed(() => Boolean(props.filterPlaceholder.trim()));

const allItems = computed(() => {
  const seen = new Set<string>();
  const out: SuggestInputItem[] = [];
  for (const raw of props.items) {
    const id = raw.id.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    out.push({
      id,
      label: raw.label?.trim() || undefined,
      description: raw.description?.trim() || undefined,
    });
  }
  return out;
});

const visibleItems = computed(() => {
  const q = filterQuery.value.trim().toLowerCase();
  if (!q) return allItems.value;
  return allItems.value.filter((item) => {
    if (item.id.toLowerCase().includes(q)) return true;
    if (item.label?.toLowerCase().includes(q)) return true;
    if (item.description?.toLowerCase().includes(q)) return true;
    return false;
  });
});

const listOpen = computed(
  () => focused.value && allItems.value.length > 0,
);

function applyPanelPosition(margin = 8, gap = 4) {
  const trig = inputRef.value;
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
  const trig = inputRef.value;
  if (!trig) return;
  const r = trig.getBoundingClientRect();
  panelWidth.value = Math.max(r.width, 140);
  posLeft.value = r.left;
  posTop.value = r.bottom + 4;
  await nextTick();
  applyPanelPosition();
  requestAnimationFrame(() => applyPanelPosition());
}

function panelKeepsFocus() {
  const active = document.activeElement;
  if (active === inputRef.value || active === filterInputRef.value) return true;
  if (active && panelRef.value?.contains(active)) return true;
  return false;
}

function onFocus() {
  focused.value = true;
  emit("focus");
}

function onBlur() {
  window.setTimeout(() => {
    if (panelKeepsFocus()) return;
    focused.value = false;
  }, 120);
}

function onInput(ev: Event) {
  emit("update:modelValue", (ev.target as HTMLInputElement).value);
}

function pick(id: string) {
  emit("update:modelValue", id);
  focused.value = false;
  filterQuery.value = "";
}

function onKeydown(ev: KeyboardEvent) {
  if (ev.key === "Escape") {
    focused.value = false;
    filterQuery.value = "";
    (ev.target as HTMLInputElement).blur();
  }
}

function onWindowResize() {
  if (listOpen.value) focused.value = false;
}

function onWindowBlur() {
  focused.value = false;
}

watch(listOpen, async (open) => {
  if (!open) {
    filterQuery.value = "";
    return;
  }
  await nextTick();
  await positionPanel();
  if (filterable.value) {
    filterInputRef.value?.focus();
  }
  const selectedEl = panelRef.value?.querySelector<HTMLElement>(
    ".suggest-input-option.on",
  );
  selectedEl?.scrollIntoView({ block: "center" });
});

watch(filterQuery, async () => {
  if (!listOpen.value) return;
  await nextTick();
  applyPanelPosition();
});

onMounted(() => {
  window.addEventListener("resize", onWindowResize);
  window.addEventListener("blur", onWindowBlur);
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", onWindowResize);
  window.removeEventListener("blur", onWindowBlur);
});
</script>

<template>
  <div class="suggest-input">
    <input
      ref="inputRef"
      :value="modelValue"
      type="text"
      autocomplete="off"
      spellcheck="false"
      class="suggest-input-field"
      :class="inputClass"
      :placeholder="placeholder"
      :aria-label="ariaLabel"
      :aria-expanded="listOpen"
      aria-autocomplete="list"
      @input="onInput"
      @focus="onFocus"
      @blur="onBlur"
      @keydown="onKeydown"
    />
    <Teleport to="body">
      <div
        v-if="listOpen"
        ref="panelRef"
        class="suggest-input-panel"
        role="listbox"
        :style="{
          left: `${posLeft}px`,
          top: `${posTop}px`,
          width: `${panelWidth}px`,
        }"
        @click.stop
      >
        <div v-if="filterable" class="suggest-input-header">
          <input
            ref="filterInputRef"
            v-model="filterQuery"
            class="suggest-input-filter"
            type="search"
            autocomplete="off"
            spellcheck="false"
            :placeholder="filterPlaceholder"
            :aria-label="filterPlaceholder"
            @keydown.stop="onKeydown"
            @focus="focused = true"
            @blur="onBlur"
          />
        </div>
        <div
          class="suggest-input-scroll"
          :style="{ maxHeight: `${scrollMaxHeight}px` }"
        >
          <button
            v-for="item in visibleItems"
            :key="item.id"
            type="button"
            role="option"
            class="suggest-input-option"
            :class="{
              on: item.id === modelValue,
              stacked: Boolean(item.description || (item.label && item.label !== item.id)),
            }"
            @mousedown.prevent
            @click="pick(item.id)"
          >
            <span class="suggest-input-option-label">{{ item.label || item.id }}</span>
            <span v-if="item.description" class="suggest-input-option-desc">{{
              item.description
            }}</span>
          </button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.suggest-input {
  position: relative;
  box-sizing: border-box;
  display: block;
  min-width: 0;
  max-width: 100%;
  width: 100%;
}

.suggest-input-field {
  box-sizing: border-box;
  width: 100%;
  min-width: 0;
}

.suggest-input-panel {
  position: fixed;
  z-index: var(--z-popover);
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  gap: 0;
  min-width: 140px;
  padding: 4px;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--menu, var(--control-bg));
  box-shadow: var(--menu-shadow, 0 8px 24px color-mix(in srgb, var(--ink) 18%, transparent));
}

.suggest-input-header {
  flex: none;
  padding: 2px 2px 6px;
}

.suggest-input-filter {
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

.suggest-input-filter:focus,
.suggest-input-filter:focus-visible {
  border-color: var(--accent);
  outline: none;
}

.suggest-input-scroll {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow: auto;
  min-height: 0;
  flex: 1 1 auto;
  box-sizing: border-box;
}

.suggest-input-option {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 2px;
  width: 100%;
  min-height: 36px;
  margin: 0;
  padding: 6px 10px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--ink);
  font-size: 13px;
  line-height: 1.2;
  text-align: left;
  cursor: pointer;
}

.suggest-input-option.stacked {
  min-height: 44px;
}

.suggest-input-option:hover:not(.on),
.suggest-input-option:focus-visible:not(.on) {
  border: none;
  background: var(--list-item-bg-hover);
  color: var(--ink);
}

.suggest-input-option.on {
  background: var(--list-item-bg-active);
  color: var(--list-item-fg-active);
}

.suggest-input-option.on:hover {
  background: var(--list-item-bg-active);
  color: var(--list-item-fg-active);
}

.suggest-input-option-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

.suggest-input-option-desc {
  font-size: 12px;
  line-height: 1.3;
  color: var(--muted);
  word-break: break-all;
}
</style>
