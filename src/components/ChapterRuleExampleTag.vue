<script setup lang="ts">
import type { ChapterExamplePreviewItem } from "../lib/chapterRules";

defineProps<{
  item: ChapterExamplePreviewItem;
}>();
</script>

<template>
  <span class="tag" :class="{ success: item.hit }">
    <template v-if="item.hit && item.capture">
      {{ item.text.slice(0, item.capture[0])
      }}<span class="tag-capture">{{
        item.text.slice(item.capture[0], item.capture[1])
      }}</span>{{ item.text.slice(item.capture[1]) }}
    </template>
    <template v-else>{{ item.text }}</template>
  </span>
</template>

<style scoped>
.tag.success {
  /* Keep green chip; only the capture run uses success ink. */
  color: var(--ink);
}

.tag-capture {
  color: var(--success);
  font-weight: 600;
}
</style>
