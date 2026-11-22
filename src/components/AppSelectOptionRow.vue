<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { AppSelectOption } from "./AppCustomSelect.vue";
import AppLangTags from "./AppLangTags.vue";
import Icon from "./Icon.vue";

const props = defineProps<{
  option: AppSelectOption;
}>();

const stacked = computed(
  () => !!(props.option.tags?.length || props.option.description?.trim()),
);

const labelRef = ref<HTMLElement | null>(null);
/** Defer font-family until visible — applying it to hundreds of rows freezes WebView. */
const previewFont = ref(false);
let io: IntersectionObserver | null = null;

onMounted(() => {
  const family = props.option.labelFontFamily?.trim();
  if (!family || !labelRef.value) return;
  io = new IntersectionObserver(
    (entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      previewFont.value = true;
      io?.disconnect();
      io = null;
    },
    { root: null, rootMargin: "120px 0px", threshold: 0 },
  );
  io.observe(labelRef.value);
});

onBeforeUnmount(() => {
  io?.disconnect();
  io = null;
});

const labelStyle = computed(() => {
  const family = props.option.labelFontFamily?.trim();
  if (!family || !previewFont.value) return undefined;
  return { fontFamily: `'${family}', sans-serif` };
});
</script>

<template>
  <span class="app-select-option-row" :class="{ 'app-select-option-row--stacked': stacked }">
    <span class="app-select-option-row-head">
      <span v-if="option.flag" class="lang-flag" :data-cc="option.flag" aria-hidden="true" />
      <Icon v-else-if="option.icon" :name="option.icon" class="app-select-option-row-icon" />
      <span
        ref="labelRef"
        class="app-select-option-row-label"
        :style="labelStyle"
        >{{ option.label }}<span
          v-if="option.labelSuffix?.trim()"
          class="app-select-option-row-label-suffix"
          >{{ option.labelSuffix }}</span
        ></span
      >
      <span v-if="option.description?.trim()" class="app-select-option-row-desc">{{
        option.description
      }}</span>
    </span>
    <AppLangTags
      v-if="option.tags?.length"
      class="app-select-option-row-meta"
      :tags="option.tags"
    />
  </span>
</template>

<style scoped>
.app-select-option-row {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  align-items: stretch;
  justify-content: center;
  gap: 2px;
  min-width: 0;
  width: max-content;
  max-width: 100%;
  text-align: left;
  white-space: normal;
}

.app-select-option-row--stacked {
  gap: 6px;
}

.app-select-option-row-head {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
  width: max-content;
  max-width: 100%;
  min-width: 0;
}

.app-select-option-row--stacked .app-select-option-row-head {
  flex-wrap: wrap;
  align-items: flex-start;
}

.app-select-option-row-icon {
  --icon-size: 20px;
  flex: none;
  width: 20px;
  height: 20px;
  color: var(--muted);
}

.app-select-option-row-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.5;
}

.app-select-option-row-label-suffix {
  font-weight: 400;
  color: var(--muted);
}

.app-select-option-row--stacked .app-select-option-row-label {
  overflow: visible;
  text-overflow: unset;
  white-space: normal;
}

.app-select-option-row-desc {
  flex: 0 1 auto;
  max-width: 52%;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 12px;
  line-height: 1.45;
  font-weight: 400;
  color: var(--muted);
  white-space: nowrap;
}

.app-select-option-row--stacked .app-select-option-row-desc {
  flex: 1 1 100%;
  max-width: 100%;
  overflow: visible;
  text-overflow: unset;
  white-space: normal;
}

.app-select-option-row-meta {
  max-width: 100%;
}
</style>
