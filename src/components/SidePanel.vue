<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { chapterIndexAtPage, chapterPageCount } from "../lib/chapters";
import { hideAppTip, showAppTip } from "../lib/appTip";
import {
  chapters,
  confirmClearBookmarks,
  deleteBookmark,
  openAddBookmark,
  openChapterRules,
  openEditBookmark,
  pageCount,
  pageIndex,
  setIndex,
  settings,
  showFullscreenSidebar,
  sideOpen,
  sideTab,
  bookmarks,
} from "../reader";
import Icon from "./Icon.vue";
import VirtualList from "./VirtualList.vue";
const { t } = useI18n();

/** Row height + gap, aligned with dropdown option density. */
const SIDE_LIST_ROW_STRIDE = 40;

const listRef = ref<InstanceType<typeof VirtualList> | null>(null);
/** List click owns the page change — do not re-center the active row. */
let skipActiveScroll = false;

const currentPage = computed(() => pageIndex.value + 1);

const activeBookmarkIndex = computed(() =>
  bookmarks.value.findIndex((bookmark) => bookmark.page === currentPage.value),
);

const activeChapterIndex = computed(() =>
  chapterIndexAtPage(chapters.value.chapters, currentPage.value),
);

function jump(page: number) {
  skipActiveScroll = true;
  setIndex(page - 1, settings.readingMode === "scroll");
  // If active row did not change, the watch below will not run — clear the flag.
  void nextTick(() => {
    skipActiveScroll = false;
  });
}

function bookmarkKey(index: number) {
  const bookmark = bookmarks.value[index];
  return bookmark ? `${bookmark.page}-${index}` : index;
}

function chapterKey(index: number) {
  const chapter = chapters.value.chapters[index];
  return chapter ? `${chapter.page}-${chapter.title}` : index;
}

function remarkOf(index: number) {
  return bookmarks.value[index]?.comment?.trim() || "";
}

function titleOf(index: number) {
  const chapter = chapters.value.chapters[index];
  if (!chapter) return "";
  return chapter.title || t("root");
}

function chapterPagesLabel(index: number) {
  const count = chapterPageCount(chapters.value.chapters, index, pageCount.value);
  return t("chapterPages", { n: count }, count);
}

/** Digit width for bookmark page column (matches total page count). */
const pageNumCh = computed(() =>
  Math.max(1, String(Math.max(0, pageCount.value)).length),
);

function showTip(event: PointerEvent, text: string) {
  const target = event.currentTarget;
  if (!text || !(target instanceof HTMLElement)) {
    hideAppTip();
    return;
  }
  const rect = target.getBoundingClientRect();
  showAppTip({
    text,
    x: rect.left + rect.width / 2,
    y: rect.top,
    placement: "above",
    gap: 8,
  });
}

function scrollToActive(behavior: ScrollBehavior = "auto") {
  const idx =
    sideTab.value === "bookmarks" ? activeBookmarkIndex.value : activeChapterIndex.value;
  if (idx < 0) return;
  void listRef.value?.scrollToIndex(idx, { align: "center", behavior });
}

watch(
  () => [sideTab.value, activeBookmarkIndex.value, activeChapterIndex.value] as const,
  (curr, prev) => {
    const tabChanged = !prev || curr[0] !== prev[0];
    if (tabChanged) {
      skipActiveScroll = false;
      void nextTick(() => scrollToActive("auto"));
      return;
    }
    // Consume before any nextTick so list-driven jumps never re-center.
    if (skipActiveScroll) {
      skipActiveScroll = false;
      return;
    }
    const activeChanged =
      (sideTab.value === "bookmarks" && curr[1] !== prev![1]) ||
      (sideTab.value === "chapters" && curr[2] !== prev![2]);
    if (!activeChanged) return;
    void nextTick(() => scrollToActive("smooth"));
  },
);

/** Remount / float reveal: jump instantly to the current page row. */
async function scrollActiveOnShow() {
  await nextTick();
  await nextTick();
  scrollToActive("auto");
}

onMounted(() => {
  void scrollActiveOnShow();
});

onBeforeUnmount(() => {
  hideAppTip();
});

watch(showFullscreenSidebar, (show) => {
  if (show) void scrollActiveOnShow();
});

watch(sideOpen, (open) => {
  if (open) void scrollActiveOnShow();
});
</script>

<template>
  <aside class="side">
    <div class="side-tabs app-tabs" role="tablist">
      <button
        type="button"
        class="app-tab"
        role="tab"
        :class="{ on: sideTab === 'bookmarks' }"
        :aria-selected="sideTab === 'bookmarks'"
        @click="sideTab = 'bookmarks'"
      >
        {{ t("bookmarks") }}
      </button>
      <button
        type="button"
        class="app-tab"
        role="tab"
        :class="{ on: sideTab === 'chapters' }"
        :aria-selected="sideTab === 'chapters'"
        @click="sideTab = 'chapters'"
      >
        {{ t("chapters") }}
      </button>
    </div>

    <div v-if="sideTab === 'bookmarks'" class="side-pane">
      <div class="side-list-host">
        <p v-if="bookmarks.length === 0" class="side-empty">{{ t("emptyBookmarks") }}</p>
        <VirtualList
          v-else
          ref="listRef"
          class="side-virtual"
          :item-count="bookmarks.length"
          :row-stride="SIDE_LIST_ROW_STRIDE"
          :overscan="10"
          :item-key="bookmarkKey"
          arrow-nav
        >
          <template #default="{ index }">
            <div
              class="side-item"
              :class="{ on: index === activeBookmarkIndex }"
            >
              <button type="button" class="side-item-main" @click="jump(bookmarks[index]!.page)">
                <span class="side-item-page" :style="{ width: `${pageNumCh}ch` }">{{ bookmarks[index]!.page }}</span>
                <span class="side-item-sep">:</span>
                <span class="side-item-text" :title="remarkOf(index) || undefined">{{ remarkOf(index) }}</span>
              </button>
              <div class="side-item-actions">
                <button
                  type="button"
                  class="icon-btn side-item-action side-item-action--edit"
                  :aria-label="t('edit')"
                  @pointerenter="showTip($event, t('edit'))"
                  @pointerleave="hideAppTip"
                  @click.stop="openEditBookmark(index)"
                >
                  <Icon name="edit" />
                </button>
                <button
                  type="button"
                  class="icon-btn side-item-action side-item-action--remove"
                  :aria-label="t('remove')"
                  @pointerenter="showTip($event, t('remove'))"
                  @pointerleave="hideAppTip"
                  @click.stop="deleteBookmark(index)"
                >
                  <Icon name="remove" />
                </button>
              </div>
            </div>
          </template>
        </VirtualList>
      </div>
      <div class="side-actions">
        <span v-if="bookmarks.length > 0" class="side-count">
          {{ t("bookmarkCount", { n: bookmarks.length }) }}
        </span>
        <span v-else class="side-count-spacer" />
        <div class="side-actions-end">
          <button
            type="button"
            class="icon-btn side-action"
            :aria-label="t('add')"
            @pointerenter="showTip($event, t('add'))"
            @pointerleave="hideAppTip"
            @click="openAddBookmark"
          >
            <Icon name="add" />
          </button>
          <button
            type="button"
            class="icon-btn side-action hover-warning"
            :aria-label="t('clear')"
            :disabled="bookmarks.length === 0"
            @pointerenter="showTip($event, t('clear'))"
            @pointerleave="hideAppTip"
            @click="hideAppTip(); confirmClearBookmarks()"
          >
            <Icon name="clear" />
          </button>
        </div>
      </div>
    </div>

    <div v-else class="side-pane">
      <div class="side-list-host">
        <p v-if="chapters.invalidRule" class="side-empty">{{ t("invalidRule") }}</p>
        <p v-else-if="chapters.chapters.length === 0" class="side-empty">{{ t("emptyChapters") }}</p>
        <VirtualList
          v-else
          ref="listRef"
          class="side-virtual"
          :item-count="chapters.chapters.length"
          :row-stride="SIDE_LIST_ROW_STRIDE"
          :overscan="10"
          :item-key="chapterKey"
          arrow-nav
        >
          <template #default="{ index }">
            <button
              type="button"
              class="side-item side-item--chapter"
              :class="{ on: index === activeChapterIndex }"
              @click="jump(chapters.chapters[index]!.page)"
            >
              <span class="side-item-chapter-label">
                <span class="side-item-text" :title="titleOf(index) || undefined">{{ titleOf(index) }}</span>
                <span class="side-item-sep">:</span>
                <span class="side-item-page">{{ chapters.chapters[index]!.page }}</span>
              </span>
              <span class="side-item-page-num">{{ chapterPagesLabel(index) }}</span>
            </button>
          </template>
        </VirtualList>
      </div>
      <div class="side-actions">
        <span v-if="chapters.chapters.length > 0" class="side-count">
          {{ t("chapterCount", { n: chapters.chapters.length }) }}
        </span>
        <span v-else class="side-count-spacer" />
        <div class="side-actions-end">
          <button
            type="button"
            class="icon-btn side-action"
            :aria-label="t('chapterRule')"
            @pointerenter="showTip($event, t('chapterRule'))"
            @pointerleave="hideAppTip"
            @click="hideAppTip(); openChapterRules()"
          >
            <Icon name="regexp" />
          </button>
        </div>
      </div>
    </div>
  </aside>
</template>

<style>
.side {
  width: 100%;
  height: 100%;
  border-right: 1px solid var(--line);
  background: var(--chrome);
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}

.side-tabs.app-tabs {
  display: flex;
  gap: 6px;
  align-items: center;
  flex: none;
  padding: 0 8px;
  background: var(--chrome);
  color: var(--chrome-ink);
}

.side-pane {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.side-list-host {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--menu);
}

.side-empty {
  margin: 0;
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px 12px;
  font-size: 13px;
  line-height: 1.4;
  color: var(--muted);
  text-align: center;
}

.side-virtual {
  flex: 1 1 auto;
  min-height: 0;
  /* Match AppCustomSelect panel padding (4px all sides). */
  padding: 4px;
  box-sizing: border-box;
}

.side-item {
  box-sizing: border-box;
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  height: 100%;
  max-height: 36px;
  margin: 0;
  padding: 0 8px 0 10px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  color: var(--ink);
  font-size: 13px;
  line-height: 1.3;
  text-align: left;
  cursor: pointer;
  transition: none;
}

.side-item--chapter {
  gap: 8px;
  padding: 0 10px;
}

.side-item:hover:not(.on) {
  background: var(--list-item-bg-hover);
  border: none;
  color: var(--ink);
}

.side-item.on {
  background: var(--list-item-bg-active);
  color: var(--ink);
  border: none;
}

.side-item.on:hover {
  background: var(--list-item-bg-active);
  color: var(--ink);
  border: none;
}

.side-item-main {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: baseline;
  gap: 0;
  margin: 0;
  padding: 6px 0;
  border: none;
  border-radius: 0;
  background: transparent;
  color: var(--ink);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.side-item-main:hover,
.side-item-main:focus-visible,
.side-item-main:hover:not(:disabled),
.side-item.on .side-item-main,
.side-item.on .side-item-main:hover {
  border: none;
  background: transparent;
  color: var(--ink);
}

.side-item-page {
  flex: none;
  box-sizing: content-box;
  color: var(--accent);
  font-variant-numeric: tabular-nums;
  text-align: right;
}

.side-item.on .side-item-page,
.side-item-main:hover .side-item-page,
.side-item-main:hover .side-item-sep {
  color: var(--accent);
}

.side-item-sep {
  flex: none;
  margin: 0 2px 0 0;
  color: var(--accent);
}

.side-item-text {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ink);
}

.side-item-main:hover .side-item-text,
.side-item.on .side-item-text {
  color: var(--ink);
}

.side-item-chapter-label {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: baseline;
  gap: 0;
}

.side-item-chapter-label .side-item-text {
  flex: 0 1 auto;
}

.side-item-chapter-label .side-item-page {
  flex: none;
  width: auto;
  text-align: left;
}

.side-item-chapter-label .side-item-sep {
  color: var(--ink);
}

.side-item-page-num {
  flex: none;
  margin-left: auto;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
  font-size: 12px;
}

.side-item--chapter:hover:not(:disabled),
.side-item--chapter.on:not(:disabled),
.side-item--chapter.on:hover:not(:disabled) {
  border: none;
  color: var(--ink);
}

.side-item--chapter:hover .side-item-page,
.side-item--chapter.on .side-item-page {
  color: var(--accent);
}

.side-item--chapter:hover .side-item-sep,
.side-item--chapter.on .side-item-sep {
  color: var(--ink);
}

.side-item--chapter:hover .side-item-page-num,
.side-item--chapter.on .side-item-page-num {
  color: var(--muted);
}

.side-item-actions {
  flex: none;
  display: none;
  align-items: center;
  gap: 0;
}

.side-item:hover .side-item-actions,
.side-item:focus-within .side-item-actions {
  display: flex;
}

.side-item-actions .side-item-action {
  width: 28px;
  height: 28px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  color: var(--chrome-ink);
}

.side-item-actions .side-item-action .asset-icon,
.side-item-actions .side-item-action .asset-icon svg {
  width: var(--icon-size);
  height: var(--icon-size);
}

.side-item-actions .side-item-action:hover:not(:disabled) {
  border: none;
  background: transparent;
  color: var(--chrome-active);
}

.side-item-actions .side-item-action--remove:hover:not(:disabled) {
  color: var(--danger);
}

.side-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
  box-sizing: border-box;
  height: 36px;
  padding: 0 8px 0 12px;
  border-top: 1px solid var(--line);
  background: var(--chrome);
  color: var(--chrome-ink);
}

.side-count {
  font-size: 12px;
  color: var(--muted);
  white-space: nowrap;
}

.side-count-spacer {
  flex: 1 1 auto;
  min-width: 0;
}

.side-actions-end {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-left: auto;
}

.side-actions-end .icon-btn.side-action {
  width: 28px;
  height: 28px;
  color: var(--chrome-ink);
}

.side-actions-end .icon-btn.side-action .asset-icon,
.side-actions-end .icon-btn.side-action .asset-icon svg {
  width: var(--icon-size);
  height: var(--icon-size);
}

.side-actions-end .icon-btn.side-action:hover:not(:disabled) {
  color: var(--chrome-active);
  background: transparent;
}

.side-actions-end .icon-btn.side-action.hover-warning:hover:not(:disabled) {
  color: var(--danger);
  background: transparent;
}
</style>
