<script setup lang="ts">
import { ref, watchEffect } from "vue";

const modelValue = defineModel<boolean>({ default: false });

const props = withDefaults(
  defineProps<{
    label?: string;
    disabled?: boolean;
    ariaLabel?: string;
    indeterminate?: boolean;
  }>(),
  {
    label: "",
    disabled: false,
    ariaLabel: "",
    indeterminate: false,
  },
);

const inputRef = ref<HTMLInputElement | null>(null);

watchEffect(() => {
  if (inputRef.value) inputRef.value.indeterminate = props.indeterminate;
});
</script>

<template>
  <label
    class="checkbox"
    :class="{ 'checkbox--disabled': disabled }"
  >
    <input
      ref="inputRef"
      v-model="modelValue"
      type="checkbox"
      :disabled="disabled"
      :aria-label="ariaLabel || label || undefined"
    />
    <span v-if="label || $slots.default" class="checkbox-label">
      <slot>{{ label }}</slot>
    </span>
  </label>
</template>

<style>
/* Checkbox. */
.checkbox {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  user-select: none;
  cursor: pointer;
  color: var(--ink);
  font-size: 14px;
  width: fit-content;
  max-width: 100%;
}

.checkbox--disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.checkbox-label {
  display: inline-flex;
  line-height: 1.4;
}

.checkbox input {
  appearance: none;
  -webkit-appearance: none;
  box-sizing: border-box;
  width: 14px;
  height: 14px;
  margin: 0;
  flex: none;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: var(--control-bg);
  display: inline-block;
  cursor: pointer;
  position: relative;
  overflow: hidden;
  vertical-align: middle;
}

.checkbox input::after {
  content: "";
  position: absolute;
  left: 50%;
  top: 50%;
  width: 4px;
  height: 8px;
  border: solid #ffffff;
  border-width: 0 2px 2px 0;
  transform: translate(-50%, -60%) rotate(45deg) scale(0);
  transform-origin: center;
  transition: transform 0.12s ease-in-out;
}

.checkbox input:checked {
  background: var(--accent);
  border-color: var(--accent);
}

.checkbox input:checked::after {
  transform: translate(-50%, -60%) rotate(45deg) scale(1);
}

.checkbox input:indeterminate {
  background: var(--accent);
  border-color: var(--accent);
}

.checkbox input:indeterminate::after {
  width: 8px;
  height: 2px;
  border: none;
  background: #ffffff;
  border-radius: 1px;
  transform: translate(-50%, -50%) scale(1);
}

.app-modal-panel label.checkbox {
  display: inline-flex;
  justify-content: flex-start;
  width: fit-content;
  gap: 6px;
}
</style>
