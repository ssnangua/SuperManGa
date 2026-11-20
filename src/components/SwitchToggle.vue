<script setup lang="ts">
const modelValue = defineModel<boolean>({ default: false });

withDefaults(
  defineProps<{
    ariaLabel?: string;
    size?: "md" | "sm";
    disabled?: boolean;
  }>(),
  { ariaLabel: "", size: "md", disabled: false },
);
</script>

<template>
  <label
    class="switch-toggle"
    :class="{
      'switch-toggle--sm': size === 'sm',
      'switch-toggle--disabled': disabled,
    }"
  >
    <input
      v-model="modelValue"
      class="switch-toggle-input"
      type="checkbox"
      role="switch"
      :disabled="disabled"
      :aria-checked="modelValue"
      :aria-label="ariaLabel || undefined"
    />
    <span class="switch-toggle-track" aria-hidden="true" />
  </label>
</template>

<style scoped>
.switch-toggle {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
  cursor: pointer;
  justify-self: start;
}

.switch-toggle--disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.switch-toggle-input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}

.switch-toggle-track {
  width: 40px;
  height: 22px;
  border-radius: 11px;
  background: var(--switch-track-off-bg);
  transition: background 0.18s ease;
  position: relative;
}

.switch-toggle-track::after {
  content: "";
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  transition: transform 0.18s ease;
}

.switch-toggle-input:checked + .switch-toggle-track {
  background: var(--accent);
}

.switch-toggle-input:checked + .switch-toggle-track::after {
  transform: translateX(18px);
}

.switch-toggle--sm .switch-toggle-track {
  width: 32px;
  height: 18px;
  border-radius: 9px;
}

.switch-toggle--sm .switch-toggle-track::after {
  top: 3px;
  left: 3px;
  width: 12px;
  height: 12px;
}

.switch-toggle--sm .switch-toggle-input:checked + .switch-toggle-track::after {
  transform: translateX(14px);
}
</style>
