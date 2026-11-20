<script setup lang="ts">
import { computed } from "vue";
import { hideAppTip, showAppTip } from "../lib/appTip";

/** Match `.range-slider::-webkit-slider-thumb` width — browser insets track by half thumb. */
const THUMB_SIZE = 12;

const props = defineProps<{
  min: number;
  max: number;
  step?: number;
  modelValue: number;
  tipSuffix?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: number];
  "scrub-start": [];
  "scrub-end": [];
}>();

const fill = computed(() => {
  const span = props.max - props.min;
  if (!(span > 0)) return "0%";
  const ratio = Math.min(1, Math.max(0, (props.modelValue - props.min) / span));
  return `${ratio * 100}%`;
});

function tipText(value: number) {
  return `${value}${props.tipSuffix ?? ""}`;
}

function snap(value: number) {
  const step = props.step ?? 1;
  const span = props.max - props.min;
  if (!(span > 0) || !(step > 0)) return props.min;
  const stepped = Math.round((value - props.min) / step) * step + props.min;
  const rounded = Number(stepped.toFixed(6));
  return Math.min(props.max, Math.max(props.min, rounded));
}

/** Map pointer X → stepped value using the same inset track the browser uses. */
function valueFromPointer(event: PointerEvent) {
  const target = event.currentTarget;
  if (!(target instanceof HTMLElement)) return props.modelValue;
  const rect = target.getBoundingClientRect();
  const span = props.max - props.min;
  const usable = Math.max(1, rect.width - THUMB_SIZE);
  if (!(span > 0)) return props.min;
  const ratio = (event.clientX - rect.left - THUMB_SIZE / 2) / usable;
  return snap(props.min + Math.min(1, Math.max(0, ratio)) * span);
}

/** Tip sits above the thumb for `value`, not under a possibly-ahead finger. */
function placeTipForValue(input: HTMLElement, value: number) {
  const rect = input.getBoundingClientRect();
  const span = props.max - props.min;
  const usable = Math.max(1, rect.width - THUMB_SIZE);
  const ratio = span > 0 ? (value - props.min) / span : 0;
  const x = rect.left + THUMB_SIZE / 2 + usable * Math.min(1, Math.max(0, ratio));
  showAppTip({
    text: tipText(value),
    x,
    y: rect.top,
    placement: "above",
    gap: 8,
  });
}

function placeTip(event: PointerEvent) {
  const target = event.currentTarget;
  if (!(target instanceof HTMLElement)) return;
  // While dragging, @input owns the tip so pointer estimates can't fight the stepped value.
  if (event.buttons !== 0) return;
  placeTipForValue(target, valueFromPointer(event));
}

function onInput(event: Event) {
  const input = event.target;
  if (!(input instanceof HTMLInputElement)) return;
  const value = snap(Number(input.value));
  emit("update:modelValue", value);
  placeTipForValue(input, value);
}

function onScrubStart() {
  emit("scrub-start");
}

function onScrubEnd() {
  emit("scrub-end");
}
</script>

<template>
  <input
    class="range-slider"
    type="range"
    :min="min"
    :max="max"
    :step="step ?? 1"
    :value="modelValue"
    :style="{ '--progress': fill }"
    @pointerenter="placeTip"
    @pointermove="placeTip"
    @pointerleave="hideAppTip"
    @pointerdown="onScrubStart"
    @pointerup="onScrubEnd"
    @pointercancel="onScrubEnd"
    @focus="onScrubStart"
    @blur="onScrubEnd"
    @input="onInput"
  />
</template>

<style>
.range-slider {
  -webkit-appearance: none;
  appearance: none;
  height: 12px;
  margin: 0;
  background: transparent;
  cursor: pointer;
}

.range-slider::-webkit-slider-runnable-track {
  height: 6px;
  border: none;
  border-radius: 999px;
  background: linear-gradient(
    to right,
    var(--accent) var(--progress, 0%),
    var(--seek) var(--progress, 0%)
  );
}

.range-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  margin-top: -3px;
  border: none;
  border-radius: 50%;
  background: var(--accent);
}

.range-slider::-moz-range-track {
  height: 6px;
  border: none;
  border-radius: 999px;
  background: var(--seek);
}

.range-slider::-moz-range-progress {
  height: 6px;
  border-radius: 999px;
  background: var(--accent);
}

.range-slider::-moz-range-thumb {
  width: 12px;
  height: 12px;
  border: none;
  border-radius: 50%;
  background: var(--accent);
}
</style>
