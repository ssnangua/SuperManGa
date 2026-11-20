<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";

const modelValue = defineModel<string>({ default: "#000000" });

type Hsv = { h: number; s: number; v: number };

function normalizeHex(value: string): string | null {
  const raw = value.trim().replace(/^#/, "");
  if (/^[0-9a-fA-F]{3}$/.test(raw)) {
    return `#${raw[0]}${raw[0]}${raw[1]}${raw[1]}${raw[2]}${raw[2]}`.toLowerCase();
  }
  if (/^[0-9a-fA-F]{6}$/.test(raw)) return `#${raw.toLowerCase()}`;
  return null;
}

function hexToRgb(hex: string) {
  const raw = (normalizeHex(hex) ?? "#000000").slice(1);
  return {
    r: Number.parseInt(raw.slice(0, 2), 16),
    g: Number.parseInt(raw.slice(2, 4), 16),
    b: Number.parseInt(raw.slice(4, 6), 16),
  };
}

function rgbToHex(r: number, g: number, b: number) {
  return `#${[r, g, b]
    .map((n) => Math.round(Math.min(255, Math.max(0, n))).toString(16).padStart(2, "0"))
    .join("")}`;
}

function rgbToHsv(r: number, g: number, b: number): Hsv {
  const rr = r / 255;
  const gg = g / 255;
  const bb = b / 255;
  const max = Math.max(rr, gg, bb);
  const min = Math.min(rr, gg, bb);
  const d = max - min;
  let h = 0;
  if (d) {
    if (max === rr) h = ((gg - bb) / d + (gg < bb ? 6 : 0)) * 60;
    else if (max === gg) h = ((bb - rr) / d + 2) * 60;
    else h = ((rr - gg) / d + 4) * 60;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max };
}

function hsvFromHex(value: string): Hsv {
  const { r, g, b } = hexToRgb(value);
  return rgbToHsv(r, g, b);
}

function hsvToRgb(h: number, s: number, v: number) {
  const hue = ((h % 360) + 360) % 360;
  const c = v * s;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = v - c;
  let r = 0;
  let g = 0;
  let b = 0;
  if (hue < 60) {
    r = c;
    g = x;
  } else if (hue < 120) {
    r = x;
    g = c;
  } else if (hue < 180) {
    g = c;
    b = x;
  } else if (hue < 240) {
    g = x;
    b = c;
  } else if (hue < 300) {
    r = x;
    b = c;
  } else {
    r = c;
    b = x;
  }
  return {
    r: (r + m) * 255,
    g: (g + m) * 255,
    b: (b + m) * 255,
  };
}

function hsvToHex(hsv: Hsv) {
  const rgb = hsvToRgb(hsv.h, hsv.s, hsv.v);
  return rgbToHex(rgb.r, rgb.g, rgb.b);
}

function clamp01(n: number) {
  return Math.min(1, Math.max(0, n));
}

const hex = computed({
  get: () => normalizeHex(modelValue.value) ?? "#000000",
  set: (value: string) => {
    const next = normalizeHex(value);
    if (next) modelValue.value = next;
  },
});

const draft = ref(hex.value.toUpperCase());
const open = ref(false);
const hsv = ref<Hsv>(hsvFromHex(modelValue.value));
const swatchRef = ref<HTMLButtonElement | null>(null);
const popoverRef = ref<HTMLElement | null>(null);
const boardRef = ref<HTMLElement | null>(null);
const popoverStyle = ref<Record<string, string>>({});

watch(hex, (value) => {
  draft.value = value.toUpperCase();
  if (!dragging) hsv.value = hsvFromHex(value);
});

let dragging = false;

function syncHsvFromHex() {
  hsv.value = hsvFromHex(hex.value);
}

function applyHsv() {
  hex.value = hsvToHex(hsv.value);
}

function placePopover() {
  const swatch = swatchRef.value;
  if (!swatch) return;
  const rect = swatch.getBoundingClientRect();
  const width = 220;
  const height = 228;
  const pad = 8;
  let left = rect.right - width;
  let top = rect.bottom + 6;
  left = Math.min(Math.max(pad, left), window.innerWidth - width - pad);
  if (top + height > window.innerHeight - pad) {
    top = Math.max(pad, rect.top - height - 6);
  }
  popoverStyle.value = {
    left: `${Math.round(left)}px`,
    top: `${Math.round(top)}px`,
  };
}

async function togglePicker() {
  if (open.value) {
    closePicker();
    return;
  }
  syncHsvFromHex();
  open.value = true;
  await nextTick();
  placePopover();
  window.addEventListener("pointerdown", onPointerDownOutside, true);
  window.addEventListener("keydown", onKeydown, true);
  window.addEventListener("resize", placePopover);
  window.addEventListener("blur", closePicker);
}

function closePicker() {
  if (!open.value) return;
  open.value = false;
  window.removeEventListener("pointerdown", onPointerDownOutside, true);
  window.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("resize", placePopover);
  window.removeEventListener("blur", closePicker);
}

function onPointerDownOutside(event: PointerEvent) {
  const target = event.target;
  if (!(target instanceof Node)) return;
  if (swatchRef.value?.contains(target) || popoverRef.value?.contains(target)) return;
  closePicker();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.stopPropagation();
    closePicker();
  }
}

function setFromBoard(event: PointerEvent) {
  const board = boardRef.value;
  if (!board) return;
  const rect = board.getBoundingClientRect();
  hsv.value = {
    ...hsv.value,
    s: clamp01((event.clientX - rect.left) / rect.width),
    v: clamp01(1 - (event.clientY - rect.top) / rect.height),
  };
  applyHsv();
}

function onBoardPointerDown(event: PointerEvent) {
  dragging = true;
  boardRef.value?.setPointerCapture(event.pointerId);
  setFromBoard(event);
}

function onBoardPointerMove(event: PointerEvent) {
  if (!dragging) return;
  setFromBoard(event);
}

function onBoardPointerUp() {
  dragging = false;
}

function onHueInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  hsv.value = { ...hsv.value, h: value };
  applyHsv();
}

function commitDraft() {
  const next = normalizeHex(draft.value);
  if (!next) {
    draft.value = hex.value.toUpperCase();
    return;
  }
  hex.value = next;
  draft.value = next.toUpperCase();
  syncHsvFromHex();
}

onBeforeUnmount(() => {
  closePicker();
});

const hueColor = computed(() => hsvToHex({ h: hsv.value.h, s: 1, v: 1 }));
const thumbStyle = computed(() => ({
  left: `${hsv.value.s * 100}%`,
  top: `${(1 - hsv.value.v) * 100}%`,
}));
</script>

<template>
  <div class="hex-color-field">
    <button
      ref="swatchRef"
      type="button"
      class="hex-color-swatch"
      :style="{ backgroundColor: hex }"
      :aria-expanded="open"
      aria-label="HEX"
      @click="togglePicker"
    />
    <input
      v-model="draft"
      class="hex-color-text"
      type="text"
      spellcheck="false"
      maxlength="7"
      aria-label="HEX"
      @change="commitDraft"
      @keydown.enter.prevent="commitDraft"
    />
    <Teleport to="body">
      <div
        v-if="open"
        ref="popoverRef"
        class="hex-picker"
        :style="popoverStyle"
        role="dialog"
        aria-label="HEX"
      >
        <div
          ref="boardRef"
          class="hex-picker-board"
          :style="{ '--hue': hueColor }"
          @pointerdown="onBoardPointerDown"
          @pointermove="onBoardPointerMove"
          @pointerup="onBoardPointerUp"
          @pointercancel="onBoardPointerUp"
        >
          <span class="hex-picker-thumb" :style="thumbStyle" />
        </div>
        <input
          class="hex-picker-hue"
          type="range"
          min="0"
          max="360"
          step="1"
          :value="hsv.h"
          aria-label="H"
          @input="onHueInput"
        />
        <input
          v-model="draft"
          class="hex-picker-hex"
          type="text"
          spellcheck="false"
          maxlength="7"
          aria-label="HEX"
          @change="commitDraft"
          @keydown.enter.prevent="commitDraft"
        />
      </div>
    </Teleport>
  </div>
</template>

<style>
/* Joined swatch + hex input (mirror of .input-button-group, swatch on the left). */
.hex-color-field {
  display: inline-flex;
  align-items: stretch;
  flex: none;
}

.app-modal-panel.settings-dialog .hex-color-text {
  width: 96px;
  font-variant-numeric: tabular-nums;
  text-transform: uppercase;
  position: relative;
  z-index: 0;
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
}

.app-modal-panel.settings-dialog .hex-color-text:focus,
.app-modal-panel.settings-dialog .hex-color-text:focus-visible {
  z-index: 1;
}

.app-modal-panel button.hex-color-swatch {
  position: relative;
  z-index: 0;
  display: block;
  box-sizing: border-box;
  width: 32px;
  flex: none;
  align-self: stretch;
  margin: 0;
  margin-right: -1px;
  padding: 0;
  justify-content: stretch;
  gap: 0;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
  overflow: hidden;
  cursor: pointer;
}

.app-modal-panel button.hex-color-swatch:hover:not(:disabled) {
  color: inherit;
  border-color: var(--accent);
  z-index: 1;
}

.hex-color-field:has(.hex-color-text:focus) button.hex-color-swatch,
.hex-color-field:has(.hex-color-text:focus-visible) button.hex-color-swatch,
.hex-color-field:has(.hex-color-text:focus) button.hex-color-swatch:hover:not(:disabled),
.hex-color-field:has(.hex-color-text:focus-visible) button.hex-color-swatch:hover:not(:disabled) {
  border-right-color: transparent;
}

.hex-picker {
  position: fixed;
  z-index: var(--z-popover);
  box-sizing: border-box;
  width: 220px;
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  box-shadow: var(--menu-shadow);
  color: var(--ink);
}

.hex-picker-board {
  position: relative;
  width: 100%;
  height: 140px;
  border-radius: var(--radius);
  background:
    linear-gradient(to top, #000, transparent),
    linear-gradient(to right, #fff, var(--hue));
  cursor: crosshair;
  touch-action: none;
}

.hex-picker-thumb {
  position: absolute;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border: 2px solid #fff;
  border-radius: 50%;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.45);
  pointer-events: none;
}

.hex-picker-hue {
  width: 100%;
  margin: 10px 0 8px;
  height: 12px;
  padding: 0;
  appearance: none;
  background: linear-gradient(
    to right,
    #ff0000,
    #ffff00,
    #00ff00,
    #00ffff,
    #0000ff,
    #ff00ff,
    #ff0000
  );
  border-radius: 99px;
  accent-color: #fff;
  cursor: pointer;
}

.hex-picker-hue::-webkit-slider-thumb {
  appearance: none;
  width: 14px;
  height: 14px;
  border: 2px solid #fff;
  border-radius: 50%;
  background: transparent;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.45);
}

.hex-picker-hex {
  width: 100%;
  box-sizing: border-box;
  font-variant-numeric: tabular-nums;
  text-transform: uppercase;
  text-align: center;
}
</style>
