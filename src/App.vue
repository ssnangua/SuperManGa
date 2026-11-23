<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import AboutDialog from "./components/AboutDialog.vue";
import ImageInfoDialog from "./components/ImageInfoDialog.vue";
import AppTipHost from "./components/AppTipHost.vue";
import AppToastHost from "./components/AppToastHost.vue";
import DetectorAssetsDialog from "./components/DetectorAssetsDialog.vue";
import NeuralAssetsDialog from "./components/NeuralAssetsDialog.vue";
import OcrAssetsDialog from "./components/OcrAssetsDialog.vue";
import PdfPasswordDialog from "./components/PdfPasswordDialog.vue";
import RestartRequiredDialog from "./components/RestartRequiredDialog.vue";
import UpdateAvailableDialog from "./components/UpdateAvailableDialog.vue";
import PageView from "./components/PageView.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import { checkUpdateOnStartup } from "./lib/updateCheck";
import { fetchDataManifest } from "./lib/render/assets";
import ShortcutsDialog from "./components/ShortcutsDialog.vue";
import SidePanel from "./components/SidePanel.vue";
import BookmarkDialog from "./components/BookmarkDialog.vue";
import ChapterRuleDialog from "./components/ChapterRuleDialog.vue";
import TitleBar from "./components/TitleBar.vue";
import ThumbnailsView from "./components/ThumbnailsView.vue";
import Toolbar from "./components/Toolbar.vue";
import ScrollView from "./components/ScrollView.vue";
import Icon from "./components/Icon.vue";
import LoadingDotsBounce from "./components/LoadingDotsBounce.vue";
import { bindMenuHot } from "./lib/menuHot";
import {
  altDown,
  applySidebarResize,
  boot,
  canShowFullscreenPanel,
  chromeEdgeRevealBlockedByOverlay,
  chromeOverlay,
  chromeTop,
  clampSidebarWidthToViewport,
  clock,
  comic,
  downloadNetworkConfig,
  dropping,
  endSidebarResize,
  globalBusy,
  globalBusyText,
  fullscreen,
  FULLSCREEN_CHROME_ZONE,
  FULLSCREEN_LEFT_EDGE_PX,
  FULLSCREEN_RIGHT_GUTTER,
  handleKey,
  handleKeyUp,
  hideFullscreenSidebar,
  magnifierOn,
  openArchiveDialog,
  openFolderDialog,
  pageColorFilter,
  pageCount,
  pageLabel,
  logicalPageSizes,
  isolateCover,
  splitDoublePages,
  showEmpty,
  pageIndex,
  resizingSidebar,
  revealFullscreenSidebar,
  scrollOffset,
  setIndex,
  settings,
  modeFit,
  showFullscreenSidebar,
  sideOpen,
  sidebarWidthForLayout,
  startResizeSidebar,
  thumbnailsOpen,
} from "./reader";

const { t } = useI18n();
const pointerIdle = ref(false);
const IDLE_MS = 2000;
const viewFit = computed(() => modeFit());
const pageFilter = computed(() => pageColorFilter());
const overlayPage = computed(() => {
  if (!chromeOverlay.value) return false;
  return fullscreen.value ? settings.fullscreenShowPage : settings.minimalistShowPage;
});
const overlayTime = computed(
  () => chromeOverlay.value && fullscreen.value && settings.fullscreenShowTime,
);

const sidebarShellVisible = computed(
  () =>
    !!comic.value
    && ((!chromeOverlay.value && sideOpen.value)
      || (chromeOverlay.value && showFullscreenSidebar.value)),
);

let idleTimer = 0;

function bumpPointerIdle() {
  pointerIdle.value = false;
  window.clearTimeout(idleTimer);
  if (!chromeOverlay.value) return;
  idleTimer = window.setTimeout(() => {
    if (document.querySelector(".menubar.reveal, .dock.reveal, .overlay-sb:hover, .sidebar-pane--float:hover")) {
      bumpPointerIdle();
      return;
    }
    pointerIdle.value = true;
  }, IDLE_MS);
}

watch(chromeOverlay, () => {
  bumpPointerIdle();
});

function inRightGutter(x: number) {
  return x >= window.innerWidth - FULLSCREEN_RIGHT_GUTTER;
}

function isInsideSidebarFloat(clientX: number, clientY: number): boolean {
  const el = document.elementFromPoint(clientX, clientY);
  if (!(el instanceof Element)) return false;
  if (el.closest(".sidebar-pane")) return true;
  if (el.closest("[data-fullscreen-sidebar-float]")) return true;
  return false;
}

function tryCollapseFullscreenSidebar(clientX: number, clientY: number) {
  if (!chromeOverlay.value || !showFullscreenSidebar.value) return;
  if (resizingSidebar.value) return;
  if (isInsideSidebarFloat(clientX, clientY)) return;
  hideFullscreenSidebar();
}

function onFullscreenSidebarMouseLeave(event: MouseEvent) {
  if (!chromeOverlay.value) return;
  const related = event.relatedTarget;
  if (related instanceof Element && related.closest("[data-fullscreen-sidebar-float]")) {
    return;
  }
  requestAnimationFrame(() => {
    tryCollapseFullscreenSidebar(event.clientX, event.clientY);
  });
}

function menubarMenuOpen() {
  return !!document.querySelector(".menubar.menu-open");
}

function updateChromeTopHover(event: PointerEvent) {
  if (menubarMenuOpen()) {
    chromeTop.value = true;
    return;
  }
  const target = event.target;
  const inBar = target instanceof Element && !!target.closest(".menubar");
  if (inBar) {
    chromeTop.value = true;
    return;
  }
  if (chromeTop.value) {
    const keep =
      event.clientY <= FULLSCREEN_CHROME_ZONE && !inRightGutter(event.clientX);
    if (!keep) chromeTop.value = false;
    return;
  }
  if (chromeEdgeRevealBlockedByOverlay()) return;
  if (event.buttons !== 0) return;
  if (
    event.clientY <= FULLSCREEN_CHROME_ZONE
    && !inRightGutter(event.clientX)
    && canShowFullscreenPanel("header")
  ) {
    chromeTop.value = true;
  }
}

function updateChromeSidebarHover(event: PointerEvent) {
  if (showFullscreenSidebar.value) {
    tryCollapseFullscreenSidebar(event.clientX, event.clientY);
    return;
  }
  if (chromeEdgeRevealBlockedByOverlay()) return;
  if (event.buttons !== 0) return;
  if (
    event.clientX <= FULLSCREEN_LEFT_EDGE_PX
    && comic.value
    && canShowFullscreenPanel("sidebar")
  ) {
    revealFullscreenSidebar();
  }
}

function onPointerMove(event: PointerEvent) {
  bumpPointerIdle();
  if (resizingSidebar.value) {
    applySidebarResize(event.clientX);
    return;
  }
  if (!chromeOverlay.value) return;
  updateChromeTopHover(event);
  updateChromeSidebarHover(event);
}

function onPointerUp() {
  if (resizingSidebar.value) endSidebarResize();
}

function onPointerOut(event: PointerEvent) {
  if (event.relatedTarget) return;
  if (!menubarMenuOpen()) chromeTop.value = false;
  if (chromeOverlay.value && !resizingSidebar.value) hideFullscreenSidebar();
}

function onLayoutPointerDown(event: PointerEvent) {
  if (!chromeOverlay.value || !showFullscreenSidebar.value) return;
  const target = event.target;
  if (target instanceof Element && target.closest(".sidebar-pane")) return;
  hideFullscreenSidebar();
}

function onWindowResize() {
  clampSidebarWidthToViewport();
}

let unbindMenuHot: (() => void) | undefined;

function blockContextMenu(event: Event) {
  event.preventDefault();
}

onMounted(() => {
  void boot().then(() => {
    void checkUpdateOnStartup();
    if (settings.fetchDataManifestOnStartup) {
      // Fail quietly offline; keep local/seed manifest.
      window.setTimeout(() => {
        void fetchDataManifest(downloadNetworkConfig()).catch(() => {});
      }, 1800);
    }
  });
  unbindMenuHot = bindMenuHot();
  window.addEventListener("keydown", handleKey);
  window.addEventListener("keyup", handleKeyUp);
  window.addEventListener("blur", clearAlt);
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", onPointerUp);
  window.addEventListener("pointercancel", onPointerUp);
  window.addEventListener("pointerout", onPointerOut);
  window.addEventListener("contextmenu", blockContextMenu);
  window.addEventListener("resize", onWindowResize);
});

onBeforeUnmount(() => {
  unbindMenuHot?.();
  window.removeEventListener("keydown", handleKey);
  window.removeEventListener("keyup", handleKeyUp);
  window.removeEventListener("blur", clearAlt);
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerup", onPointerUp);
  window.removeEventListener("pointercancel", onPointerUp);
  window.removeEventListener("pointerout", onPointerOut);
  window.removeEventListener("contextmenu", blockContextMenu);
  window.removeEventListener("resize", onWindowResize);
  window.clearTimeout(idleTimer);
});

function clearAlt() {
  altDown.value = false;
}
</script>

<template>
  <div
    class="app"
    :class="{ 'show-bounds': altDown, dropping, fullscreen, 'chrome-overlay': chromeOverlay, 'idle-pointer': chromeOverlay && pointerIdle, 'magnifier-on': magnifierOn }"
    :style="{ background: settings.backgroundColor }"
  >
    <TitleBar :revealed="!chromeOverlay || chromeTop" />
    <div class="workspace" @pointerdown.capture="onLayoutPointerDown">
      <!-- Windowed sidebar stays in workspace flex layout. -->
      <div
        v-if="!chromeOverlay && sidebarShellVisible"
        class="sidebar-pane"
        :style="{ width: `${sidebarWidthForLayout}px` }"
      >
        <SidePanel />
      </div>
      <div
        v-show="sideOpen && comic && !chromeOverlay"
        class="resizer"
        :class="{ 'resizer--active': resizingSidebar }"
        :style="{ left: `calc(${sidebarWidthForLayout}px - var(--app-sash-size, 4px) / 2)` }"
        @mousedown="startResizeSidebar"
      />
      <main class="stage">
        <p v-if="showEmpty && !comic" class="empty">
          <strong>{{ t('emptyTitle') }}</strong>
          <span>cbz · cbr · zip · rar · 7z · pdf · epub · mobi · azw3 · azw</span>
          <span class="empty-actions">
            <button type="button" @click="openArchiveDialog"><Icon name="open" />{{ t('openArchive') }}</button>
            <button type="button" @click="openFolderDialog"><Icon name="folderOpen" />{{ t('openFolder') }}</button>
          </span>
        </p>
        <ThumbnailsView
          v-else-if="comic && thumbnailsOpen"
          :key="`thumbs-${comic.sessionId}`"
        />
        <ScrollView
          v-else-if="comic && settings.readingMode === 'scroll'"
          :key="`${comic.sessionId}-split-${splitDoublePages}`"
          class="page-color"
          :style="{ filter: pageFilter }"
          :count="pageCount"
          :index="pageIndex"
          :fit="viewFit.fitMode"
          :fixed-width="viewFit.fitFixedWidth"
          :original-scale="viewFit.fitOriginalScale"
          :direction="viewFit.direction"
          :sizes="logicalPageSizes"
          :sizes-ready="true"
          :resume="scrollOffset"
          @update:index="setIndex($event, true)"
        />
        <PageView
          v-else-if="comic"
          :key="`${comic.sessionId}-${settings.readingMode}-split-${splitDoublePages}`"
          class="page-color"
          :style="{ filter: pageFilter }"
          :count="pageCount"
          :index="pageIndex"
          :mode="settings.readingMode"
          :direction="viewFit.direction"
          :fit="viewFit.fitMode"
          :fixed-width="viewFit.fitFixedWidth"
          :original-scale="viewFit.fitOriginalScale"
          :isolate-cover="isolateCover"
          :sizes="logicalPageSizes"
          @update:index="setIndex($event)"
        />
        <div v-if="overlayPage" class="fs-page">{{ pageLabel }}</div>
        <div v-if="overlayTime" class="fs-time">{{ clock }}</div>
      </main>
    </div>
    <Toolbar />
    <!--
      Float sidebar must sit outside .workspace (z-index:0 stacking context),
      otherwise the dock/seek paints over it.
    -->
    <Transition name="chromeFloatSidebar">
      <div
        v-if="chromeOverlay && showFullscreenSidebar && comic"
        class="sidebar-pane sidebar-pane--float"
        :style="{ width: `${sidebarWidthForLayout}px` }"
        @mouseleave="onFullscreenSidebarMouseLeave"
      >
        <SidePanel />
        <div
          class="resizer resizer--float"
          :class="{ 'resizer--active': resizingSidebar }"
          @mousedown="startResizeSidebar"
        />
      </div>
    </Transition>
    <AppTipHost />
    <AppToastHost />
    <Teleport to="body">
      <div v-if="dropping" class="drop-overlay" aria-hidden="true">
        <div class="drop-overlay-frame">
          <strong>{{ t("emptyTitle") }}</strong>
        </div>
      </div>
    </Teleport>
    <SettingsDialog />
    <ShortcutsDialog />
    <OcrAssetsDialog />
    <DetectorAssetsDialog />
    <NeuralAssetsDialog />
    <BookmarkDialog />
    <ChapterRuleDialog />
    <AboutDialog />
    <ImageInfoDialog />
    <PdfPasswordDialog />
    <RestartRequiredDialog />
    <UpdateAvailableDialog />
    <div v-if="globalBusy" class="global-busy" role="status" aria-live="polite">
      <div class="global-busy-card">
        <span>{{ globalBusyText || t('assetBusy') }}</span>
        <LoadingDotsBounce />
      </div>
    </div>
  </div>
</template>

<style>
.app {
  position: relative;
  height: 100%;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--ink);
  /* box-shadow: inset 0 0 0 1px var(--edge); */
}

.app.fullscreen {
  box-shadow: none;
}

.app.chrome-overlay .menubar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: var(--z-titlebar);
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transform: translateY(-8px);
  transition:
    opacity 140ms ease-out,
    transform 140ms ease-out,
    visibility 140ms ease-out;
}

.app.chrome-overlay .menubar.reveal {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
  transform: translateY(0);
}

.drop-overlay {
  position: fixed;
  inset: 0;
  z-index: var(--z-drop);
  display: grid;
  place-items: center;
  padding: 24px;
  box-sizing: border-box;
  background: color-mix(in srgb, var(--bg) 58%, transparent);
  backdrop-filter: blur(2px);
  pointer-events: none;
}

.drop-overlay-frame {
  display: grid;
  place-items: center;
  width: min(420px, 100%);
  min-height: 160px;
  padding: 28px 24px;
  box-sizing: border-box;
  border: 2px dashed var(--accent);
  border-radius: 12px;
  background: color-mix(in srgb, var(--panel) 88%, transparent);
  box-shadow: 0 10px 36px color-mix(in srgb, #000 28%, transparent);
  color: var(--ink);
  text-align: center;
}

.drop-overlay-frame strong {
  font-size: 18px;
  font-weight: 600;
  line-height: 1.4;
}

.workspace {
  position: relative;
  z-index: 0;
  flex: 1 1 auto;
  display: flex;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.stage {
  position: relative;
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.empty {
  margin: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  align-items: center;
  text-align: center;
  max-width: 420px;
  color: var(--muted);
}

.empty strong {
  color: var(--ink);
  font-size: 22px;
}

.empty-actions {
  display: flex;
  gap: 8px;
}

.overlay-scroll-host {
  --sb-gutter-x: 0px;
  --sb-gutter-y: 0px;
  position: relative;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.overlay-scroll-host.has-y-scroll {
  --sb-gutter-x: var(--sb-size);
}

.overlay-scroll-host.has-x-scroll {
  --sb-gutter-y: var(--sb-size);
}

.app.chrome-overlay .overlay-scroll-host.has-y-scroll,
.app.chrome-overlay .overlay-scroll-host.has-x-scroll {
  --sb-gutter-x: 0px;
  --sb-gutter-y: 0px;
}

.stage-frame {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
}

.stage-frame > .scroll-view,
.stage-frame > .page-stage,
.overlay-scroll-host > .magnifier-clip {
  position: absolute;
  top: var(--stage-pad-y, 0px);
  right: calc(var(--stage-pad-x, 0px) + var(--sb-gutter-x, 0px));
  bottom: calc(var(--stage-pad-y, 0px) + var(--sb-gutter-y, 0px));
  left: var(--stage-pad-x, 0px);
  flex: none;
  pointer-events: auto;
}

.overlay-scroll-host > .magnifier-clip {
  pointer-events: none;
  overflow: hidden;
  z-index: 1;
}

.overlay-scroll-host > .overlay-sb,
.overlay-scroll-host > .overlay-sb-corner {
  z-index: 2;
}

.scroll-view,
.page-stage {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
  scrollbar-width: none;
}

.scroll-view::-webkit-scrollbar,
.page-stage::-webkit-scrollbar {
  width: 0;
  height: 0;
  display: none;
}

.app.chrome-overlay.idle-pointer,
.app.chrome-overlay.idle-pointer * {
  cursor: none !important;
}

/* Magnifier: hide cursor over the stage; keep scrollbar / drag cursors. */
.app.magnifier-on .stage,
.app.magnifier-on .stage * {
  cursor: none !important;
}

.app.magnifier-on .stage .overlay-sb,
.app.magnifier-on .stage .overlay-sb * {
  cursor: default !important;
}

html.grabbing .app.magnifier-on .stage,
html.grabbing .app.magnifier-on .stage * {
  cursor: grabbing !important;
}

.app.chrome-overlay.idle-pointer .overlay-sb {
  opacity: 0;
  pointer-events: none;
}

.show-bounds img,
.show-bounds .page-bitmap {
  outline: 1px solid var(--accent);
  outline-offset: -1px;
}

.fs-page,
.fs-time {
  position: absolute;
  bottom: 16px;
  z-index: 2;
  padding: 4px 10px;
  border-radius: 999px;
  background: #000000aa;
  color: #fff;
  pointer-events: none;
}

.fs-page {
  left: 16px;
}
.fs-time {
  right: 16px;
}

.app.chrome-overlay .fs-page,
.app.chrome-overlay .fs-time {
  font-size: 12px;
  opacity: 0.3;
}

html.theme-light .fs-page,
html.theme-light .fs-time {
  background: #fff;
  color: #111;
}

.sidebar-pane {
  flex: 0 0 auto;
  height: 100%;
  min-width: 0;
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
}

.sidebar-pane--float {
  position: fixed;
  left: 0;
  top: 0;
  bottom: 0;
  z-index: 3000;
  overflow: visible;
  box-shadow: 6px 0 28px rgba(0, 0, 0, 0.45);
}

.chromeFloatSidebar-enter-active,
.chromeFloatSidebar-leave-active {
  transition: opacity 180ms ease-out, transform 180ms ease-out;
}

.chromeFloatSidebar-enter-from,
.chromeFloatSidebar-leave-to {
  opacity: 0;
  transform: translateX(-12px);
}

.chromeFloatSidebar-leave-active {
  pointer-events: none;
}

.resizer {
  position: absolute;
  top: 0;
  bottom: 0;
  z-index: 1000;
  width: var(--app-sash-size);
  cursor: ew-resize;
  touch-action: none;
  background: transparent;
}

.resizer::before {
  content: "";
  pointer-events: none;
  position: absolute;
  top: 0;
  bottom: 0;
  width: var(--app-sash-hover-size);
  left: calc(50% - (var(--app-sash-hover-size) / 2));
  background: transparent;
  transition: background-color var(--app-sash-fade-duration) ease-out;
}

.resizer:hover::before,
.resizer.resizer--active::before {
  background: var(--accent);
}

.resizer.resizer--active::before {
  transition-delay: 0s;
}

.resizer:hover::before {
  transition-delay: var(--app-sash-hover-delay);
}

.resizer.resizer--float {
  left: auto;
  right: calc(var(--app-sash-size) / -2);
  z-index: 10;
}

.global-busy {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--bg) 55%, transparent);
  backdrop-filter: blur(2px);
  pointer-events: all;
}

.global-busy-card {
  display: inline-flex;
  align-items: baseline;
  gap: 2px;
  padding: 14px 18px;
  border-radius: 10px;
  background: var(--panel);
  border: 1px solid var(--line);
  box-shadow: 0 8px 28px color-mix(in srgb, #000 35%, transparent);
  font-size: 14px;
  line-height: 1.5;
}
</style>
