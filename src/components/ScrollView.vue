<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { bindStagePointer, GLIDE_MS, glideEase, reducedMotion } from "../lib/stagePointer";
import { cssFromImagePx } from "../lib/displayScale";
import {
  bindScroll,
  consumeScrollBehavior,
  displayedZoom,
  markScrollMotion,
  pausePlayClock,
  resetPlayClock,
  resumePlayClock,
  getPageRotation,
  getPageSpin,
  isPageSpinSnap,
  halfOf,
  imageSrc,
  logicalToPhysical,
  altDown,
  ocrEnabled,
  ocrRegionsForLogical,
  pageIndex,
  pageSpins,
  rotatedSize,
  rotateGuide,
  rotationSwapped,
  scheduleRemember,
  settings,
  screenScale,
  scrollOffset,
  scrollStepPx,
  scrollViewportPages,
  type FitMode,
} from "../reader";
import MagnifierLoupe from "./MagnifierLoupe.vue";
import OcrOverlay from "./OcrOverlay.vue";
import OverlayScrollbar from "./OverlayScrollbar.vue";
import PageMedia from "./PageMedia.vue";

const props = defineProps<{
  count: number;
  index: number;
  fit: FitMode;
  fixedWidth: number;
  originalScale: number;
  direction: "ltr" | "rtl";
  sizes: { width: number; height: number }[];
  /** When false, ignore bulk size probes so the scrollbar is not rebuilt on every page. */
  sizesReady: boolean;
  resume: number;
}>();
const emit = defineEmits<{ "update:index": [index: number] }>();

const scroller = ref<HTMLElement | null>(null);
const heights = ref<number[]>([]);
const widths = ref<number[]>([]);
const sheetPx = ref<number | null>(null);
const rangeStart = ref(0);
const rangeEnd = ref(0);
const spacerTop = ref(0);
const spacerBottom = ref(0);
const broken = ref<boolean[]>([]);
const ready = ref<Record<number, boolean>>({});

let natural: ({ w: number; h: number } | null)[] = [];
/** Placeholder ratio before real sizes arrive; prefer taller than wide to avoid squash flash. */
let aspect = 2.2;
let prefix: number[] = [0];
let pendingScroll: number | null = null;
let programmatic: number | null = null;
let glideFrame = 0;
/** Shared glide state — retarget updates these; one rAF loop keeps running. */
let glideFrom = 0;
let glideTo = 0;
let glideStarted = 0;
/** Coalesce layout("keep") from page loads; flush after glide so settle doesn't hitch. */
let layoutKeepQueued = false;
let layoutKeepAfterGlide = false;
let settleGen = 0;
let resumeOffset: number | null = null;
let observedWidth = 0;
let observer: ResizeObserver | null = null;
let unbindPointer: (() => void) | null = null;

const visible = computed(() => {
  const indexes: number[] = [];
  for (let index = rangeStart.value; index < rangeEnd.value; index += 1) indexes.push(index);
  return indexes;
});

const fitMode = computed(() => (props.fit === "window" ? "auto" : props.fit));

const sheetStyle = computed(() =>
  sheetPx.value == null ? { width: "100%" } : { width: `${sheetPx.value}px` },
);

function indexAt(y: number): number {
  if (props.count <= 0) return 0;
  let lo = 0;
  let hi = props.count - 1;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if ((prefix[mid] ?? 0) <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function place(scrollTop: number) {
  const el = scroller.value;
  if (!el || props.count <= 0) {
    if (scrollViewportPages.value.length) scrollViewportPages.value = [];
    return;
  }
  const view = el.clientHeight || 1;
  const start = indexAt(Math.max(0, scrollTop - view));
  const end = Math.min(props.count, indexAt(scrollTop + view * 2) + 1);
  rangeStart.value = start;
  rangeEnd.value = Math.max(start + 1, end);
  spacerTop.value = prefix[start] ?? 0;
  spacerBottom.value = (prefix[props.count] ?? 0) - (prefix[rangeEnd.value] ?? 0);
  publishViewportPages(scrollTop, view);
}

/**
 * Pages intersecting the viewport — only needed while translation is on
 * (`translationTargetPages` in reader). Neural focus uses `pageIndex`, not this list.
 */
function publishViewportPages(scrollTop: number, viewHeight: number) {
  if (!ocrEnabled.value || props.count <= 0) {
    if (scrollViewportPages.value.length) scrollViewportPages.value = [];
    return;
  }
  const viewTop = scrollTop;
  const viewBottom = scrollTop + Math.max(1, viewHeight);
  // Contiguous strip: binary-search the page under each edge (O(log n)).
  const first = indexAt(viewTop);
  const last = indexAt(Math.max(viewTop, viewBottom - 1e-3));
  const indexes: number[] = [];
  for (let index = first; index <= last; index += 1) indexes.push(index);
  const prev = scrollViewportPages.value;
  if (prev.length === indexes.length && prev.every((page, i) => page === indexes[i])) return;
  scrollViewportPages.value = indexes;
}

function stopGlide() {
  if (!glideFrame) return;
  cancelAnimationFrame(glideFrame);
  glideFrame = 0;
}

function flushLayoutKeep() {
  layoutKeepQueued = false;
  layoutKeepAfterGlide = false;
  layout("keep");
}

/** Batch page-load layouts; never run a full layout mid-glide (causes post-settle hitch). */
function requestLayoutKeep() {
  if (glideFrame || programmatic != null) {
    layoutKeepAfterGlide = true;
    return;
  }
  if (layoutKeepQueued) return;
  layoutKeepQueued = true;
  requestAnimationFrame(() => {
    if (glideFrame || programmatic != null) {
      layoutKeepQueued = false;
      layoutKeepAfterGlide = true;
      return;
    }
    flushLayoutKeep();
  });
}

function tickGlide() {
  const node = scroller.value;
  if (!node) {
    glideFrame = 0;
    return;
  }
  const eased = glideEase((performance.now() - glideStarted) / GLIDE_MS);
  node.scrollTop = glideFrom + (glideTo - glideFrom) * Math.min(1, eased);
  if (eased < 1) {
    glideFrame = requestAnimationFrame(tickGlide);
    return;
  }
  glideFrame = 0;
  if (Math.abs(node.scrollTop - glideTo) > 0.5) node.scrollTop = glideTo;
  // Settle page-turn bookkeeping here so deferred layout isn't blocked on programmatic.
  if (programmatic != null && Math.abs(node.scrollTop - (prefix[programmatic] ?? 0)) <= 1) {
    programmatic = null;
    publishOffset(node.scrollTop);
    scheduleRemember();
  }
  // Next frame: one coalesced layout if pages loaded during the glide.
  if (layoutKeepAfterGlide) {
    requestAnimationFrame(() => {
      if (glideFrame) return;
      if (layoutKeepAfterGlide) flushLayoutKeep();
    });
  }
}

/** Cancel-and-retarget: refresh from/to/clock; keep a single rAF loop (no stacked glides). */
function startOrRetargetGlide(to: number, page: number | null) {
  const el = scroller.value;
  if (!el) return;
  markScrollMotion();
  glideFrom = el.scrollTop;
  glideTo = to;
  glideStarted = performance.now();
  if (page != null) {
    programmatic = page;
    scrollOffset.value = 0;
    settleGen += 1;
  } else {
    programmatic = null;
  }
  if (!glideFrame) glideFrame = requestAnimationFrame(tickGlide);
}

function displayNatural(index: number) {
  const size = natural[index];
  if (!size || !(size.w > 0) || !(size.h > 0)) return null;
  const rotated = rotatedSize(size.w, size.h, getPageRotation(index));
  return { w: rotated.width, h: rotated.height };
}

function pageWidth(index: number, viewportWidth: number): number {
  const size = displayNatural(index);
  const naturalW = size && size.w > 0 ? size.w : 0;
  const mode = fitMode.value;
  if (mode === "fixedWidth") return cssFromImagePx(Math.max(1, props.fixedWidth));
  if (mode === "original") return cssFromImagePx(naturalW || viewportWidth);
  if (mode === "originalScale") {
    return cssFromImagePx(naturalW || viewportWidth, Math.max(0.01, props.originalScale));
  }
  return viewportWidth;
}

function syncHorizontalOrigin(el: HTMLElement) {
  const limit = Math.max(0, el.scrollWidth - el.clientWidth);
  el.scrollLeft = props.direction === "rtl" ? limit : 0;
}

function scrollZoomPercent(): number | null {
  const mode = fitMode.value;
  if (mode === "original") return 100;
  if (mode === "originalScale") return Math.round(Math.max(0.01, props.originalScale) * 100);
  const size = displayNatural(pageIndex.value);
  if (!size) return null;
  if (mode === "fixedWidth") return Math.round((Math.max(1, props.fixedWidth) / size.w) * 100);
  const width = scroller.value?.clientWidth || observedWidth;
  if (width <= 0) return null;
  return Math.round((width * Math.max(0.01, screenScale.value) / size.w) * 100);
}

function publishZoom() {
  displayedZoom.value = scrollZoomPercent();
}

function layout(mode: "keep" | "jump") {
  publishZoom();
  const el = scroller.value;
  stopGlide();
  programmatic = null;
  if (!el || props.count <= 0) return;
  const width = el.clientWidth;
  if (width <= 0) return;
  observedWidth = width;
  if (natural.length !== props.count) {
    natural = Array.from({ length: props.count }, () => null);
    broken.value = Array.from({ length: props.count }, () => false);
  }
  absorbSizes();
  const restoring = resumeOffset != null;
  const scroll = pendingScroll ?? el.scrollTop;
  const previousPrefix = prefix;
  const anchor = mode === "keep" && !restoring ? indexAt(scroll + 1) : props.index;
  const oldHeight = heights.value[anchor] || 1;
  const offset = mode === "keep" && !restoring ? scroll - (previousPrefix[anchor] ?? 0) : 0;
  const nextHeights: number[] = [];
  const nextWidths: number[] = [];
  const nextPrefix = [0];
  let maxW = 0;
  const gap = Math.max(0, settings.spreadGap);
  for (let index = 0; index < props.count; index += 1) {
    const size = displayNatural(index);
    const renderedWidth = pageWidth(index, width);
    nextWidths.push(renderedWidth);
    maxW = Math.max(maxW, renderedWidth);
    const raw = size && size.w > 0 ? (renderedWidth * size.h) / size.w : renderedWidth * aspect;
    nextHeights.push(Math.max(1, Math.round(raw)));
    const after = index < props.count - 1 ? gap : 0;
    nextPrefix.push((nextPrefix[index] ?? 0) + (nextHeights[index] ?? 0) + after);
  }
  heights.value = nextHeights;
  widths.value = nextWidths;
  publishZoom();
  sheetPx.value = fitMode.value === "auto" ? null : Math.max(1, Math.round(maxW));
  prefix = nextPrefix;
  const pageHeight = nextHeights[anchor] || 1;
  const fraction = restoring ? (resumeOffset ?? 0) : 0;
  if (restoring) resumeOffset = null;
  const top = mode === "jump" || restoring
    ? (nextPrefix[anchor] ?? 0) + fraction * pageHeight
    : (nextPrefix[anchor] ?? 0) + offset * (pageHeight / oldHeight);
  place(top);
  publishOffset(top, anchor);
  pendingScroll = top;
  const gen = ++settleGen;
  void nextTick(() => {
    const node = scroller.value;
    if (!node || gen !== settleGen) return;
    node.scrollTop = top;
    if (mode === "jump" || restoring) syncHorizontalOrigin(node);
    pendingScroll = null;
  });
}

function publishOffset(top: number, index = indexAt(top + 1)) {
  const start = prefix[index] ?? 0;
  const height = (prefix[index + 1] ?? start + 1) - start || 1;
  const fraction = (top - start) / height;
  scrollOffset.value = Math.min(1, Math.max(0, fraction));
}

function absorbSizes(): boolean {
  if (!props.sizesReady || natural.length !== props.count) return false;
  let changed = false;
  for (let index = 0; index < props.count; index += 1) {
    const size = props.sizes[index];
    if (!size || !(size.width > 0) || !(size.height > 0)) continue;
    const previous = natural[index];
    if (previous && previous.w === size.width && previous.h === size.height) continue;
    if (!natural.some((item) => item)) aspect = size.height / size.width;
    natural[index] = { w: size.width, h: size.height };
    changed = true;
  }
  return changed;
}

function scrollToPage(index: number, behavior: ScrollBehavior) {
  const el = scroller.value;
  if (!el || props.count <= 0) return;
  const clamped = Math.max(0, Math.min(props.count - 1, index));
  const top = prefix[clamped] ?? 0;
  const smooth =
    behavior === "smooth" &&
    settings.smoothScroll &&
    !reducedMotion() &&
    Math.abs(el.scrollTop - top) > 1;
  if (smooth) {
    startOrRetargetGlide(top, clamped);
    return;
  }
  stopGlide();
  programmatic = null;
  pendingScroll = top;
  place(top);
  publishOffset(top, clamped);
  const gen = ++settleGen;
  void nextTick(() => {
    const node = scroller.value;
    if (!node || gen !== settleGen) return;
    node.scrollTop = top;
    pendingScroll = null;
  });
}

function haltProgrammatic() {
  if (programmatic == null) return;
  const el = scroller.value;
  stopGlide();
  programmatic = null;
  if (!el) return;
  el.scrollTop = el.scrollTop;
  publishOffset(el.scrollTop);
  const index = indexAt(el.scrollTop + 1);
  if (index !== props.index) emit("update:index", index);
}

function onScroll() {
  if (pendingScroll != null) return;
  const el = scroller.value;
  if (!el) return;
  markScrollMotion();
  place(el.scrollTop);
  if (programmatic != null) {
    const top = prefix[programmatic] ?? 0;
    if (Math.abs(el.scrollTop - top) <= 1) {
      programmatic = null;
      publishOffset(el.scrollTop);
      scheduleRemember();
    }
    return;
  }
  publishOffset(el.scrollTop);
  const index = indexAt(el.scrollTop + 1);
  if (index !== props.index) emit("update:index", index);
  scheduleRemember();
}

function onMediaLoad(payload: { index: number; width: number; height: number }) {
  const { index, width, height } = payload;
  if (!(width > 0 && height > 0)) return;
  const previous = natural[index];
  if (previous && previous.w === width && previous.h === height) {
    if (!ready.value[index]) ready.value = { ...ready.value, [index]: true };
    return;
  }
  if (!natural.some((size) => size)) aspect = height / width;
  natural[index] = { w: width, h: height };
  ready.value = { ...ready.value, [index]: true };
  requestLayoutKeep();
}

function onMediaError(payload: { index: number }) {
  const index = payload.index;
  if (natural[index]) return;
  broken.value[index] = true;
  natural[index] = { w: 800, h: 240 };
  ready.value = { ...ready.value, [index]: true };
  requestLayoutKeep();
}

function onWheel(event: WheelEvent) {
  event.preventDefault();
  haltProgrammatic();
  resetPlayClock();
  scroller.value?.scrollBy({
    top: (event.deltaY > 0 ? 1 : -1) * scrollStepPx(),
  });
}

function stripStyle(index: number) {
  const height = heights.value[index] ?? 0;
  const gap = index < props.count - 1 ? Math.max(0, settings.spreadGap) : 0;
  const style: Record<string, string> = { height: `${height}px` };
  if (gap) style.marginBottom = `${gap}px`;
  if (sheetPx.value != null) style.width = `${widths.value[index] ?? sheetPx.value}px`;
  return style;
}

onMounted(() => {
  resumeOffset = props.resume > 0 ? props.resume : null;
  layout("jump");
  scroller.value?.addEventListener("wheel", onWheel, { passive: false });
  scroller.value?.addEventListener("pointerdown", haltProgrammatic);
  observer = new ResizeObserver(() => {
    const width = scroller.value?.clientWidth ?? 0;
    if (!width || width === observedWidth) return;
    observedWidth = width;
    layout("keep");
  });
  if (scroller.value) {
    observer.observe(scroller.value);
    unbindPointer = bindStagePointer(
      scroller.value,
      (forward) => {
        emit("update:index", props.index + (forward ? 1 : -1));
      },
      () => props.direction === "rtl",
      () => settings.smoothScroll,
      (active) => {
        if (active) pausePlayClock();
        else resumePlayClock();
      },
    );
  }
  bindScroll((dy, animate) => {
    const el = scroller.value;
    if (!el) return false;
    haltProgrammatic();
    const before = el.scrollTop;
    const to = Math.max(0, Math.min(Math.max(0, el.scrollHeight - el.clientHeight), before + dy));
    if (Math.abs(to - before) < 1) return false;
    if (animate && settings.smoothScroll && !reducedMotion()) {
      startOrRetargetGlide(to, null);
      return true;
    }
    el.scrollTop = to;
    return el.scrollTop !== before;
  }, scroller.value);
});

onBeforeUnmount(() => {
  stopGlide();
  scroller.value?.removeEventListener("wheel", onWheel);
  scroller.value?.removeEventListener("pointerdown", haltProgrammatic);
  observer?.disconnect();
  unbindPointer?.();
  bindScroll(null);
  scrollViewportPages.value = [];
});

watch(() => props.sizes, () => {
  if (!props.sizesReady) return;
  if (absorbSizes()) layout("keep");
}, { deep: true });
watch(() => props.sizesReady, (readyNow) => {
  if (!readyNow) return;
  if (absorbSizes()) layout("keep");
});
watch(
  () =>
    [fitMode.value, props.fixedWidth, props.originalScale, settings.spreadGap, settings.stagePadX, settings.stagePadY, screenScale.value] as const,
  () => layout("keep"),
);
watch(pageSpins, () => layout("keep"), { deep: true });
watch(pageIndex, () => publishZoom());
watch(() => props.direction, () => {
  const el = scroller.value;
  if (el) syncHorizontalOrigin(el);
});
watch(() => props.index, (index) => {
  const el = scroller.value;
  if (!el) return;
  const behavior = consumeScrollBehavior();
  const scroll = pendingScroll ?? el.scrollTop;
  if (indexAt(scroll + 1) === index) return;
  scrollToPage(index, behavior);
});
watch(ocrEnabled, (on) => {
  const el = scroller.value;
  if (!on || !el) {
    if (scrollViewportPages.value.length) scrollViewportPages.value = [];
    return;
  }
  publishViewportPages(el.scrollTop, el.clientHeight || 1);
});
</script>

<template>
  <div
    class="overlay-scroll-host"
    :style="{
      '--stage-pad-x': `${Math.max(0, settings.stagePadX)}px`,
      '--stage-pad-y': `${Math.max(0, settings.stagePadY)}px`,
    }"
  >
  <div class="stage-frame">
  <div
    ref="scroller"
    class="scroll-view"
    @scroll="onScroll"
  >
    <div class="sheet" :style="sheetStyle">
      <div class="spacer" :style="{ height: `${spacerTop}px` }" />
      <div
        v-for="index in visible"
        :key="index"
        class="strip"
        :class="{
          broken: broken[index],
          loading: !ready[index],
          'rotate-guide': !!rotateGuide && index === pageIndex,
        }"
        :style="stripStyle(index)"
      >
        <div
          class="page-rotate"
          :class="{
            swapped: rotationSwapped(getPageRotation(index)),
            'spin-snap': isPageSpinSnap(index),
          }"
          :style="{ '--spin': `${getPageSpin(index)}deg` }"
        >
          <PageMedia
            :src="imageSrc(index)"
            :index="index"
            :physical-index="logicalToPhysical(index)"
            :half="halfOf(index)"
            :profile="settings.renderProfile"
            :alt="String(index + 1)"
            object-fit="contain-top"
            :prefetch-next-src="index + 1 < count ? imageSrc(index + 1) : undefined"
            :prefetch-prev-src="index > 0 ? imageSrc(index - 1) : undefined"
            :prefetch-next-index="index + 1 < count ? logicalToPhysical(index + 1) : undefined"
            :prefetch-prev-index="index > 0 ? logicalToPhysical(index - 1) : undefined"
            @load="onMediaLoad"
            @error="onMediaError"
          >
            <OcrOverlay
              v-if="ocrEnabled"
              :regions="ocrRegionsForLogical(index)"
              :show-bounds="altDown"
            />
          </PageMedia>
        </div>
      </div>
      <div class="spacer" :style="{ height: `${spacerBottom}px` }" />
    </div>
  </div>
  </div>
  <MagnifierLoupe />
  <OverlayScrollbar :target="scroller" />
  </div>
</template>

<style>
.scroll-view {
  overflow-anchor: none;
}

.sheet {
  min-width: 0;
  margin: 0 auto;
}

.scroll-view .strip {
  position: relative;
  width: 100%;
  min-width: 0;
  min-height: 0;
  margin-left: auto;
  margin-right: auto;
  background: var(--img);
  overflow: hidden;
  container-type: size;
}

.scroll-view .strip .page-media {
  height: 100%;
  opacity: 0;
  background: transparent;
}

.scroll-view .strip .page-media.show {
  opacity: 1;
}

.scroll-view .strip .page-bitmap {
  object-fit: contain;
  object-position: center top;
}

.page-stage .slot.rotate-guide .page-media,
.page-stage .slot.rotate-guide img,
.page-stage .slot.rotate-guide .page-bitmap,
.scroll-view .strip.rotate-guide .page-media,
.scroll-view .strip.rotate-guide img,
.scroll-view .strip.rotate-guide .page-bitmap {
  outline: 1px solid var(--accent);
  outline-offset: -1px;
}

.page-stage .slot.loading,
.page-stage .slot.loading img,
.page-stage .slot.loading .page-media,
.scroll-view .strip.loading {
  background-color: var(--img);
  background-image: linear-gradient(
    90deg,
    var(--img) 0%,
    var(--img-flow) 45%,
    var(--img) 90%
  );
  background-size: 200% 100%;
  animation: img-flow 1.2s linear infinite;
}
</style>
