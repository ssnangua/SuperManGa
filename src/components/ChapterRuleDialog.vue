<script setup lang="ts">
import { computed, watch } from "vue";
import {
  applyChapterRulesFromPanel,
  chapterRuleError,
  chapterRuleOpen,
  settings,
} from "../reader";
import ChapterRulePanel from "./ChapterRulePanel.vue";

const open = computed({
  get: () => chapterRuleOpen.value,
  set: (value: boolean) => {
    chapterRuleOpen.value = value;
  },
});

watch(chapterRuleOpen, (shown) => {
  if (shown) chapterRuleError.value = "";
});
</script>

<template>
  <ChapterRulePanel
    v-model="open"
    :rules="settings.chapterRules"
    :error-text="chapterRuleError"
    @apply="applyChapterRulesFromPanel($event.rules)"
  />
</template>
