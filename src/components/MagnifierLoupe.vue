<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import { maskModalOpen } from "../lib/maskModal";
import { comic, magnifierOn, pageIndex, scrollOffset, settings } from "../reader";

/** Chrome / menus that can sit above the stage (windowed dropdowns or overlay panels). */
const CHROME_HIT =
  ".menubar, .dock, .sidebar-pane, .resizer, .overlay-sb, .bar-menu";

const LOUPE_SIZE = 200;
const ZOOM = 2;
const RADIUS = LOUPE_SIZE / 2;
/** Screen-space half-size of source content visible inside the loupe. */
const SAMPLE_RADIUS = RADIUS / ZOOM;

const visible = ref(false);
const loupeStyle = ref<Record<string, string>>({});
const clipEl = ref<HTMLElement | null>(null);
const canvasEl = ref<HTMLCanvasElement | null>(null);
const ocrHostEl = ref<HTMLElement | null>(null);

let pointerX = 0;
let pointerY = 0;
let hasPointer = false;
let raf = 0;

type BitmapSource = HTMLImageElement | HTMLCanvasElement;

type PageHit = {
  media: HTMLElement;
  mediaRect: DOMRect;
  source: BitmapSource;
  bmpRect: { left: number; top: number; width: number; height: number };
  natW: number;
  natH: number;
  ocrPlane: HTMLElement | null;
};

function clipBounds(): DOMRect | null {
  const el = clipEl.value;
  return el ? el.getBoundingClientRect() : null;
}

function pointerInClip(clientX: number, clientY: number, clip: DOMRect) {
  return (
    clientX >= clip.left
    && clientX <= clip.right
    && clientY >= clip.top
    && clientY <= clip.bottom
  );
}

/** True when the topmost hit is chrome/menu (including menus that overhang the stage). */
function pointerOverChrome(clientX: number, clientY: number) {
  const hit = document.elementFromPoint(clientX, clientY);
  if (!(hit instanceof Element)) return false;
  return Boolean(hit.closest(CHROME_HIT));
}

function intersectsSample(rect: { left: number; top: number; width: number; height: number }, cx: number, cy: number, radius: number) {
  const right = rect.left + rect.width;
  const bottom = rect.top + rect.height;
  const nearestX = Math.max(rect.left, Math.min(cx, right));
  const nearestY = Math.max(rect.top, Math.min(cy, bottom));
  const dx = nearestX - cx;
  const dy = nearestY - cy;
  return dx * dx + dy * dy <= radius * radius;
}

/** Prefer full-res decode (neural overlay / base img) over display-sized canvas resample. */
function bestSource(media: HTMLElement): BitmapSource | null {
  const up = media.querySelector("img.page-bitmap.overlay.show");
  if (up instanceof HTMLImageElement && up.naturalWidth > 0) return up;
  const base = media.querySelector("img.page-bitmap.base");
  if (base instanceof HTMLImageElement && base.naturalWidth > 0) return base;
  const canvas = media.querySelector("canvas.page-bitmap.overlay.show");
  if (canvas instanceof HTMLCanvasElement && canvas.width > 0 && canvas.height > 0) return canvas;
  return null;
}

function sourceNaturalSize(source: BitmapSource): { w: number; h: number } {
  if (source instanceof HTMLImageElement) {
    return { w: source.naturalWidth, h: source.naturalHeight };
  }
  return { w: source.width, h: source.height };
}

/** Painted bitmap box in screen coords (handles object-fit contain). */
function bitmapScreenRect(
  mediaRect: DOMRect,
  natW: number,
  natH: number,
  bitmapEl: Element | null,
): { left: number; top: number; width: number; height: number } {
  if (natW < 1 || natH < 1) {
    return { left: mediaRect.left, top: mediaRect.top, width: mediaRect.width, height: mediaRect.height };
  }
  const fit = bitmapEl ? getComputedStyle(bitmapEl).objectFit : "fill";
  if (fit !== "contain") {
    return { left: mediaRect.left, top: mediaRect.top, width: mediaRect.width, height: mediaRect.height };
  }
  const scale = Math.min(mediaRect.width / natW, mediaRect.height / natH);
  const dw = natW * scale;
  const dh = natH * scale;
  const pos = bitmapEl ? getComputedStyle(bitmapEl).objectPosition : "center center";
  // Scroll strips use "center top"; page stage uses fill (handled above).
  const topAligned = /\btop\b/i.test(pos);
  const dx = (mediaRect.width - dw) / 2;
  const dy = topAligned ? 0 : (mediaRect.height - dh) / 2;
  return {
    left: mediaRect.left + dx,
    top: mediaRect.top + dy,
    width: dw,
    height: dh,
  };
}

function collectPages(cx: number, cy: number): PageHit[] {
  const nodes = document.querySelectorAll(".page-media.show, .page-media:not(.pending)");
  const hits: PageHit[] = [];
  const pad = SAMPLE_RADIUS + 2;
  for (const node of nodes) {
    if (!(node instanceof HTMLElement)) continue;
    if (node.closest(".magnifier-loupe")) continue;
    const mediaRect = node.getBoundingClientRect();
    if (mediaRect.width < 2 || mediaRect.height < 2) continue;
    if (!intersectsSample(mediaRect, cx, cy, pad)) continue;
    const source = bestSource(node);
    if (!source) continue;
    const { w: natW, h: natH } = sourceNaturalSize(source);
    if (natW < 1 || natH < 1) continue;
    const bitmapEl =
      source instanceof HTMLElement
        ? source
        : node.querySelector("img.page-bitmap.base");
    const bmpRect = bitmapScreenRect(mediaRect, natW, natH, bitmapEl);
    if (!intersectsSample(bmpRect, cx, cy, pad) && !intersectsSample(mediaRect, cx, cy, pad)) {
      continue;
    }
    const ocrPlane =
      node.querySelector(".page-media-ocr-plane") instanceof HTMLElement
        ? (node.querySelector(".page-media-ocr-plane") as HTMLElement)
        : null;
    hits.push({ media: node, mediaRect, source, bmpRect, natW, natH, ocrPlane });
  }
  return hits;
}

function drawHiRes(pages: PageHit[], focusX: number, focusY: number) {
  const canvas = canvasEl.value;
  if (!canvas) return;
  const dpr = Math.max(1, window.devicePixelRatio || 1);
  const pixel = Math.round(LOUPE_SIZE * dpr);
  if (canvas.width !== pixel || canvas.height !== pixel) {
    canvas.width = pixel;
    canvas.height = pixel;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.fillStyle = settings.backgroundColor;
  ctx.fillRect(0, 0, LOUPE_SIZE, LOUPE_SIZE);

  const sampleLeft = focusX - SAMPLE_RADIUS;
  const sampleTop = focusY - SAMPLE_RADIUS;
  const sampleRight = focusX + SAMPLE_RADIUS;
  const sampleBottom = focusY + SAMPLE_RADIUS;

  for (const page of pages) {
    const { source, bmpRect, natW, natH } = page;
    const ix0 = Math.max(sampleLeft, bmpRect.left);
    const iy0 = Math.max(sampleTop, bmpRect.top);
    const ix1 = Math.min(sampleRight, bmpRect.left + bmpRect.width);
    const iy1 = Math.min(sampleBottom, bmpRect.top + bmpRect.height);
    if (ix1 <= ix0 || iy1 <= iy0) continue;

    const sx = ((ix0 - bmpRect.left) / bmpRect.width) * natW;
    const sy = ((iy0 - bmpRect.top) / bmpRect.height) * natH;
    const sw = ((ix1 - ix0) / bmpRect.width) * natW;
    const sh = ((iy1 - iy0) / bmpRect.height) * natH;
    if (sw < 0.5 || sh < 0.5) continue;

    const dx = (ix0 - sampleLeft) * ZOOM;
    const dy = (iy0 - sampleTop) * ZOOM;
    const dw = (ix1 - ix0) * ZOOM;
    const dh = (iy1 - iy0) * ZOOM;
    try {
      ctx.drawImage(source, sx, sy, sw, sh, dx, dy, dw, dh);
    } catch {
      /* tainted */
    }
  }
}

function syncOcr(pages: PageHit[], focusX: number, focusY: number) {
  const host = ocrHostEl.value;
  if (!host) return;
  host.replaceChildren();
  for (const page of pages) {
    if (!page.ocrPlane) continue;
    const overlay = page.ocrPlane.querySelector(".ocr-overlay");
    if (!(overlay instanceof HTMLElement)) continue;
    const ocrRect = page.ocrPlane.getBoundingClientRect();
    if (ocrRect.width < 2 || ocrRect.height < 2) continue;

    const clone = overlay.cloneNode(true) as HTMLElement;
    clone.style.position = "absolute";
    clone.style.left = "0";
    clone.style.top = "0";
    clone.style.width = `${ocrRect.width}px`;
    clone.style.height = `${ocrRect.height}px`;
    clone.style.margin = "0";
    clone.style.transformOrigin = "0 0";
    clone.style.transform = `translate(${(ocrRect.left - focusX) * ZOOM + RADIUS}px, ${(ocrRect.top - focusY) * ZOOM + RADIUS}px) scale(${ZOOM})`;
    clone.style.pointerEvents = "none";
    host.appendChild(clone);
  }
}

function paint() {
  raf = 0;
  if (!magnifierOn.value || !hasPointer || maskModalOpen.value) {
    visible.value = false;
    return;
  }

  const clip = clipBounds();
  if (!clip || clip.width < 2 || clip.height < 2) {
    visible.value = false;
    return;
  }

  if (!pointerInClip(pointerX, pointerY, clip) || pointerOverChrome(pointerX, pointerY)) {
    visible.value = false;
    ocrHostEl.value?.replaceChildren();
    return;
  }

  const focusX = pointerX;
  const focusY = pointerY;
  const pages = collectPages(focusX, focusY);
  drawHiRes(pages, focusX, focusY);
  syncOcr(pages, focusX, focusY);

  loupeStyle.value = {
    left: `${focusX - RADIUS - clip.left}px`,
    top: `${focusY - RADIUS - clip.top}px`,
    width: `${LOUPE_SIZE}px`,
    height: `${LOUPE_SIZE}px`,
    background: settings.backgroundColor,
  };
  visible.value = true;
}

function schedulePaint() {
  if (raf) return;
  raf = window.requestAnimationFrame(paint);
}

function onPointerMove(event: PointerEvent) {
  pointerX = event.clientX;
  pointerY = event.clientY;
  hasPointer = true;
  if (magnifierOn.value) schedulePaint();
}

/** Leaving the window — no more stage pointer. */
function onPointerLeaveDocument() {
  if (!hasPointer) return;
  hasPointer = false;
  clearLoupe();
}

/** Scroll / glide / wheel move the page under a still pointer — refresh the loupe. */
function onStageScroll() {
  if (magnifierOn.value && hasPointer && !maskModalOpen.value) schedulePaint();
}

function refreshLoupe() {
  if (!magnifierOn.value || !hasPointer || maskModalOpen.value) return;
  schedulePaint();
  // Page swaps / layout settle one frame later.
  void nextTick(() => schedulePaint());
}

function clearLoupe() {
  visible.value = false;
  ocrHostEl.value?.replaceChildren();
}

watch(magnifierOn, (on) => {
  if (!on) {
    clearLoupe();
    return;
  }
  refreshLoupe();
});

watch(maskModalOpen, (open) => {
  if (open) clearLoupe();
  else refreshLoupe();
});

watch(comic, (book) => {
  if (!book && magnifierOn.value) magnifierOn.value = false;
});

watch(
  () => [comic.value?.sessionId, comic.value?.pages.length, pageIndex.value, scrollOffset.value] as const,
  () => refreshLoupe(),
);

window.addEventListener("pointermove", onPointerMove, { passive: true });
document.documentElement.addEventListener("pointerleave", onPointerLeaveDocument);
// Capture: scroll-view / page-stage (and glide RAF) don't bubble scroll.
document.addEventListener("scroll", onStageScroll, { capture: true, passive: true });

onBeforeUnmount(() => {
  window.removeEventListener("pointermove", onPointerMove);
  document.documentElement.removeEventListener("pointerleave", onPointerLeaveDocument);
  document.removeEventListener("scroll", onStageScroll, true);
  if (raf) window.cancelAnimationFrame(raf);
  clearLoupe();
});
</script>

<template>
  <div
    v-show="magnifierOn"
    ref="clipEl"
    class="magnifier-clip"
    aria-hidden="true"
  >
    <div
      v-show="visible"
      class="magnifier-loupe"
      :style="loupeStyle"
    >
      <canvas ref="canvasEl" class="magnifier-loupe-canvas" />
      <div ref="ocrHostEl" class="magnifier-ocr-host" />
    </div>
  </div>
</template>

<style scoped>
.magnifier-loupe {
  position: absolute;
  border-radius: 50%;
  overflow: hidden;
  pointer-events: none;
  box-sizing: border-box;
  border: 2px solid color-mix(in srgb, var(--accent) 85%, white);
}

.magnifier-loupe-canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
  pointer-events: none;
}

.magnifier-ocr-host {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
}

.magnifier-ocr-host :deep(*) {
  pointer-events: none !important;
}
</style>
