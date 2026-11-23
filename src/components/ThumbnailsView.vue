<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import {
  comic,
  halfOf,
  imageSrc,
  logicalToPhysical,
  pageCount,
  pageIndex,
  setIndex,
  settings,
  thumbnailsOpen,
  thumbSizeScrubbing,
} from "../reader";

const THUMB_GAP_X = 12;

/**
 * Pin the tile nearest the center of the thumbnail list (stage area, not the
 * window — a side panel must not shift this origin) while size or columns change.
 */
type ThumbAnchor = {
  index: number;
  /** Tile center Y minus the list container's visible center Y. */
  fromCenter: number;
};

const root = ref<HTMLElement | null>(null);
const loaded = reactive<Record<number, boolean>>({});
let observer: IntersectionObserver | null = null;
let resizeObserver: ResizeObserver | null = null;
let gestureAnchor: ThumbAnchor | null = null;
let gestureTimer = 0;
let resizeAnchor: ThumbAnchor | null = null;
let resizeTimer = 0;
let restoreFrame = 0;
let applyingSize = false;
let resizeDirty = false;
let lastColumns = 0;

function measureColumns(scroller: HTMLElement, size: number) {
  const styles = getComputedStyle(scroller);
  const pad = (Number.parseFloat(styles.paddingLeft) || 0) + (Number.parseFloat(styles.paddingRight) || 0);
  const available = Math.max(0, scroller.clientWidth - pad);
  const columns = Math.max(1, Math.floor((available + THUMB_GAP_X) / (size + THUMB_GAP_X)));
  const width = Math.min(available, columns * size + Math.max(0, columns - 1) * THUMB_GAP_X);
  return { columns, width };
}

function layoutGrid() {
  const scroller = root.value;
  if (!scroller) return lastColumns;
  const size = Math.max(1, settings.thumbSize);
  const { columns, width } = measureColumns(scroller, size);
  scroller.style.setProperty("--thumb-size", `${size}px`);
  scroller.style.setProperty("--thumb-columns", String(columns));
  scroller.style.setProperty("--thumbs-grid-width", `${width}px`);
  void scroller.offsetHeight;
  lastColumns = columns;
  return columns;
}

function captureAnchor(): ThumbAnchor | null {
  const scroller = root.value;
  if (!scroller) return null;
  const view = scroller.getBoundingClientRect();
  const viewMid = (view.top + view.bottom) / 2;
  let best: ThumbAnchor | null = null;
  let bestDist = Infinity;
  for (const node of scroller.querySelectorAll<HTMLElement>(".thumb[data-index]")) {
    const rect = node.getBoundingClientRect();
    if (rect.bottom <= view.top + 1 || rect.top >= view.bottom - 1) continue;
    const index = Number(node.dataset.index);
    if (!Number.isInteger(index) || index < 0) continue;
    const mid = (rect.top + rect.bottom) / 2;
    const dist = Math.abs(mid - viewMid);
    if (dist >= bestDist) continue;
    bestDist = dist;
    best = { index, fromCenter: mid - viewMid };
  }
  return best;
}

function restoreAnchor(anchor: ThumbAnchor | null) {
  const scroller = root.value;
  if (!scroller || !anchor) return;
  const node = scroller.querySelector<HTMLElement>(`.thumb[data-index="${anchor.index}"]`);
  if (!node) return;
  const view = scroller.getBoundingClientRect();
  const rect = node.getBoundingClientRect();
  const mid = (rect.top + rect.bottom) / 2;
  const target = (view.top + view.bottom) / 2 + anchor.fromCenter;
  const delta = mid - target;
  if (Math.abs(delta) < 0.5) return;
  const max = Math.max(0, scroller.scrollHeight - scroller.clientHeight);
  scroller.scrollTop = Math.min(max, Math.max(0, scroller.scrollTop + delta));
}

function holdGestureAnchor(anchor: ThumbAnchor | null) {
  if (anchor && !gestureAnchor) gestureAnchor = anchor;
  // While scrubbing the size slider, keep the same tile until scrub-end.
  if (thumbSizeScrubbing.value) {
    window.clearTimeout(gestureTimer);
    return;
  }
  window.clearTimeout(gestureTimer);
  gestureTimer = window.setTimeout(() => {
    gestureAnchor = null;
  }, 400);
}

function lockScrubAnchor() {
  window.clearTimeout(gestureTimer);
  if (!gestureAnchor) gestureAnchor = captureAnchor();
}

function unlockScrubAnchor() {
  window.clearTimeout(gestureTimer);
  const anchor = gestureAnchor;
  void nextTick(() => {
    applySizeLayout(anchor);
    // Drop only after the final restore so a late size event still uses it.
    gestureTimer = window.setTimeout(() => {
      if (!thumbSizeScrubbing.value) gestureAnchor = null;
    }, 50);
  });
}

function holdResizeAnchor(anchor: ThumbAnchor | null) {
  if (anchor && !resizeAnchor) resizeAnchor = anchor;
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => {
    resizeAnchor = null;
  }, 200);
}

/** Relayout after list width changes (window / side panel); keep the center tile. */
function onListResize() {
  if (applyingSize) {
    resizeDirty = true;
    return;
  }
  if (!root.value) return;
  // clientWidth is already new, but --thumbs-grid-width still reflects the previous columns.
  if (!gestureAnchor) {
    if (!resizeAnchor) resizeAnchor = captureAnchor();
    holdResizeAnchor(resizeAnchor);
  }
  applySizeLayout(gestureAnchor ?? resizeAnchor);
}

function applySizeLayout(anchor: ThumbAnchor | null) {
  applyingSize = true;
  resizeDirty = false;
  try {
    layoutGrid();
    restoreAnchor(anchor);
    layoutGrid();
    restoreAnchor(anchor);
  } finally {
    applyingSize = false;
  }
  window.cancelAnimationFrame(restoreFrame);
  restoreFrame = window.requestAnimationFrame(() => {
    applyingSize = true;
    try {
      layoutGrid();
      restoreAnchor(anchor);
    } finally {
      applyingSize = false;
    }
    if (resizeDirty) {
      onListResize();
      return;
    }
    const scroller = root.value;
    if (scroller && measureColumns(scroller, Math.max(1, settings.thumbSize)).columns !== lastColumns) {
      onListResize();
      return;
    }
    reconnect();
  });
}

function choose(index: number) {
  setIndex(index, settings.readingMode === "scroll");
  thumbnailsOpen.value = false;
}

function pageName(logical: number) {
  return comic.value?.pages[logicalToPhysical(logical)]?.name ?? "";
}

function markVisible(entries: IntersectionObserverEntry[]) {
  for (const entry of entries) {
    if (!entry.isIntersecting || entry.intersectionRatio <= 0) continue;
    const el = entry.target as HTMLElement;
    const index = Number(el.dataset.index);
    if (!Number.isInteger(index) || index < 0) continue;
    if (!loaded[index]) loaded[index] = true;
    observer?.unobserve(el);
  }
}

function reconnect() {
  observer?.disconnect();
  observer = null;
  const scroller = root.value;
  if (!scroller) return;
  if (scroller.clientHeight < 32) return;
  observer = new IntersectionObserver(markVisible, {
    root: scroller,
    rootMargin: "120px 0px",
    threshold: 0,
  });
  for (const node of scroller.querySelectorAll<HTMLElement>(".thumb[data-index]")) {
    const index = Number(node.dataset.index);
    if (Number.isInteger(index) && loaded[index]) continue;
    observer.observe(node);
  }
}

function scrollCurrent() {
  const node = root.value?.querySelector<HTMLElement>(`.thumb[data-index="${pageIndex.value}"]`);
  node?.scrollIntoView({ block: "center", inline: "nearest" });
}

function setup() {
  void nextTick(() => {
    requestAnimationFrame(() => {
      scrollCurrent();
      reconnect();
    });
  });
}

onMounted(() => {
  const scroller = root.value;
  if (scroller) {
    resizeObserver = new ResizeObserver(() => {
      onListResize();
    });
    resizeObserver.observe(scroller);
    const stage = scroller.parentElement;
    if (stage) resizeObserver.observe(stage);
  }
  layoutGrid();
  setup();
});

onBeforeUnmount(() => {
  observer?.disconnect();
  observer = null;
  resizeObserver?.disconnect();
  resizeObserver = null;
  window.clearTimeout(gestureTimer);
  window.clearTimeout(resizeTimer);
  window.cancelAnimationFrame(restoreFrame);
});

watch(pageCount, setup);

watch(thumbSizeScrubbing, (active) => {
  if (active) lockScrubAnchor();
  else unlockScrubAnchor();
});

watch(
  () => settings.thumbSize,
  () => {
    // Prefer the scrub-locked tile; only re-capture when not in a gesture.
    if (!gestureAnchor) {
      holdGestureAnchor(captureAnchor());
    } else {
      holdGestureAnchor(gestureAnchor);
    }
    const anchor = gestureAnchor;
    void nextTick(() => {
      applySizeLayout(anchor);
    });
  },
  { flush: "pre" },
);

watch(
  () => [settings.thumbShowIndex, settings.thumbShowName] as const,
  () => {
    if (!thumbnailsOpen.value) return;
    const captured = captureAnchor();
    void nextTick(() => {
      applySizeLayout(captured);
    });
  },
);

watch(pageIndex, () => {
  if (!thumbnailsOpen.value) return;
  void nextTick(() => {
    scrollCurrent();
    reconnect();
  });
});
</script>

<template>
  <div ref="root" class="thumbs">
    <div class="thumbs-grid">
      <button
        v-for="index in pageCount"
        :key="index - 1"
        type="button"
        class="thumb"
        :class="{ on: pageIndex === index - 1 }"
        :data-index="index - 1"
        :aria-current="pageIndex === index - 1 ? 'page' : undefined"
        @click="choose(index - 1)"
      >
        <span v-if="settings.thumbShowIndex" class="thumb-num">{{ index }}</span>
        <span
          class="thumb-frame"
          :data-half="halfOf(index - 1)"
          :style="{ background: settings.backgroundColor }"
        >
          <span v-if="loaded[index - 1]" class="thumb-crop">
            <img
              :src="imageSrc(index - 1)"
              :alt="String(index)"
              draggable="false"
              decoding="async"
            />
          </span>
        </span>
        <span v-if="settings.thumbShowName" class="thumb-name" :title="pageName(index - 1)">{{ pageName(index - 1) }}</span>
      </button>
    </div>
  </div>
</template>

<style>
.thumbs {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  overflow-anchor: none;
  overscroll-behavior: contain;
  padding: 14px 16px 20px;
  background: color-mix(in srgb, var(--panel) 35%, #5a5a5a 65%);
}

html.theme-light .thumbs {
  background: color-mix(in srgb, var(--panel) 40%, #9a9a9a 60%);
}

.thumbs-grid {
  /* Column count + width come from JS together. Avoid max-width / auto-fill so a
     narrower list cannot reflow rows before the center-tile anchor is captured. */
  width: var(--thumbs-grid-width, 100%);
  margin-inline: auto;
  display: grid;
  grid-template-columns: repeat(var(--thumb-columns, 4), var(--thumb-size, 128px));
  gap: 14px 12px;
  align-items: start;
  justify-content: start;
}

.thumb {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 5px;
  margin: 0;
  border: 2px solid transparent;
  border-radius: 2px;
  background: transparent;
  color: #f0f0f0;
  cursor: pointer;
}

html.theme-light .thumb {
  color: #1a1a1a;
}

.thumb:hover {
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
  color: inherit;
}

.thumb.on {
  border-color: var(--accent);
}

.thumb-frame {
  width: 100%;
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  background: var(--img);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.28);
}

.thumb-crop {
  display: flex;
  align-items: center;
  justify-content: center;
  max-width: 100%;
  max-height: 100%;
  overflow: hidden;
}

.thumb-frame[data-half="left"] .thumb-crop,
.thumb-frame[data-half="right"] .thumb-crop {
  width: 50%;
  height: 100%;
}

.thumb-frame[data-half="left"] .thumb-crop img {
  width: 200%;
  max-width: none;
  height: auto;
  object-fit: cover;
  object-position: left center;
}

.thumb-frame[data-half="right"] .thumb-crop img {
  width: 200%;
  max-width: none;
  height: auto;
  object-fit: cover;
  object-position: right center;
  margin-left: -100%;
}

.thumb-frame[data-half="full"] .thumb-crop img,
.thumb-frame:not([data-half]) .thumb-crop img {
  display: block;
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
}

.thumb-num,
.thumb-name {
  width: 100%;
  color: inherit;
  text-align: center;
}

.thumb-num {
  font-size: 12px;
  line-height: 1;
  opacity: 0.9;
}

.thumb-name {
  font-size: 11px;
  line-height: 1.3;
  opacity: 0.82;
  overflow: hidden;
  display: -webkit-box;
  line-clamp: 2;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow-wrap: anywhere;
}
</style>
