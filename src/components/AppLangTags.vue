<script setup lang="ts">
export type AppLangTag = {
  id: string;
  label: string;
};

defineProps<{
  tags: readonly AppLangTag[];
}>();
</script>

<template>
  <span v-if="tags.length" class="app-lang-tags">
    <span
      v-for="tag in tags"
      :key="tag.id"
      class="app-lang-tag"
      :data-tag="tag.id"
    >{{ tag.label }}</span>
  </span>
</template>

<style scoped>
.app-lang-tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  max-width: 100%;
}

.app-lang-tag {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  line-height: 1.5;
  font-weight: 500;
  color: #fff;
  background: #475569;
}

.app-lang-tag[data-tag="zh"] {
  background: #2563eb;
}

.app-lang-tag[data-tag="en"] {
  background: #0d9488;
}

.app-lang-tag[data-tag="ja"] {
  background: #7c3aed;
}

.app-lang-tag[data-tag="ko"] {
  background: #ca8a04;
}

.app-lang-tag[data-tag="fr"] {
  background: #db2777;
}

.app-lang-tag[data-tag="es"] {
  background: #ea580c;
}

.app-lang-tag[data-tag="pt"] {
  background: #16a34a;
}

.app-lang-tag[data-tag="de"] {
  background: #78716c;
}

.app-lang-tag[data-tag="more"] {
  min-width: 1.6em;
  justify-content: center;
  padding-inline: 5px;
  letter-spacing: 0.06em;
  color: color-mix(in srgb, var(--ink) 55%, transparent);
  background: color-mix(in srgb, var(--muted) 38%, var(--field));
}
</style>
