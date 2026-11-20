<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, ref, watch } from "vue";
import { bookmarkDialog, commitBookmark, pageCount, pageIndex } from "../reader";
import AppModal from "./AppModal.vue";

const { t } = useI18n();
const commentInput = ref<HTMLInputElement | null>(null);

const open = computed({
  get: () => bookmarkDialog.open,
  set: (value: boolean) => {
    bookmarkDialog.open = value;
  },
});

function clampBookmarkPage(value: number) {
  const total = pageCount.value;
  if (total <= 0) return 1;
  const page = Number.isFinite(value) ? Math.trunc(value) : 1;
  return Math.min(total, Math.max(1, page));
}

watch(open, async (isOpen) => {
  if (!isOpen) return;
  bookmarkDialog.page = clampBookmarkPage(bookmarkDialog.page);
  await nextTick();
  const el = commentInput.value;
  el?.focus();
  el?.select();
});

function onPageBlur() {
  bookmarkDialog.page = clampBookmarkPage(bookmarkDialog.page);
}

function useCurrentPage() {
  const total = pageCount.value;
  if (total <= 0) return;
  bookmarkDialog.page = clampBookmarkPage(pageIndex.value + 1);
}

function onCommentKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    commitBookmark();
  }
}
</script>

<template>
  <AppModal
    v-model="open"
    :title="t('bookmarkTitle')"
    max-width="360px"
    panel-class="settings-dialog bookmark-dialog"
  >
    <div class="bookmark-form">
      <div class="form-row">
        <span>{{ t("page") }}</span>
        <input
          v-model.number="bookmarkDialog.page"
          type="number"
          min="1"
          :max="Math.max(1, pageCount)"
          step="1"
          :aria-label="t('page')"
          @blur="onPageBlur"
        />
      </div>
      <div class="form-row">
        <span>{{ t("comment") }}</span>
        <input
          ref="commentInput"
          v-model="bookmarkDialog.comment"
          type="text"
          :aria-label="t('comment')"
          @keydown="onCommentKeydown"
        />
      </div>
    </div>
    <template #footer>
      <div class="app-modal-actions">
        <button type="button" class="ghost" @click="useCurrentPage">
          {{ t("bookmarkUseCurrentPage") }}
        </button>
        <div class="app-modal-actions-end">
          <button type="button" class="ghost" @click="open = false">{{ t("cancel") }}</button>
          <button type="button" class="primary" @click="commitBookmark">{{ t("ok") }}</button>
        </div>
      </div>
    </template>
  </AppModal>
</template>

<style>
.app-modal-panel.bookmark-dialog .bookmark-form {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  column-gap: 12px;
  row-gap: 6px;
  align-items: center;
  font-size: 14px;
  padding: 6px 12px;
}

.app-modal-panel.bookmark-dialog .bookmark-form .form-row {
  display: contents;
}

.app-modal-panel.bookmark-dialog .bookmark-form .form-row > span:first-child {
  text-align: right;
}

.app-modal-backdrop.restart-required-dialog {
  z-index: 10001;
}
</style>
