<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import Icon from "./Icon.vue";
import BarMenu from "./BarMenu.vue";
import RangeSlider from "./RangeSlider.vue";
import SwitchToggle from "./SwitchToggle.vue";
import { hideAppTip, showAppTip } from "../lib/appTip";
import {
  canShowFullscreenPanel,
  chromeBottom,
  chromeEdgeRevealBlockedByOverlay,
  chromeOverlay,
  fullscreen,
  FULLSCREEN_CHROME_ZONE,
  FULLSCREEN_RIGHT_GUTTER,
  globalBusy,
  pageBookmarked,
  togglePageBookmark,
  pageCount,
  pageIndex,
  pageCurrentLabel,
  pausePlayClock,
  playing,
  progressIndex,
  measureProgress,
  showMeasureBar,
  setIndex,
  settings,
  modeFit,
  type FitMode,
  getPageRotation,
  openArchiveDialog,
  resumePlayClock,
  rotateGuide,
  rotatePageClockwise,
  rotatePageCounterClockwise,
  normalizeFitFixedWidth,
  normalizeFitOriginalScale,
  hideFullscreenSidebar,
  showChapters,
  showFullscreenSidebar,
  showBookmarks,
  sideOpen,
  sideTab,
  thumbnailsOpen,
  toggleFullscreen,
  toggleMinimalist,
  togglePlay,
  toggleThumbnails,
  tipWithShortcut,
  THUMB_SIZE_MAX,
  THUMB_SIZE_MIN,
  beginThumbSizeGesture,
  endThumbSizeGesture,
  turn,
  ocrEnabled,
  translationSeekGradient,
  extractSeekGradient,
  showExtractBar,
} from "../reader";
import { maskModalOpen } from "../lib/maskModal";
import { neuralSeekGradient } from "../lib/render/neural";
import { isNeuralProfile } from "../lib/render/profile";
const { t } = useI18n();

const emptySeek = "linear-gradient(to right, transparent, transparent)";
const neuralProgressOn = computed(() => isNeuralProfile(settings.renderProfile));
const translateProgressOn = computed(() => ocrEnabled.value);
const extractProgressOn = computed(() => showExtractBar.value);

const modes = ["single", "double", "scroll"] as const;
const pageFits = ["window", "auto", "original", "fixedWidth", "originalScale"] as const satisfies readonly FitMode[];
const scrollFits = ["auto", "original", "fixedWidth", "originalScale"] as const satisfies readonly FitMode[];
const fitLabel = {
  auto: "fitAuto",
  window: "fitWindow",
  original: "fitOriginal",
  fixedWidth: "fitFixedWidth",
  originalScale: "fitOriginalScale",
} as const;

function labelForFit(fit: FitMode) {
  if (fit === "auto" && settings.readingMode === "scroll") return t("fitWidth");
  return t(fitLabel[fit]);
}
const directions = ["ltr", "rtl"] as const;
const modeIcon = { scroll: "modeScroll", single: "modeSingle", double: "modeDouble" } as const;
const directionIcon = { ltr: "dirLtr", rtl: "dirRtl" } as const;
const fitIcon = {
  auto: "fitCover",
  window: "fitContain",
  original: "fitOriginal",
  fixedWidth: "fitFixedWidth",
  originalScale: "fitOriginalScale",
} as const;
type PopupName = "mode" | "fit" | "direction";
const openPopup = ref<PopupName | null>(null);

const currentRotation = computed(() => getPageRotation(pageIndex.value));
const chromeSliding = ref(false);
const CHROME_SLIDE_MS = 140;
let chromeSlideTimer = 0;

watch(thumbnailsOpen, (open) => {
  if (open) closePopups();
});

const chromeBlocked = computed(() => maskModalOpen.value || globalBusy.value);

watch(chromeBlocked, (blocked) => {
  if (blocked) closePopups();
});

watch(chromeBottom, () => {
  if (!chromeOverlay.value) {
    chromeSliding.value = false;
    window.clearTimeout(chromeSlideTimer);
    return;
  }
  chromeSliding.value = true;
  if (!scrubbing.value) hideAppTip();
  window.clearTimeout(chromeSlideTimer);
  chromeSlideTimer = window.setTimeout(() => {
    chromeSliding.value = false;
  }, CHROME_SLIDE_MS);
});

function inRightGutter(x: number) {
  return x >= window.innerWidth - FULLSCREEN_RIGHT_GUTTER;
}

function onPointerMove(event: PointerEvent) {
  if (!chromeOverlay.value) return;
  const target = event.target;
  const inDock = target instanceof Element && !!target.closest(".dock");
  if (inDock) {
    chromeBottom.value = true;
    return;
  }
  if (chromeBottom.value) {
    const keep =
      window.innerHeight - event.clientY <= FULLSCREEN_CHROME_ZONE
      && !inRightGutter(event.clientX);
    if (!keep) chromeBottom.value = false;
    return;
  }
  if (chromeEdgeRevealBlockedByOverlay()) return;
  if (event.buttons !== 0) return;
  if (
    window.innerHeight - event.clientY <= FULLSCREEN_CHROME_ZONE
    && !inRightGutter(event.clientX)
    && canShowFullscreenPanel("footer")
  ) {
    chromeBottom.value = true;
  }
}

function onPointerOut(event: PointerEvent) {
  if (!chromeOverlay.value || event.relatedTarget) return;
  if (scrubbing.value) return;
  chromeBottom.value = false;
}
const hideTimers = new Map<PopupName, number>();

const scrubbing = ref(false);
const editingFitParam = ref(false);
const fitParamText = ref("");
const fitParamInput = ref<HTMLInputElement | null>(null);
const editingPage = ref(false);
const pageText = ref("");
const pageInput = ref<HTMLInputElement | null>(null);

const fit = computed(() => modeFit());

const fits = computed(() =>
  settings.readingMode === "scroll" ? scrollFits : pageFits,
);

const activeFit = computed(() =>
  settings.readingMode === "scroll" && fit.value.fitMode === "window"
    ? "auto"
    : fit.value.fitMode,
);

const showFitParam = computed(() =>
  fit.value.fitMode === "fixedWidth" || fit.value.fitMode === "originalScale",
);

const fitParamLabel = computed(() => {
  if (fit.value.fitMode === "fixedWidth") return String(fit.value.fitFixedWidth);
  return String(Math.round(fit.value.fitOriginalScale * 100));
});

const fitParamSuffix = computed(() => (fit.value.fitMode === "fixedWidth" ? "px" : "%"));

function chooseFit(next: FitMode) {
  const current = modeFit();
  current.fitMode = settings.readingMode === "scroll" && next === "window" ? "auto" : next;
}

const pageShown = computed(() => (pageCount.value > 0 ? pageIndex.value + 1 : 0));
/** Include double-page labels like `117,118`, not only total-page digit width. */
const pageDigits = computed(() =>
  Math.max(1, String(Math.max(0, pageCount.value)).length, pageCurrentLabel.value.length),
);

function applyFitParam(raw: string) {
  const value = Number.parseInt(raw, 10);
  if (!Number.isInteger(value)) return;
  const current = modeFit();
  if (current.fitMode === "fixedWidth") {
    current.fitFixedWidth = normalizeFitFixedWidth(value);
  } else if (current.fitMode === "originalScale") {
    current.fitOriginalScale = normalizeFitOriginalScale(value / 100);
  }
}

function beginFitParam() {
  fitParamText.value = fitParamLabel.value;
  editingFitParam.value = true;
  void nextTick(() => {
    fitParamInput.value?.focus();
    fitParamInput.value?.select();
  });
}

function applyFitParamFromEvent(event?: Event) {
  if (!editingFitParam.value) return;
  const raw = event?.target instanceof HTMLInputElement ? event.target.value : fitParamText.value;
  applyFitParam(String(raw));
}

function commitFitParam(event?: Event) {
  applyFitParamFromEvent(event);
  editingFitParam.value = false;
}

function cancelFitParam() {
  editingFitParam.value = false;
}

function onFitParamKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    cancelFitParam();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    commitFitParam(event);
    return;
  }
  if (event.key === "ArrowUp" || event.key === "ArrowDown") {
    void nextTick(() => {
      if (event.target instanceof HTMLInputElement) applyFitParam(event.target.value);
    });
  }
}

function applyPage(raw: string) {
  const value = Number.parseInt(raw, 10);
  if (!Number.isInteger(value) || value < 1) return;
  setIndex(value - 1, settings.readingMode === "scroll");
}

function beginPage() {
  if (pageCount.value <= 0) return;
  pageText.value = String(pageShown.value);
  editingPage.value = true;
  void nextTick(() => {
    pageInput.value?.focus();
    pageInput.value?.select();
  });
}

function applyPageFromEvent(event?: Event) {
  if (!editingPage.value) return;
  const raw = event?.target instanceof HTMLInputElement ? event.target.value : pageText.value;
  applyPage(String(raw));
}

function commitPage(event?: Event) {
  applyPageFromEvent(event);
  editingPage.value = false;
}

function cancelPage() {
  editingPage.value = false;
}

function onPageKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    cancelPage();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    commitPage(event);
    return;
  }
  if (event.key === "ArrowUp" || event.key === "ArrowDown") {
    void nextTick(() => {
      if (event.target instanceof HTMLInputElement) applyPage(event.target.value);
    });
  }
}

watch(() => [settings.readingMode, fit.value.fitMode] as const, () => {
  editingFitParam.value = false;
});

function showPopup(name: PopupName) {
  if (chromeBlocked.value) return;
  for (const [key, timer] of hideTimers) {
    window.clearTimeout(timer);
    hideTimers.delete(key);
  }
  openPopup.value = name;
  void nextTick(() => {
      for (const menu of document.querySelectorAll<HTMLElement>(".toolbar .bar-menu")) {
      if (getComputedStyle(menu).display === "none") continue;
      menu.style.marginLeft = "";
      const rect = menu.getBoundingClientRect();
      const pad = 4;
      let shift = 0;
      if (rect.left < pad) shift = pad - rect.left;
      else if (rect.right > window.innerWidth - pad) shift = window.innerWidth - pad - rect.right;
      menu.style.marginLeft = shift ? `${shift}px` : "";
    }
  });
}

function scheduleHidePopup(name: PopupName) {
  const existing = hideTimers.get(name);
  if (existing !== undefined) window.clearTimeout(existing);
  hideTimers.set(name, window.setTimeout(() => {
    hideTimers.delete(name);
    if (openPopup.value === name) openPopup.value = null;
  }, 280));
}

function closePopups() {
  for (const timer of hideTimers.values()) window.clearTimeout(timer);
  hideTimers.clear();
  openPopup.value = null;
}

function onPointerDownOutside(event: PointerEvent) {
  if (!openPopup.value) return;
  const dock = document.querySelector(".dock");
  if (dock && event.target instanceof Node && dock.contains(event.target)) return;
  closePopups();
}

function chooseMode(mode: (typeof modes)[number]) {
  settings.readingMode = mode;
}

function setRotateGuide(on: boolean) {
  rotateGuide.value = on;
}

function toggleSide(tab: "bookmarks" | "chapters") {
  if (chromeOverlay.value) {
    if (showFullscreenSidebar.value && sideTab.value === tab) {
      hideFullscreenSidebar();
      return;
    }
  } else if (sideOpen.value && sideTab.value === tab) {
    sideOpen.value = false;
    return;
  }
  if (tab === "bookmarks") showBookmarks();
  else showChapters();
}

function onSlider(event: Event) {
  const input = event.target;
  if (!(input instanceof HTMLInputElement)) return;
  setIndex(Number(input.value) - 1, settings.readingMode === "scroll", "auto");
}

function seekRatio(clientX: number, row: HTMLElement) {
  const rect = row.getBoundingClientRect();
  if (rect.width <= 0 || pageCount.value <= 0) return null;
  return {
    rect,
    ratio: Math.min(1, Math.max(0, (clientX - rect.left) / rect.width)),
  };
}

function seekTo(clientX: number, row: HTMLElement) {
  const point = seekRatio(clientX, row);
  if (!point) return;
  setIndex(Math.round(point.ratio * (pageCount.value - 1)), settings.readingMode === "scroll", "auto");
}

function placeTip(clientX: number, row: HTMLElement) {
  const point = seekRatio(clientX, row);
  if (!point) {
    hideAppTip();
    return;
  }
  const page = Math.round(point.ratio * (pageCount.value - 1));
  showAppTip({
    text: `${page + 1} / ${pageCount.value}`,
    x: clientX,
    y: point.rect.top,
    placement: "above",
    gap: 8,
  });
}

function showIconTip(button: HTMLElement) {
  const text = button.getAttribute("data-tip");
  if (!text) {
    hideAppTip();
    return;
  }
  const rect = button.getBoundingClientRect();
  const toolbar = button.closest(".toolbar");
  const top =
    toolbar instanceof HTMLElement ? toolbar.getBoundingClientRect().top : rect.top;
  showAppTip({
    text,
    x: rect.left + rect.width / 2,
    y: top,
    placement: "above",
    gap: 0,
  });
}

function onToolbarTip(event: PointerEvent) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest(".thumbs-size")) return;
  const button = target.closest("button.icon[data-tip]");
  if (!(button instanceof HTMLElement)) {
    if (!scrubbing.value) hideAppTip();
    return;
  }
  if (event.relatedTarget instanceof Node && button.contains(event.relatedTarget)) return;
  showIconTip(button);
}

function clampHoveredTip() {
  const button = document.querySelector("button.icon[data-tip]:hover");
  if (button instanceof HTMLElement) showIconTip(button);
}

function hideTip() {
  if (scrubbing.value) return;
  hideAppTip();
}

function pointerInsideSeek(event: PointerEvent, row: HTMLElement) {
  const rect = row.getBoundingClientRect();
  return (
    event.clientX >= rect.left &&
    event.clientX <= rect.right &&
    event.clientY >= rect.top - 4 &&
    event.clientY <= rect.bottom + 4
  );
}

function onSeekDown(event: PointerEvent) {
  if (pageCount.value <= 0 || chromeSliding.value) return;
  if (event.button !== 0) return;
  const row = event.currentTarget;
  if (!(row instanceof HTMLElement)) return;
  scrubbing.value = true;
  pausePlayClock();
  placeTip(event.clientX, row);
  seekTo(event.clientX, row);
  try {
    row.setPointerCapture(event.pointerId);
  } catch {
    // A lost pointer has nothing to capture.
  }
}

function endSeek(event: PointerEvent) {
  scrubbing.value = false;
  resumePlayClock();
  const row = event.currentTarget;
  if (!(row instanceof HTMLElement) || !pointerInsideSeek(event, row)) hideAppTip();
}

function onSeekMove(event: PointerEvent) {
  const row = event.currentTarget;
  if (!(row instanceof HTMLElement)) return;
  if (pageCount.value <= 0) return;
  if (chromeSliding.value && !scrubbing.value) {
    hideAppTip();
    return;
  }
  placeTip(event.clientX, row);
  if (!row.hasPointerCapture(event.pointerId)) return;
  seekTo(event.clientX, row);
}

onMounted(() => {
  window.addEventListener("resize", clampHoveredTip);
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerout", onPointerOut);
  document.addEventListener("pointerdown", onPointerDownOutside);
});
onBeforeUnmount(() => {
  window.removeEventListener("resize", clampHoveredTip);
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerout", onPointerOut);
  document.removeEventListener("pointerdown", onPointerDownOutside);
  closePopups();
  hideAppTip();
  rotateGuide.value = false;
  window.clearTimeout(chromeSlideTimer);
});

const progress = computed(() => {
  if (pageCount.value <= 1) return "0%";
  return `${(progressIndex.value / (pageCount.value - 1)) * 100}%`;
});

const showSeek = computed(() => {
  if (!chromeOverlay.value) return true;
  return fullscreen.value ? settings.fullscreenShowSeek : settings.minimalistShowSeek;
});

</script>

<template>
  <footer
    class="dock"
    :class="{
      reveal: chromeOverlay && chromeBottom,
      sliding: chromeSliding,
      'menu-open': !!openPopup,
      'seek-follow': chromeOverlay && !showSeek,
    }"
  >
    <div
      class="toolbar-seek"
      :class="{
        scrubbing,
        measuring: showMeasureBar,
        'neural-progress': neuralProgressOn,
        'translate-progress': translateProgressOn,
        'extract-progress': extractProgressOn,
        disabled: pageCount <= 0,
      }"
      :style="{
        '--progress': progress,
        '--measure': showMeasureBar ? measureProgress : '0%',
        '--extract-seek': extractSeekGradient === 'none' ? emptySeek : extractSeekGradient,
        '--neural-seek': neuralSeekGradient === 'none' ? emptySeek : neuralSeekGradient,
        '--translate-seek':
          translationSeekGradient === 'none' ? emptySeek : translationSeekGradient,
      }"
      @pointerdown="onSeekDown"
      @pointermove="onSeekMove"
      @pointerup="endSeek"
      @pointercancel="endSeek"
      @pointerleave="hideTip"
    >
      <input
        class="slider"
        type="range"
        :min="1"
        :max="Math.max(1, pageCount)"
        :value="progressIndex + 1"
        :disabled="pageCount <= 0"
        @input="onSlider"
      />
    </div>
    <div class="chrome-bottom">
      <div class="toolbar" @pointerenter="hideTip" @pointerover="onToolbarTip" @pointerleave="hideAppTip">
    <div class="toolbar-bar" :class="{ 'thumbs-bar': thumbnailsOpen }">
      <div class="toolbar-side" :class="{ 'thumbs-toggles': thumbnailsOpen }">
        <button
          type="button"
          class="icon"
          :disabled="chromeOverlay"
          :data-tip="chromeOverlay ? t('panel') : tipWithShortcut(t('panel'), 'panel')"
          :aria-label="chromeOverlay ? t('panel') : tipWithShortcut(t('panel'), 'panel')"
          :class="{ on: !chromeOverlay && sideOpen }"
          @click="toggleSide(sideTab)"
        ><Icon name="panelOn" /></button>
        <template v-if="thumbnailsOpen">
          <span class="thumb-toggle">
            <span>{{ t("thumbIndex") }}</span>
            <SwitchToggle v-model="settings.thumbShowIndex" size="sm" :aria-label="t('thumbIndex')" />
          </span>
          <span class="thumb-toggle">
            <span>{{ t("thumbFileName") }}</span>
            <SwitchToggle v-model="settings.thumbShowName" size="sm" :aria-label="t('thumbFileName')" />
          </span>
        </template>
        <template v-else>
        <div class="menu-wrap" @mouseenter="showPopup('mode')" @mouseleave="scheduleHidePopup('mode')">
          <button type="button" class="icon" aria-haspopup="listbox" :aria-expanded="openPopup === 'mode'"><Icon :name="modeIcon[settings.readingMode]" /></button>
          <BarMenu v-show="openPopup === 'mode'" placement="up" role="listbox" @mouseenter="showPopup('mode')" @mouseleave="scheduleHidePopup('mode')">
            <button
              v-for="mode in modes"
              :key="mode"
              type="button"
              class="with-icon"
              role="option"
              :class="{ on: settings.readingMode === mode }"
              :aria-selected="settings.readingMode === mode"
              @click="chooseMode(mode)"
            ><Icon :name="modeIcon[mode]" /><span>{{ t(mode) }}</span></button>
          </BarMenu>
        </div>
        <div class="cluster">
          <div class="menu-wrap" @mouseenter="showPopup('direction')" @mouseleave="scheduleHidePopup('direction')">
            <button type="button" class="icon" aria-haspopup="listbox" :aria-expanded="openPopup === 'direction'"><Icon :name="directionIcon[fit.direction]" /></button>
            <BarMenu v-show="openPopup === 'direction'" placement="up" role="listbox" @mouseenter="showPopup('direction')" @mouseleave="scheduleHidePopup('direction')">
              <button
                v-for="direction in directions"
                :key="direction"
                type="button"
                class="with-icon"
                role="option"
                :class="{ on: fit.direction === direction }"
                :aria-selected="fit.direction === direction"
                @click="modeFit().direction = direction"
              ><Icon :name="directionIcon[direction]" /><span>{{ t(direction) }}</span></button>
            </BarMenu>
          </div>
          <div class="menu-wrap" @mouseenter="showPopup('fit')" @mouseleave="scheduleHidePopup('fit')">
            <button type="button" class="icon" aria-haspopup="listbox" :aria-expanded="openPopup === 'fit'"><Icon :name="fitIcon[activeFit]" /></button>
            <BarMenu v-show="openPopup === 'fit'" placement="up" role="listbox" @mouseenter="showPopup('fit')" @mouseleave="scheduleHidePopup('fit')">
              <button
                v-for="fit in fits"
                :key="fit"
                type="button"
                class="with-icon"
                role="option"
                :class="{ on: activeFit === fit }"
                :aria-selected="activeFit === fit"
                @click="chooseFit(fit)"
              ><Icon :name="fitIcon[fit]" /><span>{{ labelForFit(fit) }}</span></button>
            </BarMenu>
          </div>
          <button
            v-if="showFitParam && !editingFitParam"
            type="button"
            class="text fit-param"
            @click="beginFitParam"
          >
            <span class="fit-param-number" :class="fit.fitMode === 'fixedWidth' ? 'digits-5' : 'digits-3'">{{ fitParamLabel }}</span>
            <span class="fit-param-suffix">{{ fitParamSuffix }}</span>
          </button>
          <span v-else-if="showFitParam" class="fit-param fit-param-editing">
            <input
              ref="fitParamInput"
              v-model="fitParamText"
              class="fit-param-input"
              :class="fit.fitMode === 'fixedWidth' ? 'digits-5' : 'digits-3'"
              type="number"
              inputmode="numeric"
              :min="fit.fitMode === 'fixedWidth' ? 100 : 10"
              :max="fit.fitMode === 'fixedWidth' ? 10000 : 800"
              :step="fit.fitMode === 'fixedWidth' ? 10 : 1"
              autocomplete="off"
              spellcheck="false"
              @change="applyFitParamFromEvent"
              @blur="commitFitParam"
              @keydown="onFitParamKeydown"
            />
            <span class="fit-param-suffix">{{ fitParamSuffix }}</span>
          </span>
        </div>
        </template>
      </div>
      <div v-if="thumbnailsOpen" class="thumbs-size">
        <RangeSlider
          v-model="settings.thumbSize"
          class="thumb-size-slider"
          :min="THUMB_SIZE_MIN"
          :max="THUMB_SIZE_MAX"
          :step="1"
          tip-suffix=" px"
          :aria-label="t('thumbSize')"
          @scrub-start="beginThumbSizeGesture"
          @scrub-end="endThumbSizeGesture"
        />
      </div>
      <div v-else class="toolbar-center">
        <div class="cluster toolbar-group toolbar-group-lead">
          <button
            type="button"
            class="icon"
            :data-tip="tipWithShortcut(t('openArchive'), 'openArchive')"
            :aria-label="tipWithShortcut(t('openArchive'), 'openArchive')"
            @click="openArchiveDialog"
          ><Icon name="open" /></button>
        </div>
        <div class="cluster toolbar-group toolbar-nav">
          <button type="button" class="icon" :disabled="pageCount <= 0" :data-tip="tipWithShortcut(t('first'), 'first')" :aria-label="tipWithShortcut(t('first'), 'first')" @click="setIndex(0, settings.readingMode === 'scroll')"><Icon name="first" /></button>
          <button type="button" class="icon" :disabled="pageCount <= 0" :data-tip="tipWithShortcut(t('prev'), 'prev')" :aria-label="tipWithShortcut(t('prev'), 'prev')" @click="turn(false)"><Icon name="prev" /></button>
          <button
            v-if="!editingPage"
            type="button"
            class="page-pager"
            :disabled="pageCount <= 0"
            :style="{ '--digits': pageDigits }"
            @click="beginPage"
          >
            <span class="page-current">{{ pageCurrentLabel }}</span>
            <span class="page-total">/ {{ pageCount }}</span>
          </button>
          <div v-else class="page-pager page-editing">
            <input
              ref="pageInput"
              v-model="pageText"
              class="page-input"
              type="number"
              inputmode="numeric"
              min="1"
              :max="Math.max(1, pageCount)"
              step="1"
              :style="{ '--digits': pageDigits }"
              autocomplete="off"
              spellcheck="false"
              @change="applyPageFromEvent"
              @blur="commitPage"
              @keydown="onPageKeydown"
            />
            <span class="page-total">/ {{ pageCount }}</span>
          </div>
          <button type="button" class="icon" :disabled="pageCount <= 0" :data-tip="tipWithShortcut(t('next'), 'next')" :aria-label="tipWithShortcut(t('next'), 'next')" @click="turn(true)"><Icon name="next" /></button>
          <button type="button" class="icon" :disabled="pageCount <= 0" :data-tip="tipWithShortcut(t('last'), 'last')" :aria-label="tipWithShortcut(t('last'), 'last')" @click="setIndex(pageCount - 1, settings.readingMode === 'scroll')"><Icon name="last" /></button>
        </div>
        <div class="cluster toolbar-group toolbar-group-trail">
          <button
            type="button"
            class="icon"
            :disabled="pageCount <= 0"
            :data-tip="tipWithShortcut(playing ? t('stop') : t('play'), 'play')"
            :aria-label="tipWithShortcut(playing ? t('stop') : t('play'), 'play')"
            @click="togglePlay"
          ><Icon :name="playing ? 'pause' : 'play'" /></button>
          <button
            type="button"
            class="icon rotate-ccw"
            :class="{ on: currentRotation !== 0 }"
            :disabled="pageCount <= 0"
            :data-tip="tipWithShortcut(t('rotateCcw'), 'rotateCcw')"
            :aria-label="tipWithShortcut(t('rotateCcw'), 'rotateCcw')"
            @pointerenter="setRotateGuide(true)"
            @pointerleave="setRotateGuide(false)"
            @click="rotatePageCounterClockwise()"
          ><Icon name="rotate" /></button>
          <button
            type="button"
            class="icon"
            :class="{ on: currentRotation !== 0 }"
            :disabled="pageCount <= 0"
            :data-tip="tipWithShortcut(t('rotateCw'), 'rotate')"
            :aria-label="tipWithShortcut(t('rotateCw'), 'rotate')"
            @pointerenter="setRotateGuide(true)"
            @pointerleave="setRotateGuide(false)"
            @click="rotatePageClockwise()"
          ><Icon name="rotate" /></button>
        </div>
      </div>
      <div class="cluster toolbar-side toolbar-side-end">
        <button
          type="button"
          class="icon"
          :class="{ on: pageBookmarked }"
          :disabled="pageCount <= 0"
          :data-tip="tipWithShortcut(pageBookmarked ? t('removeBookmark') : t('addBookmark'), 'bookmark')"
          :aria-label="tipWithShortcut(pageBookmarked ? t('removeBookmark') : t('addBookmark'), 'bookmark')"
          @pointerenter="setRotateGuide(true)"
          @pointerleave="setRotateGuide(false)"
          @click="togglePageBookmark"
        ><Icon :name="pageBookmarked ? 'bookmarkOn' : 'bookmark'" /></button>
        <button
          type="button"
          class="icon"
          :class="{ on: thumbnailsOpen }"
          :disabled="pageCount <= 0"
          :data-tip="tipWithShortcut(thumbnailsOpen ? t('hideThumbnails') : t('showThumbnails'), 'thumbnails')"
          :aria-label="tipWithShortcut(thumbnailsOpen ? t('hideThumbnails') : t('showThumbnails'), 'thumbnails')"
          @click="toggleThumbnails"
        ><Icon name="thumb" /></button>
        <button
          type="button"
          class="icon"
          :class="{ on: settings.minimalist }"
          :disabled="fullscreen"
          :data-tip="tipWithShortcut(settings.minimalist ? t('leaveMinimalist') : t('minimalist'), 'minimalist')"
          :aria-label="tipWithShortcut(settings.minimalist ? t('leaveMinimalist') : t('minimalist'), 'minimalist')"
          @click="toggleMinimalist"
        >
          <Icon :name="settings.minimalist ? 'minimalistExit' : 'minimalist'" />
        </button>
        <button
          type="button"
          class="icon"
          :class="{ on: fullscreen }"
          :data-tip="tipWithShortcut(fullscreen ? t('leaveFullscreen') : t('fullscreen'), 'fullscreen')"
          :aria-label="tipWithShortcut(fullscreen ? t('leaveFullscreen') : t('fullscreen'), 'fullscreen')"
          @click="toggleFullscreen"
        >
          <Icon :name="fullscreen ? 'exit' : 'full'" />
        </button>
      </div>
    </div>
    </div>
    </div>
  </footer>
</template>

<style>
.dock {
  position: relative;
  z-index: var(--z-dock);
  flex: 0 0 44px;
  height: 44px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: visible;
}

.dock.menu-open {
  z-index: var(--z-menu);
}

.dock.menu-open .chrome-bottom {
  z-index: 5;
}

.app.chrome-overlay .dock {
  flex: 0 0 4px;
  height: 4px;
}

.app.chrome-overlay .dock.seek-follow {
  flex: 0 0 0;
  height: 0;
}

.toolbar-seek {
  position: relative;
  z-index: 4; /* above toolbar chrome */
  flex: 0 0 4px;
  display: flex;
  align-items: center;
  width: 100%;
  height: 4px;
  min-width: 0;
  min-height: 0;
  overflow: visible;
  cursor: pointer;
}

.toolbar-seek.disabled {
  cursor: default;
  pointer-events: none;
  opacity: 0.5;
}

.toolbar-seek::before {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  top: -4px;
  bottom: -4px;
}

.chrome-bottom {
  position: relative;
  z-index: 1; /* below .toolbar-seek */
  min-width: 0;
}

.toolbar {
  flex: 0 0 40px;
  min-width: 0;
  height: 40px;
  display: flex;
  align-items: center;
  padding: 0 8px;
  background: var(--chrome);
  color: var(--chrome-ink);
  overflow: visible;
}

.app.chrome-overlay .toolbar-seek {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
}

.app.chrome-overlay .dock.sliding .toolbar-seek {
  pointer-events: none;
  transition: bottom 140ms ease-out;
}

.app.chrome-overlay .dock.reveal .toolbar-seek {
  bottom: 40px;
}

.app.chrome-overlay .dock.seek-follow .toolbar-seek {
  bottom: 40px;
  opacity: 0;
  pointer-events: none;
  transform: translateY(44px);
}

.app.chrome-overlay .dock.seek-follow.reveal .toolbar-seek {
  opacity: 1;
  pointer-events: auto;
  transform: translateY(0);
}

.app.chrome-overlay .dock.seek-follow.sliding .toolbar-seek {
  transition:
    opacity 140ms ease-out,
    transform 140ms ease-out;
}

.app.chrome-overlay .chrome-bottom {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 40px;
  z-index: 1;
  pointer-events: none;
  opacity: 0;
  transform: translateY(100%);
  transition:
    opacity 140ms ease-out,
    transform 140ms ease-out;
}

.app.chrome-overlay .dock.reveal .chrome-bottom {
  pointer-events: auto;
  opacity: 1;
  transform: translateY(0);
}

.toolbar-bar {
  position: relative;
  display: flex;
  align-items: stretch;
  gap: 6px;
  width: 100%;
  height: 100%;
  min-width: 0;
  overflow: visible;
}

.toolbar-side {
  display: flex;
  align-items: center;
  align-self: stretch;
  gap: 4px;
  min-width: 0;
  z-index: 2;
}

.toolbar .cluster {
  align-self: stretch;
  display: flex;
  align-items: center;
  gap: 2px;
}

.toolbar-side-end {
  margin-left: auto;
  justify-content: flex-end;
}

.toolbar-bar.thumbs-bar {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  column-gap: 12px;
}

.thumbs-bar .toolbar-side-end {
  margin-left: 0;
}

.thumbs-toggles {
  display: flex;
  align-items: center;
  gap: 14px;
  z-index: 2;
}

.thumb-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  line-height: 1;
  color: var(--chrome-ink);
  white-space: nowrap;
}

.thumbs-size {
  display: flex;
  justify-content: center;
  min-width: 0;
}

.thumbs-size .thumb-size-slider {
  width: min(320px, 100%);
}

/* Full-bar overlay: group 2 (nav) is absolutely centered; groups 1/3 flank it. */
.toolbar-center {
  position: absolute;
  inset: 0;
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  column-gap: 14px;
  pointer-events: none;
  z-index: 1;
}

.toolbar-group {
  pointer-events: auto;
}

.toolbar-group-lead {
  justify-self: end;
}

.toolbar-nav {
  justify-self: center;
}

.toolbar-group-trail {
  justify-self: start;
}

.toolbar button.icon.rotate-ccw .asset-icon {
  transform: scaleX(-1);
}

.menu-wrap {
  position: relative;
  align-self: stretch;
  display: flex;
  align-items: center;
}

.bar-menu.up {
  top: auto;
  left: 50%;
  bottom: 100%;
  transform: translateX(-50%);
  min-width: 72px;
}

.toolbar .bar-menu button {
  width: 100%;
  height: auto;
  padding: var(--control-pad-y) var(--control-pad-x);
  font-size: 13px;
  line-height: 18px;
  border-radius: 0;
  color: var(--popup-ink);
  white-space: nowrap;
  text-align: left;
  justify-content: flex-start;
}

.toolbar .bar-menu button.with-icon {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
}

.toolbar .asset-icon {
  flex: 0 0 auto;
}

.toolbar .bar-menu button.on,
.toolbar .bar-menu button.on:hover {
  color: var(--accent);
  font-weight: 600;
}

.toolbar .bar-menu button.on .key,
.toolbar .bar-menu button.on:hover .key {
  font-weight: 400;
}

.toolbar .bar-menu button:hover:not(:disabled),
.toolbar .bar-menu button.is-hot:not(:disabled) {
  color: var(--chrome-active);
  background: var(--chrome-hover);
}

.toolbar .bar-menu button.on:hover,
.toolbar .bar-menu button.on.is-hot {
  color: var(--accent);
  background: var(--chrome-hover);
}

.toolbar button,
.toolbar button:hover,
.toolbar button.on {
  border: none;
  background: transparent;
  border-radius: 4px;
}

.toolbar .bar-menu button,
.toolbar .bar-menu button:hover,
.toolbar .bar-menu button.on,
.toolbar .bar-menu button.is-hot {
  border-radius: 0;
}

.toolbar button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 28px;
  padding: 0 6px;
  line-height: 1;
  color: var(--chrome-ink);
}

.toolbar button:hover:not(:disabled) {
  color: var(--chrome-active);
}
.toolbar button.on:not(:disabled),
.toolbar button.on:hover:not(:disabled) {
  color: var(--accent);
}

.toolbar button.icon {
  position: relative;
  width: 28px;
  padding: 0;
}

.toolbar .text {
  min-width: 0;
  font-size: 13px;
  line-height: 1;
  white-space: nowrap;
}

.toolbar button.zoom-value,
.toolbar .zoom-value {
  box-sizing: border-box;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 2px;
  width: calc(4ch + 16px);
  height: 28px;
  margin: 0;
  padding: 0;
  font-variant-numeric: tabular-nums;
  color: var(--chrome-ink);
}

.toolbar button.zoom-value:hover {
  background: transparent;
  color: var(--chrome-ink);
}

.toolbar button.zoom-value:hover .zoom-number {
  color: var(--chrome-active);
}

.toolbar .zoom-number {
  color: inherit;
}

.toolbar .zoom-suffix {
  flex: 0 0 auto;
  color: var(--chrome-ink);
  line-height: 22px;
}

.toolbar .zoom-editing {
  color: var(--chrome-ink);
}

.toolbar .zoom-input {
  box-sizing: border-box;
  width: calc(3ch + 10px);
  min-width: 0;
  height: 22px;
  margin: 0;
  padding: 0 4px;
  border: none;
  border-radius: 3px;
  background: color-mix(in srgb, var(--chrome-active) 12%, transparent);
  color: var(--chrome-active);
  font: inherit;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  line-height: 22px;
  text-align: right;
}

.toolbar button.fit-param,
.toolbar .fit-param {
  box-sizing: border-box;
  display: inline-flex;
  align-items: center;
  justify-content: flex-start;
  gap: 2px;
  min-width: 0;
  height: 28px;
  margin: 0;
  padding: 0 2px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--chrome-ink);
}

.toolbar button.fit-param:hover {
  background: transparent;
  color: var(--chrome-ink);
}

.toolbar button.fit-param:hover .fit-param-number {
  color: var(--chrome-active);
}

.toolbar .fit-param-number,
.toolbar .fit-param-input {
  box-sizing: border-box;
  display: inline-block;
  min-width: 0;
  height: 22px;
  margin: 0;
  padding: 0 4px;
  font: inherit;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 22px;
  text-align: right;
}

.toolbar .fit-param-number.digits-5,
.toolbar .fit-param-input.digits-5 {
  width: calc(5ch + 8px);
}

.toolbar .fit-param-number.digits-3,
.toolbar .fit-param-input.digits-3 {
  width: calc(3ch + 8px);
}

.toolbar .fit-param-suffix {
  flex: 0 0 auto;
  color: var(--chrome-ink);
  line-height: 22px;
}

.toolbar .fit-param-editing {
  color: var(--chrome-ink);
  cursor: default;
}

.toolbar .fit-param-input {
  border: none;
  border-radius: 3px;
  background: color-mix(in srgb, var(--chrome-active) 12%, transparent);
  color: var(--chrome-active);
  appearance: textfield;
}

.toolbar .fit-param-input::-webkit-inner-spin-button,
.toolbar .fit-param-input::-webkit-outer-spin-button {
  appearance: none;
  display: none;
  margin: 0;
}

.toolbar button.page-pager,
.toolbar .page-pager {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  height: 28px;
  margin: 0;
  padding: 0 2px;
  border: none;
  border-radius: 4px;
  background: transparent;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 1;
  color: var(--chrome-ink);
  cursor: pointer;
}

.toolbar button.page-pager:hover {
  background: transparent;
  color: var(--chrome-ink);
}

.toolbar button.page-pager:hover .page-current {
  color: var(--chrome-active);
}

.toolbar .page-pager.page-editing {
  cursor: default;
}

.toolbar .page-current,
.toolbar .page-input {
  box-sizing: border-box;
  display: inline-block;
  width: calc(var(--digits, 1) * 1ch + 8px);
  height: 22px;
  margin: 0;
  padding: 0 4px;
  font: inherit;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 22px;
  text-align: right;
}

.toolbar .page-input {
  border: none;
  border-radius: 3px;
  background: color-mix(in srgb, var(--chrome-active) 12%, transparent);
  color: var(--chrome-active);
  appearance: textfield;
}

.toolbar .page-input::-webkit-inner-spin-button,
.toolbar .page-input::-webkit-outer-spin-button {
  appearance: none;
  display: none;
  margin: 0;
}

.toolbar .page-total {
  flex: 0 0 auto;
  color: var(--chrome-ink);
}

.toolbar select {
  height: 32px;
  border: none;
  background: transparent;
  color: inherit;
  padding: 0 4px;
  cursor: pointer;
}

.toolbar select:hover {
  color: var(--chrome-active);
}

.toolbar-seek .slider {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 12px;
  margin: 0;
  background: transparent;
  pointer-events: none;
}

.toolbar-seek .slider::-webkit-slider-runnable-track {
  height: 4px;
  border: none;
  border-radius: 0;
  background: linear-gradient(
    to right,
    var(--accent) var(--progress, 0%),
    var(--seek) var(--progress, 0%)
  );
}

/* Extract / measure: semi-transparent reading wash on top of base-service fills. */
.toolbar-seek.extract-progress:not(.neural-progress):not(.translate-progress) .slider::-webkit-slider-runnable-track,
.toolbar-seek.measuring:not(.neural-progress):not(.translate-progress) .slider::-webkit-slider-runnable-track {
  background-color: var(--seek);
  background-repeat: no-repeat;
  background-image:
    linear-gradient(
      to right,
      var(--seek-read-overlay) var(--progress, 0%),
      transparent var(--progress, 0%)
    ),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 100%, 100% 100%;
}

/* Reading progress overlay shared by neural / translate seek fills. */
.toolbar-seek.neural-progress .slider::-webkit-slider-runnable-track,
.toolbar-seek.translate-progress .slider::-webkit-slider-runnable-track {
  background-color: var(--seek);
  background-repeat: no-repeat;
}

.toolbar-seek.neural-progress:not(.translate-progress) .slider::-webkit-slider-runnable-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent));
  background-size: 100% 100%, 100% 100%, 100% 100%;
  background-position: 0 0, 0 0, 0 0;
}

.toolbar-seek.translate-progress:not(.neural-progress) .slider::-webkit-slider-runnable-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent));
  background-size: 100% 100%, 100% 100%, 100% 100%;
  background-position: 0 0, 0 0, 0 0;
}

/* Dual: translation on top half, neural SR on bottom half (4px → 2px + 2px). */
.toolbar-seek.translate-progress.neural-progress .slider::-webkit-slider-runnable-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent));
  background-size: 100% 100%, 100% 50%, 100% 50%, 100% 100%;
  background-position: 0 0, 0 0, 0 100%, 0 0;
}

.toolbar-seek.neural-progress.measuring:not(.translate-progress) .slider::-webkit-slider-runnable-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 100%, 100% 100%, 100% 100%;
}

.toolbar-seek.translate-progress.measuring:not(.neural-progress) .slider::-webkit-slider-runnable-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 100%, 100% 100%, 100% 100%;
}

.toolbar-seek.translate-progress.neural-progress.measuring .slider::-webkit-slider-runnable-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 50%, 100% 50%, 100% 100%, 100% 100%;
  background-position: 0 0, 0 0, 0 100%, 0 0, 0 0;
}

.toolbar-seek .slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  margin-top: -4px;
  border: none;
  border-radius: 50%;
  background: var(--accent);
  opacity: 0;
  transition: opacity 0.15s ease;
}

.toolbar-seek:hover .slider::-webkit-slider-thumb,
.toolbar-seek.scrubbing .slider::-webkit-slider-thumb {
  opacity: 1;
  margin-top: -3px;
}

.toolbar-seek:hover .slider::-webkit-slider-runnable-track,
.toolbar-seek.scrubbing .slider::-webkit-slider-runnable-track {
  height: 6px;
  margin-top: -1px;
}

.toolbar-seek .slider::-moz-range-track {
  height: 4px;
  border: none;
  border-radius: 0;
  background: var(--seek);
}

.toolbar-seek.extract-progress:not(.neural-progress):not(.translate-progress) .slider::-moz-range-track,
.toolbar-seek.measuring:not(.neural-progress):not(.translate-progress) .slider::-moz-range-track {
  background-color: var(--seek);
  background-repeat: no-repeat;
  /* Reading wash is ::-moz-range-progress; track only carries extract + measure. */
  background-image:
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 100%;
}

.toolbar-seek.neural-progress .slider::-moz-range-track,
.toolbar-seek.translate-progress .slider::-moz-range-track {
  background-color: var(--seek);
  background-repeat: no-repeat;
}

.toolbar-seek.neural-progress:not(.translate-progress) .slider::-moz-range-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent));
  background-size: 100% 100%, 100% 100%, 100% 100%;
  background-position: 0 0, 0 0, 0 0;
}

.toolbar-seek.translate-progress:not(.neural-progress) .slider::-moz-range-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent));
  background-size: 100% 100%, 100% 100%, 100% 100%;
  background-position: 0 0, 0 0, 0 0;
}

.toolbar-seek.translate-progress.neural-progress .slider::-moz-range-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent));
  background-size: 100% 100%, 100% 50%, 100% 50%, 100% 100%;
  background-position: 0 0, 0 0, 0 100%, 0 0;
}

.toolbar-seek.neural-progress.measuring:not(.translate-progress) .slider::-moz-range-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 100%, 100% 100%, 100% 100%;
}

.toolbar-seek.translate-progress.measuring:not(.neural-progress) .slider::-moz-range-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 100%, 100% 100%, 100% 100%;
}

.toolbar-seek.translate-progress.neural-progress.measuring .slider::-moz-range-track {
  background-image:
    var(
      --neural-progress-overlay,
      linear-gradient(
        to right,
        var(--seek-read-overlay) var(--progress, 0%),
        transparent var(--progress, 0%)
      )
    ),
    var(--translate-seek, linear-gradient(to right, transparent, transparent)),
    var(--neural-seek, linear-gradient(to right, transparent, transparent)),
    var(--extract-seek, linear-gradient(to right, transparent, transparent)),
    linear-gradient(
      to right,
      var(--seek-buffer) var(--measure, 0%),
      transparent var(--measure, 0%)
    );
  background-size: 100% 100%, 100% 50%, 100% 50%, 100% 100%, 100% 100%;
  background-position: 0 0, 0 0, 0 100%, 0 0, 0 0;
}

.toolbar-seek .slider::-moz-range-progress {
  height: 4px;
  border-radius: 0;
  background: var(--accent);
}

.toolbar-seek.extract-progress .slider::-moz-range-progress,
.toolbar-seek.measuring .slider::-moz-range-progress {
  background: var(--seek-read-overlay);
}

/* Progress fill lives in the track overlay so neural/translate marks can show through. */
.toolbar-seek.neural-progress .slider::-moz-range-progress,
.toolbar-seek.translate-progress .slider::-moz-range-progress {
  background: transparent;
}

.toolbar-seek .slider::-moz-range-thumb {
  width: 12px;
  height: 12px;
  border: none;
  border-radius: 50%;
  background: var(--accent);
}

.tiny {
  width: 64px;
  padding: 4px 6px;
}
.toolbar .tiny {
  width: 52px;
  height: 28px;
  background: transparent;
  border: none;
  text-align: center;
  color: var(--chrome-active);
}
</style>
