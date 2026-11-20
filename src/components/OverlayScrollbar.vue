<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { GLIDE_MS, glideEase, reducedMotion } from "../lib/stagePointer";
import { chromeOverlay, pausePlayClock, resetPlayClock, resumePlayClock, settings } from "../reader";

const props = defineProps<{ target: HTMLElement | null }>();

/** Keep a grabable floor; stay close to native (very long docs must not look stubby). */
const MIN_THUMB = 12;
const LINE = 40;
const HOLD_DELAY = 400;
const HOLD_SPEED = 1000;

function cssPx(name: string, fallback: number) {
  const n = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  return Number.isFinite(n) && n > 0 ? n : fallback;
}

function cornerSize() {
  return cssPx("--sb-size", 15);
}

function buttonSize() {
  return cssPx("--sb-btn", 18);
}

const yNeed = ref(false);
const xNeed = ref(false);
const ySize = ref(0);
const xSize = ref(0);
const yOffset = ref(0);
const xOffset = ref(0);
const yAtStart = ref(true);
const yAtEnd = ref(false);
const xAtStart = ref(true);
const xAtEnd = ref(false);
const dragging = ref<"x" | "y" | null>(null);
const pressedArrow = ref<"y-" | "y+" | "x-" | "x+" | null>(null);
const yTrackEl = ref<HTMLElement | null>(null);
const xTrackEl = ref<HTMLElement | null>(null);

let observer: ResizeObserver | null = null;
let mutations: MutationObserver | null = null;
let bound: HTMLElement | null = null;
let flushTracks = false;
let syncRaf = 0;
let holdDelay = 0;
let holdRaf = 0;
let holdCarry = 0;
let holdLast = 0;
let holdAxis: "x" | "y" = "y";
let holdDir: -1 | 1 = 1;
let holdPtr = 0;
let holdPaging = false;
let glideFrame = 0;
let glideGen = 0;

function trackLength(axis: "x" | "y", view: number, otherNeed: boolean) {
  const track = axis === "y" ? yTrackEl.value : xTrackEl.value;
  const measured = track ? (axis === "y" ? track.clientHeight : track.clientWidth) : 0;
  if (measured > 0) return measured;
  const arrows = buttonSize() * 2;
  return Math.max(0, view - arrows - (otherNeed ? cornerSize() : 0));
}

function axisMetrics(el: HTMLElement, axis: "x" | "y") {
  const view = axis === "y" ? el.clientHeight : el.clientWidth;
  const all = axis === "y" ? el.scrollHeight : el.scrollWidth;
  const pos = axis === "y" ? el.scrollTop : el.scrollLeft;
  const max = Math.max(0, all - view);
  const otherNeed =
    axis === "y"
      ? el.scrollWidth - el.clientWidth > 1
      : el.scrollHeight - el.clientHeight > 1;
  const track = trackLength(axis, view, otherNeed);
  const need = max > 1;
  const size = need && all > 0 ? Math.min(track, Math.max(MIN_THUMB, (view / all) * track)) : 0;
  const offset = need && max > 0 && track > size ? (pos / max) * (track - size) : 0;
  return { need, size, offset, atStart: pos <= 0.5, atEnd: pos >= max - 0.5 };
}

function markHost(el: HTMLElement | null, y: boolean, x: boolean) {
  const host = el?.closest(".overlay-scroll-host");
  if (!host) return;
  host.classList.toggle("has-y-scroll", y);
  host.classList.toggle("has-x-scroll", x);
}

function syncNow() {
  const el = props.target;
  if (!el) {
    yNeed.value = false;
    xNeed.value = false;
    markHost(bound, false, false);
    return;
  }
  const y = axisMetrics(el, "y");
  const x = axisMetrics(el, "x");
  const needChanged = yNeed.value !== y.need || xNeed.value !== x.need;
  yNeed.value = y.need;
  xNeed.value = x.need;
  ySize.value = y.size;
  xSize.value = x.size;
  yOffset.value = y.offset;
  xOffset.value = x.offset;
  yAtStart.value = y.atStart;
  yAtEnd.value = y.atEnd;
  xAtStart.value = x.atStart;
  xAtEnd.value = x.atEnd;
  markHost(el, y.need, x.need);
  if (needChanged && !flushTracks) {
    flushTracks = true;
    void nextTick(() => {
      flushTracks = false;
      scheduleSync();
    });
  }
}

/** Coalesce scroll / resize / mutation sync to one update per frame. */
function scheduleSync() {
  if (syncRaf) return;
  syncRaf = requestAnimationFrame(() => {
    syncRaf = 0;
    syncNow();
  });
}

function sync() {
  scheduleSync();
}

function bind(el: HTMLElement | null) {
  observer?.disconnect();
  mutations?.disconnect();
  if (syncRaf) {
    cancelAnimationFrame(syncRaf);
    syncRaf = 0;
  }
  if (bound) bound.removeEventListener("scroll", sync);
  markHost(bound, false, false);
  bound = el;
  if (!el) {
    syncNow();
    return;
  }
  observer = new ResizeObserver(sync);
  observer.observe(el);
  for (const child of el.children) observer.observe(child);
  mutations = new MutationObserver(() => {
    for (const child of el.children) observer?.observe(child);
    sync();
  });
  mutations.observe(el, { childList: true });
  el.addEventListener("scroll", sync, { passive: true });
  sync();
}

function stopGlide() {
  window.cancelAnimationFrame(glideFrame);
  glideFrame = 0;
  glideGen += 1;
}

function animateScroll(el: HTMLElement, axis: "x" | "y", delta: number, onDone?: () => void) {
  const from = axis === "y" ? el.scrollTop : el.scrollLeft;
  const view = axis === "y" ? el.clientHeight : el.clientWidth;
  const all = axis === "y" ? el.scrollHeight : el.scrollWidth;
  const to = Math.max(0, Math.min(Math.max(0, all - view), from + delta));
  if (Math.abs(to - from) < 1 || reducedMotion() || !settings.smoothScroll) {
    if (axis === "y") el.scrollTop = to;
    else el.scrollLeft = to;
    onDone?.();
    return;
  }
  stopGlide();
  const gen = glideGen;
  const started = performance.now();
  const frame = () => {
    if (gen !== glideGen) return;
    const eased = glideEase((performance.now() - started) / GLIDE_MS);
    const pos = from + (to - from) * eased;
    if (axis === "y") el.scrollTop = pos;
    else el.scrollLeft = pos;
    if (eased < 1) {
      glideFrame = window.requestAnimationFrame(frame);
      return;
    }
    glideFrame = 0;
    onDone?.();
  };
  glideFrame = window.requestAnimationFrame(frame);
}

function stopHold() {
  window.clearTimeout(holdDelay);
  window.cancelAnimationFrame(holdRaf);
  holdDelay = 0;
  holdRaf = 0;
  holdCarry = 0;
  holdPaging = false;
  pressedArrow.value = null;
  window.removeEventListener("pointerup", stopHold);
  window.removeEventListener("pointercancel", stopHold);
  window.removeEventListener("pointermove", onHoldMove);
  resumePlayClock();
}

function onHoldMove(event: PointerEvent) {
  holdPtr = holdAxis === "y" ? event.clientY : event.clientX;
}

function atBound(axis: "x" | "y", dir: -1 | 1) {
  if (axis === "y") return dir < 0 ? yAtStart.value : yAtEnd.value;
  return dir < 0 ? xAtStart.value : xAtEnd.value;
}

function holdFrame(now: number) {
  const el = props.target;
  if (!el || atBound(holdAxis, holdDir)) {
    stopHold();
    return;
  }
  const dt = Math.min(32, now - holdLast);
  holdLast = now;
  holdCarry += HOLD_SPEED * holdDir * (dt / 1000);
  const step = holdCarry > 0 ? Math.floor(holdCarry) : Math.ceil(holdCarry);
  if (step) {
    holdCarry -= step;
    if (holdAxis === "y") el.scrollTop += step;
    else el.scrollLeft += step;
  }
  holdRaf = window.requestAnimationFrame(holdFrame);
}

function onArrowDown(axis: "x" | "y", dir: -1 | 1, event: PointerEvent) {
  if (event.button !== 0 || atBound(axis, dir)) return;
  const el = props.target;
  if (!el) return;
  event.preventDefault();
  event.stopPropagation();
  stopHold();
  pausePlayClock();
  pressedArrow.value = axis === "y" ? (dir < 0 ? "y-" : "y+") : dir < 0 ? "x-" : "x+";
  holdAxis = axis;
  holdDir = dir;
  animateScroll(el, axis, LINE * dir);
  window.addEventListener("pointerup", stopHold);
  window.addEventListener("pointercancel", stopHold);
  holdDelay = window.setTimeout(() => {
    stopGlide();
    holdLast = performance.now();
    holdCarry = 0;
    holdRaf = window.requestAnimationFrame(holdFrame);
  }, HOLD_DELAY);
}

function onThumbDown(axis: "x" | "y", event: PointerEvent) {
  if (event.button !== 0) return;
  const el = props.target;
  if (!el) return;
  event.preventDefault();
  event.stopPropagation();
  stopHold();
  stopGlide();
  pausePlayClock();
  dragging.value = axis;
  const startPtr = axis === "y" ? event.clientY : event.clientX;
  const startScroll = axis === "y" ? el.scrollTop : el.scrollLeft;
  const view = axis === "y" ? el.clientHeight : el.clientWidth;
  const all = axis === "y" ? el.scrollHeight : el.scrollWidth;
  const max = Math.max(1, all - view);
  const size = axis === "y" ? ySize.value : xSize.value;
  const otherNeed = axis === "y" ? xNeed.value : yNeed.value;
  const track = Math.max(1, trackLength(axis, view, otherNeed));
  const span = Math.max(1, track - size);
  const onMove = (next: PointerEvent) => {
    const delta = (axis === "y" ? next.clientY : next.clientX) - startPtr;
    const scroll = startScroll + (delta / span) * max;
    if (axis === "y") el.scrollTop = scroll;
    else el.scrollLeft = scroll;
  };
  const onUp = () => {
    dragging.value = null;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onUp);
    resumePlayClock();
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onUp);
}

function pageToward(axis: "x" | "y", dir: -1 | 1) {
  const el = props.target;
  const track = axis === "y" ? yTrackEl.value : xTrackEl.value;
  if (!el || !track || atBound(axis, dir)) {
    stopHold();
    return;
  }
  const rect = track.getBoundingClientRect();
  const ptr = holdPtr - (axis === "y" ? rect.top : rect.left);
  const start = axis === "y" ? yOffset.value : xOffset.value;
  const size = axis === "y" ? ySize.value : xSize.value;
  if (dir < 0 && ptr >= start) {
    stopHold();
    return;
  }
  if (dir > 0 && ptr <= start + size) {
    stopHold();
    return;
  }
  const view = axis === "y" ? el.clientHeight : el.clientWidth;
  animateScroll(el, axis, view * dir, () => {
    if (holdPaging) pageToward(axis, dir);
  });
}

function onTrackDown(axis: "x" | "y", event: PointerEvent) {
  if (event.button !== 0 || event.target !== event.currentTarget) return;
  if (settings.scrollTrack === "off") return;
  const el = props.target;
  const track = event.currentTarget;
  if (!el || !(track instanceof HTMLElement)) return;
  event.preventDefault();
  const rect = track.getBoundingClientRect();
  const ptr = axis === "y" ? event.clientY - rect.top : event.clientX - rect.left;
  const start = axis === "y" ? yOffset.value : xOffset.value;
  const size = axis === "y" ? ySize.value : xSize.value;
  if (ptr >= start && ptr <= start + size) return;
  if (settings.scrollTrack === "jump") {
    const view = axis === "y" ? el.clientHeight : el.clientWidth;
    const all = axis === "y" ? el.scrollHeight : el.scrollWidth;
    const max = Math.max(1, all - view);
    const ratio = rect.height > 0 && axis === "y"
      ? ptr / rect.height
      : rect.width > 0
        ? ptr / rect.width
        : 0;
    const next = Math.min(max, Math.max(0, ratio * max - view / 2));
    if (axis === "y") el.scrollTop = next;
    else el.scrollLeft = next;
    resetPlayClock();
    return;
  }
  const dir: -1 | 1 = ptr < start ? -1 : 1;
  stopHold();
  pausePlayClock();
  holdAxis = axis;
  holdDir = dir;
  holdPtr = axis === "y" ? event.clientY : event.clientX;
  holdPaging = true;
  pageToward(axis, dir);
  window.addEventListener("pointerup", stopHold);
  window.addEventListener("pointercancel", stopHold);
  window.addEventListener("pointermove", onHoldMove);
}

function arrowKey(event: KeyboardEvent): { axis: "x" | "y"; dir: -1 | 1 } | null {
  if (event.key === "ArrowDown") return { axis: "y", dir: 1 };
  if (event.key === "ArrowUp") return { axis: "y", dir: -1 };
  if (event.key === "ArrowRight") return { axis: "x", dir: 1 };
  if (event.key === "ArrowLeft") return { axis: "x", dir: -1 };
  return null;
}

function onKeyDown(event: KeyboardEvent) {
  if (event.repeat || event.altKey || event.ctrlKey || event.metaKey) return;
  const el = props.target;
  if (!el) return;
  const target = event.target;
  if (target instanceof HTMLElement) {
    const tag = target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable) return;
  }
  if (document.querySelector(".app-modal-backdrop")) return;
  const key = arrowKey(event);
  if (!key) return;
  const can =
    key.axis === "y" ? el.scrollHeight - el.clientHeight > 1 : el.scrollWidth - el.clientWidth > 1;
  if (!can || atBound(key.axis, key.dir)) return;
  event.preventDefault();
  stopHold();
  pausePlayClock();
  holdAxis = key.axis;
  holdDir = key.dir;
  animateScroll(el, key.axis, LINE * key.dir);
  holdDelay = window.setTimeout(() => {
    stopGlide();
    holdLast = performance.now();
    holdCarry = 0;
    holdRaf = window.requestAnimationFrame(holdFrame);
  }, HOLD_DELAY);
}

function onKeyUp(event: KeyboardEvent) {
  if (!arrowKey(event)) return;
  stopHold();
}

watch(() => props.target, bind, { immediate: true });
watch(chromeOverlay, sync);

onMounted(() => {
  window.addEventListener("keydown", onKeyDown);
  window.addEventListener("keyup", onKeyUp);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeyDown);
  window.removeEventListener("keyup", onKeyUp);
  stopHold();
  stopGlide();
  bind(null);
});
</script>

<template>
  <div
    v-show="yNeed"
    class="overlay-sb overlay-sb-y"
    :class="{ inset: xNeed, dragging: dragging === 'y' }"
  >
    <div
      class="overlay-sb-arrow overlay-sb-arrow-up"
      :class="{ pressed: pressedArrow === 'y-', disabled: yAtStart }"
      @pointerdown="onArrowDown('y', -1, $event)"
    >
      <svg class="overlay-sb-chevron" viewBox="0 0 9 9" aria-hidden="true">
        <path d="M0 7.5 L9 7.5 L4.5 2.5 Z" />
      </svg>
    </div>
    <div ref="yTrackEl" class="overlay-sb-track" @pointerdown="onTrackDown('y', $event)">
      <div
        class="overlay-sb-thumb"
        :style="{ height: `${ySize}px`, transform: `translateY(${yOffset}px)` }"
        @pointerdown="onThumbDown('y', $event)"
      />
    </div>
    <div
      class="overlay-sb-arrow overlay-sb-arrow-down"
      :class="{ pressed: pressedArrow === 'y+', disabled: yAtEnd }"
      @pointerdown="onArrowDown('y', 1, $event)"
    >
      <svg class="overlay-sb-chevron" viewBox="0 0 9 9" aria-hidden="true">
        <path d="M0 7.5 L9 7.5 L4.5 2.5 Z" />
      </svg>
    </div>
  </div>
  <div
    v-show="xNeed"
    class="overlay-sb overlay-sb-x"
    :class="{ inset: yNeed, dragging: dragging === 'x' }"
  >
    <div
      class="overlay-sb-arrow overlay-sb-arrow-left"
      :class="{ pressed: pressedArrow === 'x-', disabled: xAtStart }"
      @pointerdown="onArrowDown('x', -1, $event)"
    >
      <svg class="overlay-sb-chevron" viewBox="0 0 9 9" aria-hidden="true">
        <path d="M0 7.5 L9 7.5 L4.5 2.5 Z" />
      </svg>
    </div>
    <div ref="xTrackEl" class="overlay-sb-track" @pointerdown="onTrackDown('x', $event)">
      <div
        class="overlay-sb-thumb"
        :style="{ width: `${xSize}px`, transform: `translateX(${xOffset}px)` }"
        @pointerdown="onThumbDown('x', $event)"
      />
    </div>
    <div
      class="overlay-sb-arrow overlay-sb-arrow-right"
      :class="{ pressed: pressedArrow === 'x+', disabled: xAtEnd }"
      @pointerdown="onArrowDown('x', 1, $event)"
    >
      <svg class="overlay-sb-chevron" viewBox="0 0 9 9" aria-hidden="true">
        <path d="M0 7.5 L9 7.5 L4.5 2.5 Z" />
      </svg>
    </div>
  </div>
  <div v-show="yNeed && xNeed" class="overlay-sb overlay-sb-corner" />
</template>

<style>
.overlay-sb {
  position: absolute;
  pointer-events: auto;
  display: flex;
  background: var(--sb-track);
}

.overlay-sb-y {
  top: 0;
  right: 0;
  bottom: 0;
  width: var(--sb-size);
  flex-direction: column;
}

.overlay-sb-y.inset {
  bottom: var(--sb-size);
}

.overlay-sb-x {
  left: 0;
  right: 0;
  bottom: 0;
  height: var(--sb-size);
  flex-direction: row;
}

.overlay-sb-x.inset {
  right: var(--sb-size);
}

.overlay-sb-corner {
  right: 0;
  bottom: 0;
  width: var(--sb-size);
  height: var(--sb-size);
  pointer-events: none;
}

.overlay-sb-track {
  position: relative;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
}

.overlay-sb-y .overlay-sb-arrow {
  flex: 0 0 var(--sb-btn);
  align-self: stretch;
  height: var(--sb-btn);
}

.overlay-sb-x .overlay-sb-arrow {
  flex: 0 0 var(--sb-btn);
  align-self: stretch;
  width: var(--sb-btn);
}

.overlay-sb-arrow {
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--sb-arrow-fg);
  cursor: default;
  appearance: none;
}

.overlay-sb-chevron {
  width: var(--sb-arrow);
  height: var(--sb-arrow);
  display: block;
  overflow: visible;
}

.overlay-sb-chevron path {
  fill: currentColor;
}

.overlay-sb-arrow-down .overlay-sb-chevron {
  transform: rotate(180deg);
}

.overlay-sb-arrow-left .overlay-sb-chevron {
  transform: rotate(-90deg);
}

.overlay-sb-arrow-right .overlay-sb-chevron {
  transform: rotate(90deg);
}

.overlay-sb-arrow:hover,
.overlay-sb-arrow.pressed {
  color: var(--sb-arrow-hover);
}

.overlay-sb-arrow.disabled {
  opacity: 0.35;
  pointer-events: none;
}

.overlay-sb-arrow.disabled:hover,
.overlay-sb-arrow.disabled.pressed {
  color: var(--sb-arrow-fg);
}

.overlay-sb-arrow.pressed .overlay-sb-chevron {
  width: 7px;
  height: 7px;
}

.overlay-sb-thumb {
  position: absolute;
  background: var(--sb-thumb);
  border-radius: 999px;
}

.overlay-sb-y .overlay-sb-thumb {
  left: 0;
  right: 0;
  width: 9px;
  margin-inline: auto;
}

.overlay-sb-x .overlay-sb-thumb {
  top: 0;
  bottom: 0;
  height: 9px;
  margin-block: auto;
}

.overlay-sb-thumb:hover,
.overlay-sb.dragging .overlay-sb-thumb {
  background: var(--sb-thumb-hover);
}
</style>
