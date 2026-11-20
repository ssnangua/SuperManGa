<script setup lang="ts">
export type RadioGroupOption = {
  id: string;
  label: string;
};

const modelValue = defineModel<string>({ required: true });

withDefaults(
  defineProps<{
    options: readonly RadioGroupOption[];
    ariaLabel?: string;
    disabled?: boolean;
  }>(),
  { ariaLabel: "", disabled: false },
);

function select(id: string) {
  if (modelValue.value === id) return;
  modelValue.value = id;
}
</script>

<template>
  <div
    class="radio-group"
    :class="{ disabled }"
    role="radiogroup"
    :aria-label="ariaLabel || undefined"
  >
    <button
      v-for="opt in options"
      :key="opt.id"
      type="button"
      class="radio-group-option"
      role="radio"
      :aria-checked="modelValue === opt.id"
      :disabled="disabled"
      :class="{ on: modelValue === opt.id }"
      @click="select(opt.id)"
    >
      {{ opt.label }}
    </button>
  </div>
</template>

<style>
.radio-group {
  display: inline-flex;
  flex-shrink: 0;
  vertical-align: middle;
}

button.radio-group-option {
  position: relative;
  min-width: 0;
  margin-left: -1px;
  padding: var(--control-pad-y) var(--control-pad-x);
  border: 1px solid var(--line);
  background: var(--button);
  color: var(--ink);
  font-size: 14px;
  line-height: 1.2;
  white-space: nowrap;
  border-radius: 0;
}

button.radio-group-option:first-child {
  margin-left: 0;
  border-radius: var(--radius) 0 0 var(--radius);
}

button.radio-group-option:last-child {
  border-radius: 0 var(--radius) var(--radius) 0;
}

button.radio-group-option:only-child {
  border-radius: var(--radius);
}

button.radio-group-option:hover:not(:disabled),
button.radio-group-option.on:not(:disabled) {
  border-color: var(--line);
}

button.radio-group-option:hover:not(:disabled):not(.on) {
  color: var(--accent);
  background: var(--button);
  border-color: var(--line);
}

button.radio-group-option.on:not(:disabled),
button.radio-group-option.on:hover:not(:disabled) {
  z-index: 1;
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}
</style>
