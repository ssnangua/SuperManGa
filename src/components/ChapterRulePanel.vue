<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";

import {
  generateChapterRuleId,
  previewChapterExamples,
  type ChapterFilenameRule,
} from "../lib/chapterRules";
import {
  SORTABLE_ROW_HANDLE_CLASS,
  useSortableReorder,
} from "../composables/useSortableReorder";
import { hideAppTip, showAppTip } from "../lib/appTip";
import AppModal from "./AppModal.vue";
import Icon from "./Icon.vue";
import SwitchToggle from "./SwitchToggle.vue";
import ChapterRuleExampleTag from "./ChapterRuleExampleTag.vue";
import ChapterRuleEditDialog, {
  type ChapterRuleEditPayload,
} from "./ChapterRuleEditDialog.vue";

const { t } = useI18n();
const modelValue = defineModel<boolean>({ default: false });

const props = defineProps<{
  rules: ChapterFilenameRule[];
  errorText?: string;
}>();

const emit = defineEmits<{
  apply: [payload: { rules: ChapterFilenameRule[] }];
}>();

const localRules = ref<ChapterFilenameRule[]>([]);

const showEditDialog = ref(false);
const editDialogTitle = ref("");
const editingRuleId = ref<string | null>(null);
const editBuiltinRuleId = ref<string | null>(null);
const editPatternDraft = ref("");
const editExamplesDraft = ref("");

watch(
  () => [props.rules, modelValue.value] as const,
  ([rules, opened]) => {
    if (!opened) return;
    localRules.value = rules.map((r) => ({
      ...r,
      examples: [...r.examples],
    }));
  },
  { immediate: true, deep: true },
);

watch(
  modelValue,
  (open) => {
    if (!open) showEditDialog.value = false;
  },
  { flush: "sync" },
);

const rowsWithPreview = computed(() =>
  localRules.value.map((rule) => ({
    rule,
    preview: previewChapterExamples(rule.pattern, rule.examples),
  })),
);

const ruleTableBodyRef = ref<HTMLElement | null>(null);
const tableBodyScrollRef = ref<HTMLElement | null>(null);
const headScrollbarPadPx = ref(0);
const ruleCount = computed(() => localRules.value.length);
let tableBodyScrollRo: ResizeObserver | null = null;

const ruleTableHeadPadStyle = computed(() =>
  headScrollbarPadPx.value > 0
    ? { paddingRight: `${headScrollbarPadPx.value}px` }
    : undefined,
);

function syncRuleTableHeadScrollbarPad() {
  const el = tableBodyScrollRef.value;
  headScrollbarPadPx.value = el ? el.offsetWidth - el.clientWidth : 0;
}

function teardownRuleTableBodyScrollRo() {
  tableBodyScrollRo?.disconnect();
  tableBodyScrollRo = null;
}

function ensureRuleTableBodyScrollRo() {
  teardownRuleTableBodyScrollRo();
  const el = tableBodyScrollRef.value;
  if (!el) {
    headScrollbarPadPx.value = 0;
    return;
  }
  syncRuleTableHeadScrollbarPad();
  if (typeof ResizeObserver === "undefined") return;
  tableBodyScrollRo = new ResizeObserver(syncRuleTableHeadScrollbarPad);
  tableBodyScrollRo.observe(el);
}

const { remount: remountSortable } = useSortableReorder({
  containerRef: ruleTableBodyRef,
  active: modelValue,
  itemCount: ruleCount,
  enabled: computed(() => localRules.value.length > 1),
  onReorder(from, to) {
    moveRule(from, to);
  },
});

watch(modelValue, (open) => {
  if (open) {
    void nextTick(() => {
      ensureRuleTableBodyScrollRo();
      remountSortable();
    });
  } else {
    teardownRuleTableBodyScrollRo();
    headScrollbarPadPx.value = 0;
    hideAppTip();
  }
});

watch(ruleCount, () => {
  void nextTick(syncRuleTableHeadScrollbarPad);
});

onBeforeUnmount(() => {
  teardownRuleTableBodyScrollRo();
  hideAppTip();
});

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

function examplesToEditText(examples: string[]): string {
  return examples.join("\n");
}

function close() {
  modelValue.value = false;
}

function applyRules() {
  emit("apply", {
    rules: localRules.value.map((r) => ({
      ...r,
      examples: [...r.examples],
    })),
  });
}

function openEdit(rule: ChapterFilenameRule) {
  editingRuleId.value = rule.id;
  editBuiltinRuleId.value = rule.builtIn ? rule.id : null;
  editDialogTitle.value = rule.builtIn
    ? t("chapterRuleEditBuiltin")
    : t("chapterRuleEditCustom");
  editPatternDraft.value = rule.pattern;
  editExamplesDraft.value = examplesToEditText(rule.examples);
  showEditDialog.value = true;
}

function openAdd() {
  editingRuleId.value = null;
  editBuiltinRuleId.value = null;
  editDialogTitle.value = t("chapterRuleAddTitle");
  editPatternDraft.value = "";
  editExamplesDraft.value = "";
  showEditDialog.value = true;
}

function onEditSave(payload: ChapterRuleEditPayload) {
  if (editingRuleId.value === null) {
    localRules.value.push({
      id: generateChapterRuleId(),
      pattern: payload.pattern,
      enabled: true,
      examples: payload.examples,
      builtIn: false,
    });
    return;
  }
  const idx = localRules.value.findIndex((r) => r.id === editingRuleId.value);
  if (idx < 0) return;
  const prev = localRules.value[idx];
  if (!prev) return;
  localRules.value[idx] = {
    ...prev,
    pattern: payload.pattern,
    examples: payload.examples,
  };
}

function removeRule(rule: ChapterFilenameRule) {
  if (rule.builtIn) return;
  hideAppTip();
  localRules.value = localRules.value.filter((r) => r.id !== rule.id);
}

function setRuleEnabled(rule: ChapterFilenameRule, enabled: boolean) {
  const idx = localRules.value.findIndex((r) => r.id === rule.id);
  if (idx < 0) return;
  const prev = localRules.value[idx];
  if (!prev) return;
  localRules.value[idx] = { ...prev, enabled };
}

function moveRule(fromIndex: number, toIndex: number) {
  if (fromIndex === toIndex) return;
  const arr = [...localRules.value];
  const [item] = arr.splice(fromIndex, 1);
  if (!item) return;
  arr.splice(toIndex, 0, item);
  localRules.value = arr;
}

</script>

<template>
  <AppModal
    v-model="modelValue"
    :title="t('chapterRule')"
    max-width="640px"
    :mask-closable="false"
    panel-class="settings-dialog chapter-rule-panel"
  >
    <div class="chapter-rule-hints">
      <p class="settings-hint">{{ t("chapterRuleFolderHint") }}</p>
      <i18n-t keypath="chapterRuleDesc" tag="p" class="settings-hint chapter-rule-desc">
        <template #move>
          <Icon name="move" class="chapter-rule-desc-move" />
        </template>
      </i18n-t>
    </div>

    <div class="table-wrap">
      <div class="rule-table-head-wrap" :style="ruleTableHeadPadStyle">
        <table class="rule-table rule-table--head">
          <colgroup>
            <col class="col-check" />
            <col class="col-rule" />
            <col class="col-actions" />
          </colgroup>
          <thead>
            <tr>
              <th class="col-check" scope="col" :aria-label="t('chapterRuleEnable')" />
              <th class="col-rule" scope="col">{{ t("chapterRuleAndExamples") }}</th>
              <th class="col-actions" scope="col">{{ t("chapterRuleActions") }}</th>
            </tr>
          </thead>
        </table>
      </div>
      <div ref="tableBodyScrollRef" class="table-body-scroll">
        <table class="rule-table rule-table--body">
          <colgroup>
            <col class="col-check" />
            <col class="col-rule" />
            <col class="col-actions" />
          </colgroup>
          <tbody ref="ruleTableBodyRef">
            <tr v-for="item in rowsWithPreview" :key="item.rule.id">
              <td class="cell-check">
                <div class="cell-check-inner">
                  <SwitchToggle
                    :model-value="item.rule.enabled"
                    size="sm"
                    :aria-label="t('chapterRuleEnable')"
                    @update:model-value="setRuleEnabled(item.rule, $event)"
                  />
                </div>
              </td>
              <td class="cell-rule">
                <div class="rule-pattern">{{ item.rule.pattern }}</div>
                <div v-if="item.preview.kind === 'error'" class="rule-examples">
                  {{ t("invalidRule") }}
                </div>
                <div
                  v-else-if="item.preview.kind === 'ok'"
                  class="rule-examples rule-examples-body"
                >
                  <ChapterRuleExampleTag
                    v-for="(ex, i) in item.preview.items"
                    :key="i"
                    :item="ex"
                  />
                </div>
              </td>
              <td class="cell-actions">
                <div class="cell-actions-inner">
                  <button
                    type="button"
                    class="icon-btn"
                    :class="SORTABLE_ROW_HANDLE_CLASS"
                    :aria-label="t('chapterRuleReorder')"
                    :disabled="localRules.length <= 1"
                    @pointerenter="showTip($event, t('chapterRuleReorder'))"
                    @pointerleave="hideAppTip"
                  >
                    <Icon name="move" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn"
                    :aria-label="t('edit')"
                    @pointerenter="showTip($event, t('edit'))"
                    @pointerleave="hideAppTip"
                    @click="openEdit(item.rule)"
                  >
                    <Icon name="edit" />
                  </button>
                  <button
                    v-if="!item.rule.builtIn"
                    type="button"
                    class="icon-btn danger-icon"
                    :aria-label="t('remove')"
                    @pointerenter="showTip($event, t('remove'))"
                    @pointerleave="hideAppTip"
                    @click="removeRule(item.rule)"
                  >
                    <Icon name="remove" />
                  </button>
                  <span
                    v-else
                    class="action-slot-spacer"
                    aria-hidden="true"
                  />
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <p v-if="errorText" class="error-text">{{ errorText }}</p>

    <template #footer>
      <div class="app-modal-actions">
        <button class="ghost" type="button" @click="openAdd">
          {{ t("chapterRuleAdd") }}
        </button>
        <div class="app-modal-actions-end">
          <button class="ghost" type="button" @click="close">{{ t("close") }}</button>
          <button class="primary" type="button" @click="applyRules">{{ t("save") }}</button>
        </div>
      </div>
    </template>
  </AppModal>

  <ChapterRuleEditDialog
    v-model="showEditDialog"
    :title="editDialogTitle"
    :pattern="editPatternDraft"
    :examples-text="editExamplesDraft"
    :builtin-rule-id="editBuiltinRuleId"
    @save="onEditSave"
  />
</template>

<style scoped>
.chapter-rule-hints {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0 0 12px;
}

.chapter-rule-hints .settings-hint {
  margin: 0;
}

.chapter-rule-desc-move {
  display: inline-flex;
  vertical-align: -0.15em;
  margin: 0 1px;
  color: var(--muted);
  --icon-size: 14px;
}

.chapter-rule-desc-move :deep(svg) {
  width: 14px;
  height: 14px;
  display: block;
}

.table-wrap {
  display: flex;
  flex-direction: column;
  max-height: min(50vh, 420px);
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--bg);
  overflow: hidden;
}

.rule-table-head-wrap {
  flex-shrink: 0;
}

.table-body-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
}

.rule-table {
  width: 100%;
  border-collapse: separate;
  border-spacing: 0;
  table-layout: fixed;
}

.rule-table col.col-check {
  width: 44px;
}

.rule-table col.col-actions {
  width: 118px;
}

.rule-table th,
.rule-table td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--line);
  vertical-align: middle;
}

.rule-table--head th {
  text-align: left;
  font-size: 12px;
  font-weight: 600;
  color: var(--muted);
  background: color-mix(in srgb, var(--bg) 92%, var(--line));
}

.rule-table--body tbody tr:last-child td {
  border-bottom: none;
}

.cell-check {
  text-align: center;
}

.cell-check-inner {
  display: flex;
  align-items: center;
  justify-content: center;
  /* Match action icon-btn height so switch and actions share the same band. */
  min-height: 32px;
}

.rule-pattern {
  font-size: 12px;
  line-height: 1.45;
  font-family: Consolas, "Courier New", monospace;
  color: var(--ink);
  word-break: break-all;
  white-space: pre-wrap;
}

.rule-examples {
  margin-top: 6px;
  font-size: 12px;
  line-height: 1.45;
  color: var(--muted);
}

.rule-examples-body {
  min-width: 0;
  word-break: break-all;
  color: var(--info);
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.cell-rule {
  min-width: 0;
}

.cell-actions {
  vertical-align: middle;
  text-align: left;
}

/* Fixed 3 slots so move/edit line up whether delete is present. */
.cell-actions-inner {
  display: grid;
  grid-template-columns: repeat(3, 32px);
  gap: 4px;
  justify-content: start;
  align-items: center;
}

.cell-actions-inner .icon-btn {
  width: 32px;
  height: 32px;
}

.cell-actions-inner .icon-btn :deep(.asset-icon),
.cell-actions-inner .icon-btn :deep(.asset-icon svg) {
  width: 16px;
  height: 16px;
}

.action-slot-spacer {
  display: block;
  width: 32px;
  height: 32px;
}

.danger-icon:hover:not(:disabled) {
  color: var(--danger);
}

.error-text {
  margin: 10px 0 0;
  color: var(--danger);
  font-size: 12px;
}

:deep(tr.sortableRowGhost) {
  opacity: 0.45;
}

:deep(tr.sortableRowChosen) {
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}

:deep(.sortableRowHandle) {
  cursor: grab;
}

:deep(.sortableRowHandle:active) {
  cursor: grabbing;
}
</style>
