<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { OcrRegion } from "../lib/ocr/types";
import { settings } from "../reader";

const props = withDefaults(
  defineProps<{
    regions: OcrRegion[];
    /** Alt held: Saber-style yellow OCR/translation frames + index. */
    showBounds?: boolean;
  }>(),
  { showBounds: false },
);

/** Drop absurd boxes so a bad OCR result cannot cover the whole page. */
function sane(region: OcrRegion) {
  const { x, y, w, h } = region;
  if (![x, y, w, h].every((n) => Number.isFinite(n))) return false;
  if (w < 0.004 || h < 0.003) return false;
  if (h > 0.55 || w * h > 0.18) return false;
  if (w > 0.85 && h > 0.12) return false;
  if (x < 0 || y < 0 || x + w > 1.02 || y + h > 1.02) return false;
  return Boolean(region.translation?.trim());
}

/** Debug frames: show any region with OCR/translation text (incl. filtered ones). */
function debugable(region: OcrRegion) {
  const { x, y, w, h } = region;
  if (![x, y, w, h].every((n) => Number.isFinite(n))) return false;
  if (w < 0.002 || h < 0.002) return false;
  if (x < -0.02 || y < -0.02 || x + w > 1.05 || y + h > 1.05) return false;
  return Boolean(region.text?.trim() || region.translation?.trim());
}

const visible = computed(() => props.regions.filter(sane));
const debugRegions = computed(() =>
  props.showBounds ? props.regions.filter(debugable) : [],
);

const style = computed(() => settings.translation);

const rootEl = ref<HTMLElement | null>(null);
const fittedSize = ref<Record<string, number>>({});
let fitRaf = 0;
let ro: ResizeObserver | null = null;

/** Match Saber `calculate_auto_font_size` defaults (allow smaller for dense CJK). */
const MIN_FONT_PX = 8;
const MAX_FONT_PX = 80;
/** Saber: avg_char_width = mid * c_w */
const CHAR_WIDTH_RATIO = 1.0;

function linePitchRatio(): number {
  const n = style.value.textLineSpacing;
  return typeof n === "number" && Number.isFinite(n) && n > 0 ? n : 1;
}

/** Resolve vertical vs horizontal from settings + detector textline vote. */
function isVerticalRegion(region: OcrRegion): boolean {
  const dir = style.value.textDirection;
  if (dir === "vertical") return true;
  if (dir === "horizontal") return false;
  // auto: prefer Saber textline long-edge majority; AABB h>w only as fallback.
  if (typeof region.vertical === "boolean") return region.vertical;
  return region.h > region.w;
}

function parseHexLum(color: string | undefined): number | null {
  if (!color) return null;
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color.trim());
  if (!m) return null;
  let hex = m[1];
  if (hex.length === 3) {
    hex = hex
      .split("")
      .map((c) => c + c)
      .join("");
  }
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function textColorFor(region: OcrRegion): string {
  if (!style.value.textAutoColor) {
    return style.value.textColor?.trim() || "#000000";
  }
  return region.fgColor || "#000000";
}

/** Stroke: prefer 48px model bg (MIT/Saber), else contrast from fg/fill. */
function strokeColorFor(region: OcrRegion, fg: string): string {
  if (style.value.textAutoColor && region.strokeColor) {
    return region.strokeColor;
  }
  const fgLum = parseHexLum(fg);
  if (fgLum != null) {
    return fgLum >= 160 ? "#111111" : "#FFFFFF";
  }
  const bgLum = parseHexLum(region.bgColor);
  if (bgLum != null) {
    return bgLum >= 160 ? "#FFFFFF" : "#111111";
  }
  return "#FFFFFF";
}

function strokeWidthPx(fontPx: number, fg: string, stroke: string): number {
  if (!style.value.textStrokeEnabled) return 0;
  // MIT: drop stroke when ink ≈ outline (avoids muddy grey halos).
  const fgLum = parseHexLum(fg);
  const stLum = parseHexLum(stroke);
  if (fgLum != null && stLum != null && Math.abs(fgLum - stLum) <= 20) {
    return 0;
  }
  if (style.value.textStrokeAutoScale) {
    return Math.max(1, Math.min(2.5, fontPx * 0.08));
  }
  const w = style.value.textStrokeWidth;
  return typeof w === "number" && Number.isFinite(w) ? Math.max(1, Math.round(w)) : 3;
}

/**
 * Saber `calculate_auto_font_size` — keep translation inside the detection box.
 * Vertical: characters stack in columns (CSS `writing-mode: vertical-rl`).
 */
function saberFontSize(
  text: string,
  boxW: number,
  boxH: number,
  vertical: boolean,
  linePitch: number,
): number {
  const paragraphs = text.split("\n").map((p) => p.trim()).filter(Boolean);
  const lengths = paragraphs.length
    ? paragraphs.map((p) => [...p].length)
    : [[...text.replace(/\s+/g, "")].length];
  const n = lengths.reduce((a, b) => a + b, 0);
  if (n <= 0 || boxW <= 0 || boxH <= 0) return MIN_FONT_PX;

  let lo = MIN_FONT_PX;
  let hi = Math.min(MAX_FONT_PX, Math.floor(vertical ? boxW : boxH));
  let best = lo;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (mid <= 0) break;
    let fits = false;
    if (vertical) {
      const charsPerCol = Math.max(1, Math.floor(boxH / mid));
      let cols = 0;
      for (const len of lengths) {
        cols += Math.max(1, Math.ceil(len / charsPerCol));
      }
      fits = cols * mid * linePitch <= boxW;
    } else {
      const charW = mid * CHAR_WIDTH_RATIO;
      const charsPerLine = Math.max(1, Math.floor(boxW / charW));
      let lines = 0;
      for (const len of lengths) {
        lines += Math.max(1, Math.ceil(len / charsPerLine));
      }
      fits = lines * mid * linePitch <= boxH;
    }
    if (fits) {
      best = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return best;
}

function fitsDom(textEl: HTMLElement, availW: number, availH: number) {
  return (
    textEl.scrollHeight <= availH + 0.5 && textEl.scrollWidth <= availW + 0.5
  );
}

function fitAll() {
  const root = rootEl.value;
  if (!root) return;
  if (!style.value.textAutoFontSize) {
    if (Object.keys(fittedSize.value).length) fittedSize.value = {};
    return;
  }
  const pitch = linePitchRatio();
  const next: Record<string, number> = { ...fittedSize.value };
  let changed = false;
  for (const layer of root.querySelectorAll<HTMLElement>(".ocr-text")) {
    const id = layer.dataset.regionId;
    const textEl = layer.querySelector<HTMLElement>(".ocr-dst");
    if (!id || !textEl) continue;
    const boxW = layer.clientWidth;
    const boxH = layer.clientHeight;
    if (boxW < 4 || boxH < 4) continue;

    const raw = (textEl.textContent || "").trim();
    const vertical = layer.classList.contains("is-vertical");
    let best = saberFontSize(raw, boxW, boxH, vertical, pitch);

    const prevSize = textEl.style.fontSize;
    const prevStroke = textEl.style.webkitTextStrokeWidth;
    textEl.style.webkitTextStrokeWidth = "0px";
    textEl.style.fontSize = `${best}px`;
    if (!fitsDom(textEl, boxW, boxH)) {
      let lo = MIN_FONT_PX;
      let hi = best;
      best = lo;
      for (let i = 0; i < 14; i++) {
        const mid = (lo + hi) / 2;
        textEl.style.fontSize = `${mid}px`;
        if (fitsDom(textEl, boxW, boxH)) {
          best = mid;
          lo = mid;
        } else {
          hi = mid;
        }
      }
    }
    textEl.style.fontSize = `${best}px`;
    const region = props.regions.find((r) => r.id === id);
    const fg = region ? textColorFor(region) : "#000000";
    const stroke = region ? strokeColorFor(region, fg) : "#FFFFFF";
    const strokeW = strokeWidthPx(best, fg, stroke);
    textEl.style.webkitTextStrokeWidth = strokeW > 0 ? `${strokeW}px` : "0px";
    if (strokeW > 0 && !fitsDom(textEl, boxW, boxH)) {
      let lo = MIN_FONT_PX;
      let hi = best;
      best = lo;
      for (let i = 0; i < 12; i++) {
        const mid = (lo + hi) / 2;
        const sw = strokeWidthPx(mid, fg, stroke);
        textEl.style.fontSize = `${mid}px`;
        textEl.style.webkitTextStrokeWidth = sw > 0 ? `${sw}px` : "0px";
        if (fitsDom(textEl, boxW, boxH)) {
          best = mid;
          lo = mid;
        } else {
          hi = mid;
        }
      }
    }

    textEl.style.fontSize = prevSize;
    textEl.style.webkitTextStrokeWidth = prevStroke;

    const rounded = Math.max(MIN_FONT_PX, Math.floor(best));
    if (next[id] !== rounded) {
      next[id] = rounded;
      changed = true;
    }
  }
  if (changed) fittedSize.value = next;
}

function scheduleFit() {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(() => {
    nextTick(fitAll);
  });
}

watch(visible, scheduleFit, { deep: true, immediate: true });
watch(
  () =>
    [
      style.value.textAutoFontSize,
      style.value.textFontSize,
      style.value.textFontFamily,
      style.value.textDirection,
      style.value.textLineSpacing,
      style.value.textAlign,
      style.value.textColor,
      style.value.textAutoColor,
      style.value.textStrokeEnabled,
      style.value.textStrokeWidth,
      style.value.textStrokeAutoScale,
    ].join("\0"),
  scheduleFit,
);

onMounted(() => {
  scheduleFit();
  if (rootEl.value) {
    ro = new ResizeObserver(scheduleFit);
    ro.observe(rootEl.value);
  }
});

onBeforeUnmount(() => {
  cancelAnimationFrame(fitRaf);
  ro?.disconnect();
});

function frameStyle(region: OcrRegion) {
  return {
    left: `${region.x * 100}%`,
    top: `${region.y * 100}%`,
    width: `${region.w * 100}%`,
    height: `${region.h * 100}%`,
  };
}

/** Layer 1 — Saber solid fill over original ink. */
function maskStyle(region: OcrRegion) {
  return {
    ...frameStyle(region),
    background: region.bgColor || "#f2efe6",
  };
}

function alignClass(): string {
  const a = style.value.textAlign;
  if (a === "center") return "align-center";
  if (a === "end") return "align-end";
  return "align-start";
}

/** Layer 2 — same rect as mask; font auto-fits or fixed size from settings. */
function textStyle(region: OcrRegion) {
  const auto = style.value.textAutoFontSize;
  const fixed = Math.max(1, Math.round(style.value.textFontSize || 26));
  const fitted = fittedSize.value[region.id];
  const fontPx = auto ? (fitted ?? fixed) : fixed;
  const fg = textColorFor(region);
  const stroke = strokeColorFor(region, fg);
  const strokeW = strokeWidthPx(fontPx, fg, stroke);
  const family = style.value.textFontFamily?.trim();
  return {
    ...frameStyle(region),
    color: fg,
    fontSize: `${fontPx}px`,
    fontFamily: family ? `"${family}", sans-serif` : undefined,
    lineHeight: String(linePitchRatio()),
    "--ocr-stroke": stroke,
    "--ocr-stroke-w": strokeW > 0 ? `${strokeW}px` : "0px",
  };
}
</script>

<template>
  <div ref="rootEl" class="ocr-overlay">
    <div class="ocr-mask-layer" aria-hidden="true">
      <div
        v-for="region in visible"
        :key="`mask-${region.id}`"
        class="ocr-mask"
        :style="maskStyle(region)"
      />
    </div>
    <div class="ocr-text-layer">
      <div
        v-for="region in visible"
        :key="`text-${region.id}`"
        class="ocr-text"
        :class="[
          alignClass(),
          { 'is-vertical': isVerticalRegion(region), 'no-stroke': !style.textStrokeEnabled },
        ]"
        :data-region-id="region.id"
        :style="textStyle(region)"
      >
        <p class="ocr-dst">{{ region.translation }}</p>
      </div>
    </div>
    <div v-if="debugRegions.length" class="ocr-debug-layer" aria-hidden="true">
      <div
        v-for="(region, i) in debugRegions"
        :key="`dbg-${region.id}`"
        class="ocr-debug-box"
        :style="frameStyle(region)"
      >
        <span class="ocr-debug-idx">{{ i + 1 }}</span>
      </div>
    </div>
  </div>
</template>

<style>
.ocr-overlay {
  position: absolute;
  inset: 0;
  z-index: 3;
  pointer-events: none;
  container-type: size;
}

/* Alt: Saber-style OCR/translation region frames */
.ocr-debug-layer {
  position: absolute;
  inset: 0;
  z-index: 4;
  pointer-events: none;
}

.ocr-debug-box {
  position: absolute;
  box-sizing: border-box;
  border: 2px solid #f5c518;
  background: color-mix(in srgb, #f5c518 12%, transparent);
}

.ocr-debug-idx {
  position: absolute;
  left: 0;
  top: 0;
  transform: translateY(-100%);
  min-width: 1.1em;
  padding: 0 4px;
  box-sizing: border-box;
  background: #111;
  color: #fff;
  font-size: 11px;
  font-weight: 700;
  line-height: 16px;
  font-variant-numeric: tabular-nums;
  text-align: center;
  border-radius: 1px 1px 0 0;
}

/* Layer 1: solid fill over OCR boxes (Saber solid inpaint / fillColor). */
.ocr-mask-layer {
  position: absolute;
  inset: 0;
  z-index: 1;
  pointer-events: none;
}

.ocr-mask {
  position: absolute;
  box-sizing: border-box;
  border: none;
  border-radius: 1px;
  background: #f2efe6;
}

/* Layer 2: translation above the fill — same rect as mask.
   Font auto-fit soft-constrains size to the box; slight glyph overflow is OK
   (Saber does not hard-clip to the detection frame). */
.ocr-text-layer {
  position: absolute;
  inset: 0;
  z-index: 2;
  pointer-events: none;
  overflow: visible;
}

.ocr-text {
  position: absolute;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1px 2px;
  overflow: visible;
  border: none;
  background: transparent;
  color: #1a1a1a;
  pointer-events: none;
  --ocr-stroke: #ffffff;
  --ocr-stroke-w: 2px;
}

.ocr-dst {
  margin: 0;
  width: 100%;
  max-width: 100%;
  max-height: 100%;
  padding: 0;
  color: inherit;
  font: inherit;
  font-weight: 600;
  line-height: inherit;
  text-align: center;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: anywhere;
  overflow: visible;
  box-sizing: border-box;
  /* Saber strokeEnabled (default white / ~3px scaled with size). */
  -webkit-text-stroke: var(--ocr-stroke-w) var(--ocr-stroke);
  paint-order: stroke fill;
}

.ocr-text.no-stroke .ocr-dst {
  -webkit-text-stroke: 0;
}

/* Multi-line align inside the text block; outer .ocr-text stays flex-centered in the box. */
.ocr-text.align-start .ocr-dst {
  text-align: left;
}
.ocr-text.align-center .ocr-dst {
  text-align: center;
}
.ocr-text.align-end .ocr-dst {
  text-align: right;
}

/* Saber auto textDirection: taller boxes use vertical-rl (JP manga columns).
   width:100% (not auto) so multi-column intrinsic width is not flex-centered
   into neighboring bubbles; overflow stays visible for slight frame bleed. */
.ocr-text.is-vertical .ocr-dst {
  writing-mode: vertical-rl;
  /* Keep CJK upright (Saber vertical draw); avoid sideways glyphs. */
  text-orientation: upright;
  width: 100%;
  min-width: 0;
  height: 100%;
  max-width: 100%;
  max-height: 100%;
}

/* Vertical: text-align maps to block-axis (top / center / bottom of the column run). */
.ocr-text.is-vertical.align-start .ocr-dst {
  text-align: start;
}
.ocr-text.is-vertical.align-center .ocr-dst {
  text-align: center;
}
.ocr-text.is-vertical.align-end .ocr-dst {
  text-align: end;
}
</style>
