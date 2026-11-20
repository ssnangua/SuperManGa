<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";

const props = withDefaults(
  defineProps<{
    /** Glyph width; height follows the 2:1 bounce viewBox. Number = px. */
    size?: string | number;
    /**
     * Inline animated “…” next to text (default).
     * Set false for centered panel spinners (e.g. select loading).
     */
    ellipsis?: boolean;
  }>(),
  { size: "1.15em", ellipsis: true },
);

const widthCss = computed(() =>
  typeof props.size === "number" ? `${props.size}px` : props.size,
);
</script>

<template>
  <span
    class="loading-dots-bounce"
    :class="{ 'is-ellipsis': ellipsis }"
    :style="{ width: widthCss }"
    aria-hidden="true"
  >
    <Icon name="loadingDots" />
  </span>
</template>

<style scoped>
.loading-dots-bounce {
  display: inline-flex;
  flex: none;
  color: inherit;
  line-height: 0;
}

.loading-dots-bounce.is-ellipsis {
  align-items: flex-end;
  height: 1em;
  overflow: hidden;
  vertical-align: baseline;
  /* Resting dots sit mid-line in the viewBox; nudge onto the text baseline. */
  transform: translateY(0.2em);
}

.loading-dots-bounce:not(.is-ellipsis) {
  align-items: center;
  height: auto;
}

.loading-dots-bounce :deep(.asset-icon),
.loading-dots-bounce :deep(.asset-icon svg) {
  display: block;
  width: 100%;
  height: auto;
  aspect-ratio: 2 / 1;
  overflow: hidden;
}
</style>
