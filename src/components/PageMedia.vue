<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  cssImageRendering,
  isAnime4kProfile,
  isNeuralProfile,
  needsCanvas2d,
  neuralAssetId,
  usesCssImage,
  type Anime4kProfile,
  type RenderProfile,
} from "../lib/render/profile";
import { Anime4kPresenter } from "../lib/render/anime4k";
import { imageToImageData, type ResampleKind } from "../lib/render/resample";
import { resampleImageDataAsync } from "../lib/render/resampleAsync";
import {
  NEURAL_PRIORITY_CURRENT,
  NEURAL_PRIORITY_NEXT,
  NEURAL_PRIORITY_PREV,
  applyNeuralSkip,
  bookFolderName,
  isDetachedImageError,
  isNeuralCancelled,
  neuralDiskKey,
  neuralDoneFlags,
  neuralResultUrl,
  neuralSkipFlags,
  prefetchNeuralUpscale,
  resolveNeuralOutputSize,
  runNeuralUpscale,
} from "../lib/render/neural";
import { comic, scrollResampleReady, settings } from "../reader";
import { ANIMATED_IMAGE_SKIP, isAnimatedImage, isAnimatedImageSkip } from "../lib/render/animated";
import { notifyError } from "../lib/appToast";

/** Settle window-resize redraws before expensive classic/neural work. */
const RESIZE_DEBOUNCE_MS = 160;

const props = defineProps<{
  src: string;
  profile: RenderProfile;
  index: number;
  alt?: string;
  /** page-stage fill vs scroll contain-top */
  objectFit?: "fill" | "contain-top";
  pending?: boolean;
  /** Neighbor page URLs for neural prefetch (current → next → prev). */
  prefetchNextSrc?: string;
  prefetchPrevSrc?: string;
  /** Explicit neighbor indexes (double-page spreads may skip ±1). */
  prefetchNextIndex?: number;
  prefetchPrevIndex?: number;
}>();

const emit = defineEmits<{
  load: [payload: { index: number; width: number; height: number }];
  error: [payload: { index: number }];
}>();

const root = ref<HTMLElement | null>(null);
const canvasEl = ref<HTMLCanvasElement | null>(null);
/** Natural page size for OCR plane alignment under object-fit contain. */
const naturalW = ref(0);
const naturalH = ref(0);
const mediaBox = ref({ width: 0, height: 0 });
/** Base <img> (and layout) ready. */
const ready = ref(false);
/** Canvas overlay has a drawn frame; until then the base <img> shows through. */
const overlayReady = ref(false);
/** Neural upscale in flight for this slot (per-image loading veil). */
const neuralBusy = ref(false);
const neuralUrlFailed = ref(false);
const upscaledReady = ref(false);
/** Animated GIF / WebP / APNG: keep native <img> playback instead of freezing a frame. */
const skipAlgorithm = ref(false);
let source: HTMLImageElement | ImageBitmap | null = null;
let sourceKey = "";
let anime4k: Anime4kPresenter | null = null;
let drawGen = 0;
let observer: ResizeObserver | null = null;
let retryTimer = 0;
let resizeTimer = 0;
/** True when the last classic paint skipped kernels due to scroll motion. */
let deferredClassicPreview = false;
/** True when Anime4K was skipped for a cheap drawImage during scroll. */
let deferredAnime4kPreview = false;
/** True when neural upscale was deferred until scroll settle. */
let deferredNeural = false;

function scrollDefersHeavy(): boolean {
  return settings.readingMode === "scroll" && !scrollResampleReady.value;
}

const mode = computed(() => {
  if (usesCssImage(props.profile) || skipAlgorithm.value) return "img" as const;
  if (isAnime4kProfile(props.profile)) return "anime4k" as const;
  return "canvas2d" as const;
});

const useOverlay = computed(() => mode.value !== "img");

const imgStyle = computed(() => ({
  // Fallback under canvas stays smooth; pixelated only when img mode is selected.
  imageRendering: (mode.value === "img"
    ? cssImageRendering(usesCssImage(props.profile) ? props.profile : "bilinear")
    : "auto") as "auto" | "pixelated",
}));

/** Map normalized OCR boxes onto the painted bitmap (Saber: coords vs scaled display). */
const ocrPlaneStyle = computed(() => {
  const nw = naturalW.value;
  const nh = naturalH.value;
  const cw = mediaBox.value.width;
  const ch = mediaBox.value.height;
  if (nw < 1 || nh < 1 || cw < 1 || ch < 1 || props.objectFit !== "contain-top") {
    return { left: "0", top: "0", width: "100%", height: "100%" };
  }
  const scale = Math.min(cw / nw, ch / nh);
  const dw = nw * scale;
  const dh = nh * scale;
  const dx = (cw - dw) / 2;
  return {
    left: `${dx}px`,
    top: "0px",
    width: `${dw}px`,
    height: `${dh}px`,
  };
});

function syncMediaBox() {
  const el = root.value;
  if (!el) return;
  const width = el.clientWidth;
  const height = el.clientHeight;
  if (mediaBox.value.width === width && mediaBox.value.height === height) return;
  mediaBox.value = { width, height };
}

const upscaledSrc = computed(() => {
  if (skipAlgorithm.value || neuralUrlFailed.value) return "";
  if (!isNeuralProfile(props.profile)) return "";
  if (neuralSkipFlags.value[props.index]) return "";
  if (!neuralDoneFlags.value[props.index]) return "";
  const assetId = neuralAssetId(props.profile);
  const key = pageDiskKey(props.index, assetId);
  return key ? neuralResultUrl(key) : "";
});

function classicKind(profile: RenderProfile): ResampleKind {
  if (profile === "bicubic") return "bicubic";
  if (profile === "lanczos") return "lanczos";
  if (profile === "averageSharpWeak") return "averageSharpWeak";
  if (profile === "averageSharpStrong") return "averageSharpStrong";
  return "average";
}

function sourceSize(image: HTMLImageElement | ImageBitmap) {
  if ("naturalWidth" in image) {
    return { width: image.naturalWidth, height: image.naturalHeight };
  }
  return { width: image.width, height: image.height };
}

function paintSize() {
  const el = root.value;
  if (!el) return { width: 0, height: 0, dpr: 1 };
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  const width = Math.max(0, Math.round(el.clientWidth * dpr));
  const height = Math.max(0, Math.round(el.clientHeight * dpr));
  return { width, height, dpr };
}

function scheduleRetry() {
  window.clearTimeout(retryTimer);
  retryTimer = window.setTimeout(() => {
    void redraw();
  }, 32);
}

function scheduleResizeRedraw() {
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => {
    if (useOverlay.value) void redraw();
  }, RESIZE_DEBOUNCE_MS);
}

/** Let the browser paint (e.g. close menus) before CPU-heavy classic/neural work. */
function afterPaint() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      setTimeout(resolve, 0);
    });
  });
}

function revealOverlay() {
  overlayReady.value = true;
  ready.value = true;
}

/**
 * True when the slot we painted into is still the live layout size.
 * Scroll layout often starts at a placeholder aspect, then jumps to the real
 * page size mid-resample; revealing that bitmap would CSS-stretch into the new
 * box (wrong-ratio flash). Retry against the settled size instead.
 */
function paintSizeStillCurrent(width: number, height: number): boolean {
  const now = paintSize();
  return now.width === width && now.height === height;
}

function revealOverlayIfCurrent(width: number, height: number) {
  if (!paintSizeStillCurrent(width, height)) {
    scheduleRetry();
    return;
  }
  revealOverlay();
}

function pageDiskKey(index: number, assetId: string): string {
  const book = comic.value;
  const page = book?.pages[index];
  if (!book || !page) return "";
  return neuralDiskKey(
    assetId,
    settings.neuralTargetSize,
    bookFolderName(book.path),
    page.relPath || page.name,
  );
}

function scheduleNeuralPrefetch(assetId: string) {
  const targetSize = settings.neuralTargetSize;
  if (props.prefetchNextSrc) {
    const nextIndex = props.prefetchNextIndex ?? props.index + 1;
    prefetchNeuralUpscale(props.prefetchNextSrc, assetId, targetSize, NEURAL_PRIORITY_NEXT, {
      cacheKey: pageDiskKey(nextIndex, assetId),
      pageIndex: nextIndex,
    });
  }
  if (props.prefetchPrevSrc) {
    const prevIndex = props.prefetchPrevIndex ?? props.index - 1;
    if (prevIndex >= 0) {
      prefetchNeuralUpscale(props.prefetchPrevSrc, assetId, targetSize, NEURAL_PRIORITY_PREV, {
        cacheKey: pageDiskKey(prevIndex, assetId),
        pageIndex: prevIndex,
      });
    }
  }
}

async function ensureSource() {
  if (source && sourceKey === props.src) return source;
  // Must fetch -> blob -> ImageBitmap. Drawing the display <img> (comic://) taints 2d and
  // breaks getImageData used by classic resampling.
  try {
    const response = await fetch(props.src);
    if (!response.ok) throw new Error(`fetch ${response.status}`);
    const blob = await response.blob();
    const bytes = new Uint8Array(await blob.arrayBuffer());
    if (isAnimatedImage(bytes)) {
      skipAlgorithm.value = true;
      throw new Error(ANIMATED_IMAGE_SKIP);
    }
    const bitmap = await createImageBitmap(blob);
    source = bitmap;
    sourceKey = props.src;
    return bitmap;
  } catch (error) {
    if (skipAlgorithm.value || isAnimatedImageSkip(error)) throw error;
    const image = new Image();
    image.crossOrigin = "anonymous";
    image.decoding = "async";
    image.src = props.src;
    await image.decode().catch(() => undefined);
    if (!image.naturalWidth) throw new Error("decode failed");
    source = image;
    sourceKey = props.src;
    return image;
  }
}

function drawContainTop(
  ctx: CanvasRenderingContext2D,
  image: CanvasImageSource,
  width: number,
  height: number,
  srcW: number,
  srcH: number,
) {
  const scale = Math.min(width / srcW, height / srcH);
  const dw = srcW * scale;
  const dh = srcH * scale;
  const dx = (width - dw) / 2;
  ctx.clearRect(0, 0, width, height);
  ctx.drawImage(image, dx, 0, dw, dh);
}

let neuralJobGen = 0;

async function syncNeuralJob() {
  if (!isNeuralProfile(props.profile) || skipAlgorithm.value) {
    neuralBusy.value = false;
    deferredNeural = false;
    return;
  }
  if (scrollDefersHeavy()) {
    deferredNeural = true;
    neuralBusy.value = false;
    return;
  }
  deferredNeural = false;
  if ((root.value?.clientWidth ?? 0) < 16) {
    neuralBusy.value = false;
    return;
  }
  const assetId = neuralAssetId(props.profile);
  const gen = ++neuralJobGen;
  if (neuralSkipFlags.value[props.index] || neuralDoneFlags.value[props.index]) {
    neuralBusy.value = false;
    scheduleNeuralPrefetch(assetId);
    return;
  }
  try {
    const image = await ensureSource();
    if (gen !== neuralJobGen) return;
    if (skipAlgorithm.value) {
      neuralBusy.value = false;
      return;
    }
    const natural = sourceSize(image);
    const neuralSize = resolveNeuralOutputSize(
      natural.width,
      natural.height,
      settings.neuralTargetSize,
    );
    if (!neuralSize) {
      applyNeuralSkip(props.index);
      neuralBusy.value = false;
      scheduleNeuralPrefetch(assetId);
      return;
    }
    neuralBusy.value = true;
    scheduleNeuralPrefetch(assetId);
    await runNeuralUpscale(image, assetId, neuralSize.width, neuralSize.height, {
      cacheKey: pageDiskKey(props.index, assetId),
      pageIndex: props.index,
      priority: NEURAL_PRIORITY_CURRENT,
      requeueOnCancel: false,
    });
  } catch (error) {
    if (gen !== neuralJobGen || isDetachedImageError(error) || isNeuralCancelled(error)) return;
    if (skipAlgorithm.value || isAnimatedImageSkip(error)) {
      neuralBusy.value = false;
      return;
    }
    notifyError(String(error));
  } finally {
    if (gen === neuralJobGen) neuralBusy.value = false;
  }
}

async function redraw() {
  if (!useOverlay.value) return;
  const gen = ++drawGen;
  await nextTick();
  const canvas = canvasEl.value;
  if (!canvas) {
    scheduleRetry();
    return;
  }
  const { width, height, dpr } = paintSize();
  if (width < 2 || height < 2) {
    scheduleRetry();
    return;
  }
  try {
    const image = await ensureSource();
    if (gen !== drawGen) return;
    if (skipAlgorithm.value || !useOverlay.value) {
      overlayReady.value = false;
      ready.value = true;
      return;
    }
    const natural = sourceSize(image);
    ready.value = true;
    naturalW.value = natural.width;
    naturalH.value = natural.height;
    syncMediaBox();
    emit("load", { index: props.index, width: natural.width, height: natural.height });

    if (mode.value === "anime4k" && isAnime4kProfile(props.profile)) {
      const preview = width < 16 || height < 16;
      const deferAnime4k = scrollDefersHeavy();
      if (preview || deferAnime4k) {
        if (deferAnime4k) deferredAnime4kPreview = true;
        if (canvas.width !== width || canvas.height !== height) {
          canvas.width = width;
          canvas.height = height;
        }
        const ctx = canvas.getContext("2d");
        if (ctx) {
          if (props.objectFit === "contain-top") {
            drawContainTop(ctx, image, width, height, natural.width, natural.height);
          } else {
            ctx.clearRect(0, 0, width, height);
            ctx.drawImage(image, 0, 0, width, height);
          }
        }
        // Tiny warm slots stay overlay-hidden so a 2px bitmap is never CSS-stretched on reveal.
        if (!preview) revealOverlayIfCurrent(width, height);
        return;
      }
      const strength = props.profile as Anime4kProfile;
      if (!anime4k || anime4k.currentProfile !== strength) {
        anime4k?.dispose();
        anime4k = new Anime4kPresenter(strength);
      }
      await afterPaint();
      if (gen !== drawGen) return;
      if (!paintSizeStillCurrent(width, height)) {
        scheduleRetry();
        return;
      }
      deferredAnime4kPreview = false;
      anime4k.draw(
        image,
        canvas,
        width,
        height,
        props.objectFit === "contain-top" ? "contain-top" : "fill",
        dpr,
      );
      if (gen !== drawGen) return;
      revealOverlayIfCurrent(width, height);
      return;
    }

    anime4k?.dispose();
    anime4k = null;
    // Assigning width/height clears the bitmap; keep the previous frame while resampling.
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) return;

    // Warm/offscreen slots are ~2px; avoid full-res ImageData kernels on every neighbor.
    const preview = width < 16 || height < 16;

    if (needsCanvas2d(props.profile)) {
      if (preview) {
        ctx.drawImage(image, 0, 0, width, height);
        // Do not reveal: warm 2px overlays stretch into a dark flash when the slot becomes visible.
        return;
      }
      // During scroll motion, skip classic kernels — eye won't catch quality mid-glide.
      if (scrollDefersHeavy()) {
        deferredClassicPreview = true;
        if (props.objectFit === "contain-top") {
          drawContainTop(ctx, image, width, height, natural.width, natural.height);
        } else {
          ctx.clearRect(0, 0, width, height);
          ctx.drawImage(image, 0, 0, width, height);
        }
        revealOverlayIfCurrent(width, height);
        return;
      }
      // Cheap drawImage first so the slot never sits on --img while bicubic/lanczos runs.
      if (props.objectFit === "contain-top") {
        drawContainTop(ctx, image, width, height, natural.width, natural.height);
      } else {
        ctx.clearRect(0, 0, width, height);
        ctx.drawImage(image, 0, 0, width, height);
      }
      revealOverlayIfCurrent(width, height);
      await afterPaint();
      if (gen !== drawGen) return;
      if (!paintSizeStillCurrent(width, height)) {
        scheduleRetry();
        return;
      }
      const src = await imageToImageData(image, natural.width, natural.height);
      if (gen !== drawGen) return;
      let tw = width;
      let th = height;
      if (props.objectFit === "contain-top") {
        const scale = Math.min(width / natural.width, height / natural.height);
        tw = Math.max(1, Math.round(natural.width * scale));
        th = Math.max(1, Math.round(natural.height * scale));
      }
      const data = await resampleImageDataAsync(src, tw, th, classicKind(props.profile));
      if (gen !== drawGen) return;
      if (!paintSizeStillCurrent(width, height)) {
        scheduleRetry();
        return;
      }
      deferredClassicPreview = false;
      // putImageData replaces the cheap preview; keep overlay visible across the swap.
      ctx.clearRect(0, 0, width, height);
      if (props.objectFit === "contain-top") {
        const dx = Math.round((width - tw) / 2);
        ctx.putImageData(data, dx, 0);
      } else {
        ctx.putImageData(data, 0, 0);
      }
      revealOverlayIfCurrent(width, height);
      return;
    }

    if (props.objectFit === "contain-top") {
      drawContainTop(ctx, image, width, height, natural.width, natural.height);
    } else {
      ctx.clearRect(0, 0, width, height);
      ctx.drawImage(image, 0, 0, width, height);
    }
    revealOverlayIfCurrent(width, height);
  } catch (error) {
    if (gen !== drawGen || isDetachedImageError(error)) return;
    if (skipAlgorithm.value || isAnimatedImageSkip(error)) {
      overlayReady.value = false;
      ready.value = true;
      return;
    }
    console.error("page media redraw failed", error);
    overlayReady.value = false;
    ready.value = true;
    emit("error", { index: props.index });
  }
}

function onImgLoad(event: Event) {
  const image = event.target;
  if (!(image instanceof HTMLImageElement) || !image.naturalWidth) return;
  ready.value = true;
  naturalW.value = image.naturalWidth;
  naturalH.value = image.naturalHeight;
  syncMediaBox();
  emit("load", { index: props.index, width: image.naturalWidth, height: image.naturalHeight });
  if (useOverlay.value) void redraw();
}

function onUpscaledLoad() {
  upscaledReady.value = true;
}

function onUpscaledError() {
  neuralUrlFailed.value = true;
  upscaledReady.value = false;
}

function onImgError() {
  ready.value = true;
  overlayReady.value = false;
  emit("error", { index: props.index });
}

function releaseSource() {
  if (source && "close" in source) {
    try {
      source.close();
    } catch {
      // ImageBitmap may already be closed.
    }
  }
  source = null;
  sourceKey = "";
}

onMounted(() => {
  observer = new ResizeObserver(() => {
    syncMediaBox();
    // Hide before CSS stretches a stale canvas bitmap into the new box.
    if (overlayReady.value) overlayReady.value = false;
    scheduleResizeRedraw();
  });
  if (root.value) observer.observe(root.value);
  syncMediaBox();
  if (useOverlay.value) void redraw();
  if (isNeuralProfile(props.profile)) void syncNeuralJob();
});

onBeforeUnmount(() => {
  drawGen += 1;
  neuralBusy.value = false;
  window.clearTimeout(retryTimer);
  window.clearTimeout(resizeTimer);
  observer?.disconnect();
  anime4k?.dispose();
  anime4k = null;
  releaseSource();
});

watch(
  () => props.src,
  () => {
    // Invalidate in-flight redraws before closing ImageBitmap (avoids detached drawImage).
    drawGen += 1;
    releaseSource();
    ready.value = false;
    overlayReady.value = false;
    neuralBusy.value = false;
    skipAlgorithm.value = false;
    neuralUrlFailed.value = false;
    upscaledReady.value = false;
    if (useOverlay.value) void redraw();
    if (isNeuralProfile(props.profile)) void syncNeuralJob();
  },
);

watch(
  () => [props.profile, props.objectFit, mode.value] as const,
  (current, previous) => {
    anime4k?.dispose();
    anime4k = null;
    neuralBusy.value = false;
    if (!useOverlay.value) {
      overlayReady.value = false;
      if (isNeuralProfile(props.profile)) void syncNeuralJob();
      return;
    }
    // Hide overlay while canvas remounts (2d / anime4k); base <img> stays visible.
    if (!previous || previous[2] !== current[2]) overlayReady.value = false;
    void redraw();
  },
  { flush: "post" },
);

watch(
  () => settings.neuralTargetSize,
  () => {
    if (!isNeuralProfile(props.profile)) return;
    neuralBusy.value = false;
    neuralUrlFailed.value = false;
    upscaledReady.value = false;
    void syncNeuralJob();
  },
);

watch(
  () => neuralDoneFlags.value[props.index],
  (done) => {
    if (!isNeuralProfile(props.profile) || !done) return;
    neuralBusy.value = false;
  },
);

watch(scrollResampleReady, (ready) => {
  if (!ready || settings.readingMode !== "scroll") return;
  if (deferredClassicPreview && needsCanvas2d(props.profile) && useOverlay.value) {
    void redraw();
  }
  if (deferredAnime4kPreview && mode.value === "anime4k" && useOverlay.value) {
    void redraw();
  }
  if (deferredNeural && isNeuralProfile(props.profile)) {
    void syncNeuralJob();
  }
});

watch(upscaledSrc, () => {
  upscaledReady.value = false;
});
</script>

<template>
  <div ref="root" class="page-media" :class="{ pending, show: ready, overlay: useOverlay }">
    <img
      class="page-bitmap base"
      :src="src"
      :data-index="index"
      :alt="alt || String(index + 1)"
      :style="imgStyle"
      draggable="false"
      @load="onImgLoad"
      @error="onImgError"
    />
    <img
      v-if="upscaledSrc"
      class="page-bitmap overlay"
      :class="{ show: upscaledReady }"
      :src="upscaledSrc"
      :alt="alt || String(index + 1)"
      :style="imgStyle"
      draggable="false"
      @load="onUpscaledLoad"
      @error="onUpscaledError"
    />
    <!-- :key remounts when context family changes (2d / anime4k). -->
    <canvas
      v-if="useOverlay"
      :key="mode"
      ref="canvasEl"
      class="page-bitmap overlay"
      :class="{ show: overlayReady }"
      :data-index="index"
    />
    <div v-if="neuralBusy" class="page-media-neural-busy" aria-hidden="true" />
    <div class="page-media-ocr-plane" :style="ocrPlaneStyle">
      <slot />
    </div>
  </div>
</template>

<style>
.scroll-view img,
.page-stage img,
.scroll-view .page-bitmap,
.page-stage .page-bitmap,
.page-media {
  display: block;
  width: 100%;
  min-width: 0;
  min-height: 0;
  background: var(--img);
}

.page-media {
  position: relative;
  height: 100%;
}

.page-media .page-bitmap {
  width: 100%;
  height: 100%;
  background: transparent;
}

/* Keep decoded <img> as fallback; canvas covers it only after a frame is drawn. */
.page-media .page-bitmap.overlay {
  position: absolute;
  inset: 0;
  opacity: 0;
  pointer-events: none;
}

.page-media .page-bitmap.overlay.show {
  opacity: 1;
}

.page-media-neural-busy {
  position: absolute;
  inset: 0;
  z-index: 2;
  pointer-events: none;
  background-color: color-mix(in srgb, var(--img) 55%, transparent);
  background-image: linear-gradient(
    90deg,
    transparent 0%,
    color-mix(in srgb, var(--img-flow) 70%, transparent) 45%,
    transparent 90%
  );
  background-size: 200% 100%;
  animation: img-flow 1.2s linear infinite;
}

@keyframes img-flow {
  0% {
    background-position: 100% 0;
  }
  100% {
    background-position: -100% 0;
  }
}

/* Align OCR % boxes to the painted bitmap when object-fit is contain. */
.page-media-ocr-plane {
  position: absolute;
  z-index: 3;
  pointer-events: none;
  overflow: hidden;
}
</style>
