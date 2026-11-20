<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, ref, watch } from "vue";

import {
  getBuiltinRuleDefaults,
  previewChapterExamples,
} from "../lib/chapterRules";
import AppModal from "./AppModal.vue";
import ChapterRuleExampleTag from "./ChapterRuleExampleTag.vue";

const { t } = useI18n();
export type ChapterRuleEditPayload = {
  pattern: string;
  examples: string[];
};

const modelValue = defineModel<boolean>({ default: false });

const props = defineProps<{
  title: string;
  pattern: string;
  examplesText: string;
  builtinRuleId?: string | null;
}>();

const emit = defineEmits<{
  save: [payload: ChapterRuleEditPayload];
}>();

const patternInput = ref(props.pattern);
const examplesInput = ref(props.examplesText);
const patternTemplate = "([\\d.-]+)_\\d+";

function normalizePatternForEdit(raw: string): string {
  return raw.replace(/\r?\n/g, " ").trim();
}

watch(
  () => [props.pattern, props.examplesText, modelValue.value] as const,
  ([pattern, examplesText, opened]) => {
    if (!opened) return;
    patternInput.value = normalizePatternForEdit(pattern);
    examplesInput.value = examplesText;
  },
  { immediate: true },
);

function parseExamples(raw: string): string[] {
  return raw
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
}

const patternMatchPreview = computed(() =>
  previewChapterExamples(patternInput.value, parseExamples(examplesInput.value)),
);

function close() {
  modelValue.value = false;
}

function onSave() {
  emit("save", {
    pattern: normalizePatternForEdit(patternInput.value),
    examples: parseExamples(examplesInput.value),
  });
  close();
}

function onPatternKeydown(e: KeyboardEvent) {
  if (e.key !== "Enter" || e.isComposing) return;
  e.preventDefault();
}

function onPatternPaste(e: ClipboardEvent) {
  e.preventDefault();
  const el = e.target as HTMLTextAreaElement;
  const pasted = (e.clipboardData?.getData("text/plain") ?? "").replace(/\r?\n/g, " ");
  const start = el.selectionStart;
  const end = el.selectionEnd;
  const v = patternInput.value;
  patternInput.value = v.slice(0, start) + pasted + v.slice(end);
  const caret = start + pasted.length;
  void nextTick(() => {
    el.focus();
    el.setSelectionRange(caret, caret);
  });
}

function onResetBuiltinDefault() {
  const id = props.builtinRuleId?.trim();
  if (!id) return;
  const d = getBuiltinRuleDefaults(id);
  if (!d) return;
  patternInput.value = normalizePatternForEdit(d.pattern);
  examplesInput.value = d.examples.join("\n");
}
</script>

<template>
  <AppModal
    v-model="modelValue"
    :title="title"
    max-width="520px"
    :mask-closable="false"
    panel-class="settings-dialog chapter-rule-edit-dialog"
  >
    <i18n-t keypath="chapterRuleEditHint" tag="p" class="edit-hint">
      <template #capture>
        <b>{{ t("chapterRuleTermCapture") }}</b>
      </template>
      <template #name>
        <b>{{ t("chapterRuleTermName") }}</b>
      </template>
      <template #pattern>
        <code class="hint-code">([\d.-]+)_\d+</code>
      </template>
      <template #sample>
        <code class="hint-code">1_01.jpg</code>
      </template>
      <template #chapter>
        <code class="hint-code">1</code>
      </template>
    </i18n-t>

    <div class="field-label-row">
      <label class="field-label field-label-row-text" for="chapterRulePattern">
        <b>{{ t("chapterRulePattern") }}</b>{{ t("chapterRulePatternSuffix") }}
      </label>
      <button
        class="text-link"
        type="button"
        @click="patternInput = patternTemplate"
      >
        {{ t("chapterRuleFillTemplate") }}
      </button>
    </div>
    <textarea
      id="chapterRulePattern"
      v-model="patternInput"
      class="pattern-input"
      rows="3"
      wrap="soft"
      spellcheck="false"
      @keydown="onPatternKeydown"
      @paste="onPatternPaste"
    />

    <div
      v-if="patternMatchPreview.kind !== 'empty'"
      class="pattern-preview"
      aria-live="polite"
    >
      <label class="field-label" for="chapterRulePattern">{{ t("chapterRulePreview") }}</label>
      <div
        v-if="patternMatchPreview.kind === 'error'"
        class="pattern-preview-error"
      >
        {{ t("invalidRule") }}
      </div>
      <div v-else class="pattern-preview-body">
        <ChapterRuleExampleTag
          v-for="(item, i) in patternMatchPreview.items"
          :key="i"
          :item="item"
        />
      </div>
    </div>

    <label class="field-label" for="chapterRuleExamples">
      <b>{{ t("chapterRuleExamples") }}</b>{{ t("chapterRuleExamplesSuffix") }}
    </label>
    <textarea
      id="chapterRuleExamples"
      v-model="examplesInput"
      class="pattern-input"
      rows="3"
      spellcheck="false"
    />

    <template #footer>
      <div class="app-modal-actions">
        <button
          v-if="builtinRuleId"
          class="ghost"
          type="button"
          @click="onResetBuiltinDefault"
        >
          {{ t("chapterRuleResetDefault") }}
        </button>
        <div class="app-modal-actions-end">
          <button class="ghost" type="button" @click="close">{{ t("cancel") }}</button>
          <button class="primary" type="button" @click="onSave">{{ t("ok") }}</button>
        </div>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.edit-hint {
  margin: 0 0 12px;
  font-size: 12px;
  line-height: 1.5;
  color: var(--muted);
}

.edit-hint b {
  font-weight: 600;
}

.hint-code {
  display: inline-block;
  padding: 2px 6px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--line);
  background: var(--field);
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
    "Courier New", monospace;
  font-size: 11px;
  line-height: 1.2;
  vertical-align: baseline;
}

.field-label {
  display: block;
  margin: 10px 0 6px;
  font-size: 12px;
  color: var(--ink);
  font-weight: 400;
}

.field-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin: 10px 0 6px;
}

.field-label-row-text {
  margin: 0;
  flex: 1;
  min-width: 0;
}

button.text-link {
  flex: none;
  margin: 0;
  padding: 0;
  border: none;
  border-radius: 0;
  background: transparent;
  font-size: 12px;
  line-height: 1.2;
  box-shadow: none;
}

button.text-link:hover:not(:disabled) {
  border: none;
  background: transparent;
  color: var(--accent);
  text-decoration: underline;
}

.pattern-input {
  width: 100%;
  min-height: 64px;
  box-sizing: border-box;
  font-family: Consolas, "Courier New", monospace;
  overflow-wrap: anywhere;
  word-break: break-word;
  line-height: 1.45;
}

.pattern-preview {
  margin-bottom: 20px;
  font-size: 12px;
}

.pattern-preview-body {
  min-width: 0;
  word-break: break-all;
  color: var(--info);
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.pattern-preview-error {
  color: var(--danger);
}
</style>
