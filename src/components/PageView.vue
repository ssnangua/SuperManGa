<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { bindStagePointer, scrollStageOrigin } from "../lib/stagePointer";
import {
  bindPageStage,
  getPageRotation,
  getPageSpin,
  isPageSpinSnap,
  halfOf,
  imageSrc,
  logicalToPhysical,
  ocrEnabled,
  altDown,
  ocrRegionsForLogical,
  pageIndex,
  pageSpins,
  pausePlayClock,
  resetPlayClock,
  resumePlayClock,
  rotatedSize,
  rotateGuide,
  rotationSwapped,
  screenScale,
  settings,
  type FitMode,
} from "../reader";
import { spreadIndexes, type ReadingMode } from "../lib/spread";
import MagnifierLoupe from "./MagnifierLoupe.vue";
import OcrOverlay from "./OcrOverlay.vue";
import OverlayScrollbar from "./OverlayScrollbar.vue";
import PageMedia from "./PageMedia.vue";

const props = defineProps<{
  count: number;
  index: number;
  mode: ReadingMode;
  direction: "ltr" | "rtl";
  fit: FitMode;
  fixedWidth: number;
  originalScale: number;
  isolateCover: boolean;
  sizes: { width: number; height: number }[];
}>();
const emit = defineEmits<{ "update:index": [index: number] }>();

const stage = ref<HTMLElement | null>(null);
const measured = ref<Record<number, { width: number; height: number }>>({});
const ready = ref<Record<number, boolean>>({});
const viewport = ref({ width: 0, height: 0 });
/** Spread index currently painted; lags until the target pages are ready (keeps old image on jumps). */
const paintedIndex = ref(props.index);
let unbindPointer: (() => void) | null = null;
let observer: ResizeObserver | null = null;

function measureViewport() {
  const root = stage.value;
  if (!root) return;
  const width = root.clientWidth;
  const height = root.clientHeight;
  if (viewport.value.width === width && viewport.value.height === height) return;
  viewport.value = { width, height };
}

function spreadOf(index: number) {
  return spreadIndexes(index, props.count, props.mode, props.isolateCover);
}

/** Prefetch counterpart in the next/prev spread (double) or immediate neighbor (single). */
function prefetchTarget(page: number, forward: boolean): { index: number; src: string } | undefined {
  if (props.count <= 0) return undefined;
  let target: number | undefined;
  if (props.mode !== "double") {
    const next = forward ? page + 1 : page - 1;
    if (next < 0 || next >= props.count) return undefined;
    target = next;
  } else {
    const current = spreadOf(page);
    if (!current.length) return undefined;
    const first = current[0]!;
    const last = current[current.length - 1]!;
    const neighborIndex = forward ? last + 1 : first - 1;
    if (neighborIndex < 0 || neighborIndex >= props.count) return undefined;
    const neighbor = spreadOf(neighborIndex);
    if (!neighbor.length) return undefined;
    const offset = Math.max(0, page - first);
    target = neighbor[Math.min(offset, neighbor.length - 1)];
  }
  if (target == null) return undefined;
  return { index: logicalToPhysical(target), src: imageSrc(target) };
}

function pagesReady(pages: number[]) {
  return pages.length > 0 && pages.every((index) => ready.value[index]);
}

const paintedSpread = computed(() => spreadOf(paintedIndex.value));
const targetSpread = computed(() => spreadOf(props.index));

const visibleSet = computed(() => new Set(paintedSpread.value));

/** Keep current ± neighbor spreads mounted so adjacent turns reuse live PageMedia. */
const mountPages = computed(() => {
  const indexes = new Set<number>();
  const addSpread = (index: number) => {
    for (const page of spreadOf(index)) indexes.add(page);
  };
  for (const index of [paintedIndex.value, props.index]) {
    const pages = spreadOf(index);
    if (!pages.length) continue;
    addSpread(index);
    const first = pages[0]!;
    const last = pages[pages.length - 1]!;
    if (first > 0) addSpread(first - 1);
    if (last + 1 < props.count) addSpread(last + 1);
  }
  return [...indexes].sort((a, b) => a - b);
});

/** Double-page with only one real image (cover alone / trailing odd page) keeps a blank mate. */
const needsPlaceholder = computed(
  () => props.mode === "double" && paintedSpread.value.length === 1,
);

const isPair = computed(
  () => paintedSpread.value.length > 1 || needsPlaceholder.value,
);

const displayOrder = computed(() => {
  const pages = paintedSpread.value;
  if (props.direction === "rtl" && pages.length > 1) return pages.slice().reverse();
  return pages;
});

type LayoutSlot =
  | {
      kind: "page";
      page: number;
      /** Off-layout neighbor kept mounted so adjacent turns reuse the same PageMedia. */
      warm?: boolean;
      prefetchNext?: { index: number; src: string };
      prefetchPrev?: { index: number; src: string };
    }
  | { kind: "placeholder"; width: number; height: number };

function pageSlot(page: number, warm = false): LayoutSlot {
  return {
    kind: "page",
    page,
    warm,
    prefetchNext: warm ? undefined : prefetchTarget(page, true),
    prefetchPrev: warm ? undefined : prefetchTarget(page, false),
  };
}

/** Visible spread (+ blank mate) first, then warm neighbors — same parent so Vue reuses PageMedia by page key. */
const layoutSlots = computed((): LayoutSlot[] => {
  const pages = paintedSpread.value;
  let visible: LayoutSlot[];
  if (!needsPlaceholder.value) {
    visible = displayOrder.value.map((page) => pageSlot(page));
  } else {
    const page = pages[0]!;
    const size = sizeOf(page);
    const image = pageSlot(page);
    const blank: LayoutSlot = { kind: "placeholder", width: size.width, height: size.height };
    // 左开(ltr): image | blank ；右开(rtl): blank | image
    visible = props.direction === "rtl" ? [blank, image] : [image, blank];
  }
  const visiblePages = new Set(displayOrder.value);
  const warm = mountPages.value
    .filter((page) => !visiblePages.has(page))
    .map((page) => pageSlot(page, true));
  return [...visible, ...warm];
});

function exactSize(index: number) {
  const page = props.sizes[index];
  if (page && page.width > 0 && page.height > 0) return { width: page.width, height: page.height };
  return measured.value[index] ?? null;
}

/** Prefer real page metrics; fall back to a nearby page so the slot aspect is close before decode. */
function naturalSizeOf(index: number) {
  const exact = exactSize(index);
  if (exact) return exact;
  for (let distance = 1; distance < props.count; distance += 1) {
    for (const near of [index - distance, index + distance]) {
      if (near < 0 || near >= props.count) continue;
      const size = exactSize(near);
      if (size) return size;
    }
  }
  return { width: 2, height: 3 };
}

function sizeOf(index: number) {
  const natural = naturalSizeOf(index);
  return rotatedSize(natural.width, natural.height, getPageRotation(index));
}

function waitForExactSize(index: number) {
  return !exactSize(index);
}

function layoutSizes() {
  const pages = paintedSpread.value.map((index) => sizeOf(index));
  if (needsPlaceholder.value && pages[0]) return [pages[0], pages[0]];
  return pages;
}

const frame = computed(() => {
  void pageSpins;
  const pages = layoutSizes();
  const sumW = pages.reduce((sum, page) => sum + page.width, 0) || 1;
  const maxH = pages.reduce((max, page) => Math.max(max, page.height), 0) || 1;
  const gap = pages.length > 1 ? Math.max(0, settings.spreadGap) : 0;
  return {
    "--spread": String(Math.max(1, pages.length)),
    "--sum-w": String(sumW),
    "--max-h": String(maxH),
    "--gaps": `${gap}px`,
    "--spread-gap": `${gap}px`,
    "--fixed-w": String(Math.max(1, props.fixedWidth)),
    "--scale": String(Math.max(0.01, props.originalScale)),
    "--dpr": String(Math.max(0.01, screenScale.value)),
  };
});

const stagePadStyle = computed(() => ({
  "--stage-pad-x": `${Math.max(0, settings.stagePadX)}px`,
  "--stage-pad-y": `${Math.max(0, settings.stagePadY)}px`,
}));

const revealed = computed(
  () => paintedSpread.value.length < 2 || paintedSpread.value.every((index) => ready.value[index]),
);

const layoutFit = computed(() => {
  void pageSpins;
  if (props.fit === "window") return "page";
  if (props.fit === "original") return "original";
  if (props.fit === "fixedWidth") return "fixed-width";
  if (props.fit === "originalScale") return "original-scale";
  const { width, height } = viewport.value;
  const pages = layoutSizes();
  const sumW = pages.reduce((sum, page) => sum + page.width, 0);
  const maxH = pages.reduce((max, page) => Math.max(max, page.height), 0);
  if (width <= 0 || height <= 0 || sumW <= 0 || maxH <= 0) return "page";
  const verticalOverflow = width * maxH / sumW - height;
  const horizontalOverflow = height * sumW / maxH - width;
  if (verticalOverflow >= horizontalOverflow) {
    const still = width * maxH / sumW - height;
    return still > 1 ? "width" : "page";
  }
  const still = height * sumW / maxH - width;
  return still > 1 ? "height" : "page";
});

function settle(index: number, size?: { width: number; height: number }) {
  if (size && size.width > 0) {
    const page = props.sizes[index];
    const previous = measured.value[index];
    if (!(page && page.width > 0 && page.height > 0) && (!previous || previous.width !== size.width || previous.height !== size.height)) {
      measured.value = {
        ...measured.value,
        [index]: { width: size.width, height: size.height },
      };
    }
  }
  if (ready.value[index]) return;
  ready.value = { ...ready.value, [index]: true };
}

function onMediaLoad(payload: { index: number; width: number; height: number }) {
  settle(payload.index, payload);
}

function onMediaError(payload: { index: number }) {
  settle(payload.index);
}

function commitPainted(index: number) {
  if (paintedIndex.value === index) return;
  paintedIndex.value = index;
}

function tryCommitTarget() {
  if (paintedIndex.value === props.index) return;
  if (pagesReady(targetSpread.value)) commitPainted(props.index);
}

watch(
  () => [props.index, props.mode, props.isolateCover, props.count] as const,
  ([nextIndex], previous) => {
    // Same reading layout: hold the old spread until the target is decoded so jumps
    // (e.g. 50→100) don't flash the dark placeholder like sequential turns already avoid.
    if (
      previous
      && previous[1] === props.mode
      && previous[2] === props.isolateCover
      && previous[3] === props.count
      && !pagesReady(spreadOf(nextIndex))
    ) {
      return;
    }
    commitPainted(nextIndex);
  },
);

watch(mountPages, (pages) => {
  const keep = new Set(pages);
  const next = { ...ready.value };
  let changed = false;
  for (const key of Object.keys(next)) {
    const index = Number(key);
    if (!keep.has(index)) {
      delete next[index];
      changed = true;
    }
  }
  if (changed) ready.value = next;
});

watch([ready, () => props.index], () => {
  tryCommitTarget();
});

async function resetScrollOrigin() {
  await nextTick();
  const root = stage.value;
  if (!root) return;
  scrollStageOrigin(root, props.direction === "rtl");
}

watch(() => [paintedIndex.value, props.direction] as const, () => {
  void resetScrollOrigin();
});

watch(revealed, (ok) => {
  if (ok) void resetScrollOrigin();
});

watch(
  () => [settings.stagePadX, settings.stagePadY] as const,
  () => measureViewport(),
);

onMounted(() => {
  if (stage.value) {
    bindPageStage(stage.value);
    unbindPointer = bindStagePointer(
      stage.value,
      (forward) => {
        const step = Math.max(1, targetSpread.value.length);
        emit("update:index", props.index + (forward ? step : -step));
      },
      () => props.direction === "rtl",
      () => settings.smoothScroll,
      (active) => {
        if (active) pausePlayClock();
        else resumePlayClock();
      },
    );
    stage.value.addEventListener("wheel", resetPlayClock, { passive: true });
    measureViewport();
    observer = new ResizeObserver(() => measureViewport());
    observer.observe(stage.value);
    void resetScrollOrigin();
  }
});

onBeforeUnmount(() => {
  stage.value?.removeEventListener("wheel", resetPlayClock);
  bindPageStage(null);
  unbindPointer?.();
  observer?.disconnect();
});
</script>

<template>
  <div class="overlay-scroll-host" :style="stagePadStyle">
  <div class="stage-frame">
  <div
    ref="stage"
    class="page-stage"
    :class="[`fit-${layoutFit}`, { pair: isPair }]"
    :style="frame"
  >
    <div
      class="spread"
      :class="{ [`spread-${settings.spreadAlign}`]: isPair }"
    >
      <template v-for="(slot, slotIndex) in layoutSlots" :key="slot.kind === 'page' ? slot.page : `blank-${slotIndex}`">
        <div
          v-if="slot.kind === 'placeholder'"
          class="slot placeholder"
          aria-hidden="true"
          :style="{ '--w': slot.width, '--h': slot.height }"
        />
        <div
          v-else
          class="slot"
          :class="{
            warm: !!slot.warm,
            loading: !slot.warm && visibleSet.has(slot.page) && !ready[slot.page],
            'rotate-guide': !slot.warm && !!rotateGuide && slot.page === pageIndex,
          }"
          :style="{ '--w': sizeOf(slot.page).width, '--h': sizeOf(slot.page).height }"
        >
          <div
            class="page-rotate"
            :class="{
              swapped: rotationSwapped(getPageRotation(slot.page)),
              'spin-snap': isPageSpinSnap(slot.page),
            }"
            :style="{ '--spin': `${getPageSpin(slot.page)}deg` }"
          >
            <PageMedia
              :src="imageSrc(slot.page)"
              :index="slot.page"
              :physical-index="logicalToPhysical(slot.page)"
              :half="halfOf(slot.page)"
              :profile="settings.renderProfile"
              :alt="String(slot.page + 1)"
              object-fit="fill"
              :prefetch-next-src="slot.prefetchNext?.src"
              :prefetch-prev-src="slot.prefetchPrev?.src"
              :prefetch-next-index="slot.prefetchNext?.index"
              :prefetch-prev-index="slot.prefetchPrev?.index"
              :pending="
                !slot.warm
                  && visibleSet.has(slot.page)
                  && ((paintedSpread.length > 1 && !revealed) || (!ready[slot.page] && waitForExactSize(slot.page)))
              "
              @load="onMediaLoad"
              @error="onMediaError"
            >
              <OcrOverlay
                v-if="ocrEnabled && !slot.warm"
                :regions="ocrRegionsForLogical(slot.page)"
                :show-bounds="altDown"
              />
            </PageMedia>
          </div>
        </div>
      </template>
    </div>
  </div>
  </div>
  <MagnifierLoupe />
  <OverlayScrollbar :target="stage" />
  </div>
</template>

<style>
.page-rotate {
  position: absolute;
  top: 50%;
  left: 50%;
  width: 100cqw;
  height: 100cqh;
  transform: translate(-50%, -50%) rotate(var(--spin, 0deg));
  transform-origin: center center;
  transition: transform 0.22s ease;
}

.page-rotate.spin-snap {
  transition: none;
}

.page-rotate.swapped {
  width: 100cqh;
  height: 100cqw;
}

@media (prefers-reduced-motion: reduce) {
  .page-rotate {
    transition: none;
  }
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

.page-stage {
  position: relative;
  flex: 1 1 0;
  display: flex;
  align-items: safe center;
  justify-content: safe center;
  container-type: size;
}

.page-stage .spread {
  display: flex;
  flex: 0 0 auto;
  flex-direction: row;
  align-items: center;
  justify-content: center;
  gap: var(--spread-gap, 0px);
  min-width: 0;
  min-height: 0;
}

.page-stage .spread.spread-top {
  align-items: flex-start;
}

.page-stage .spread.spread-center {
  align-items: center;
}

.page-stage .spread.spread-bottom {
  align-items: flex-end;
}

.page-stage.fit-width .spread {
  width: 100%;
}

.page-stage.fit-height .spread {
  height: 100%;
}

.page-stage .slot {
  position: relative;
  flex: 0 0 auto;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  container-type: size;
}

.page-stage .slot img,
.page-stage .slot .page-media,
.page-stage .slot .page-bitmap {
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  object-fit: fill;
}

.page-stage .slot img.pending,
.page-stage .slot .page-media.pending {
  visibility: hidden;
}

.page-stage.fit-width {
  overflow-x: hidden;
  overflow-y: auto;
}
.page-stage.fit-height {
  overflow-x: auto;
  overflow-y: hidden;
}
.page-stage.fit-page {
  overflow: hidden;
}

.page-stage.fit-width .slot {
  width: calc((100% - var(--gaps)) * var(--w) / var(--sum-w));
  aspect-ratio: var(--w) / var(--h);
}

.page-stage.fit-height .slot {
  height: calc(100% * var(--h) / var(--max-h));
  aspect-ratio: var(--w) / var(--h);
}

.page-stage.fit-page .slot {
  width: auto;
  height: min(
    calc(100cqh * var(--h) / var(--max-h)),
    calc((100cqw - var(--gaps)) * var(--h) / var(--sum-w))
  );
  aspect-ratio: var(--w) / var(--h);
}

.page-stage.fit-original .slot {
  width: calc(var(--w) / var(--dpr, 1) * 1px);
  height: calc(var(--h) / var(--dpr, 1) * 1px);
}

.page-stage.fit-fixed-width .slot {
  width: calc(var(--fixed-w) / var(--dpr, 1) * 1px * var(--w) / var(--sum-w));
  aspect-ratio: var(--w) / var(--h);
}

.page-stage.fit-original-scale .slot {
  width: calc(var(--w) * var(--scale) / var(--dpr, 1) * 1px);
  height: calc(var(--h) * var(--scale) / var(--dpr, 1) * 1px);
}

/* Keep neighbor pages mounted off-layout so adjacent turns reuse decoded media. */
.page-stage .slot.warm {
  position: absolute;
  left: 0;
  top: 0;
  width: 2px;
  height: 2px;
  aspect-ratio: auto;
  overflow: hidden;
  opacity: 0;
  pointer-events: none;
  z-index: -1;
}
</style>
