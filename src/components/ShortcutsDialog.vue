<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";

import {
  acceleratorToDisplayKeys,
  acceleratorToDisplayText,
  collectShortcutConflicts,
  FIXED_DEVTOOLS_ACCEL,
  FIXED_LOGS_ACCEL,
  isListedShortcut,
  keyboardEventToAccelerator,
  SHORTCUT_ACTIONS,
  type ShortcutActionId,
} from "../lib/shortcuts";
import { isMacDesktop } from "../lib/platform";
import {
  isShortcutAccelOccupied,
  resetShortcutBindings,
  setShortcutBinding,
  shortcutBindings,
  shortcutsOpen,
} from "../reader";
import AppModal from "./AppModal.vue";
import Icon from "./Icon.vue";

const { t } = useI18n();
const mac = isMacDesktop();
type FixedRow = {
  id: string;
  label: () => string;
  keys: string[];
  icon?: string;
};

const fixedRows: FixedRow[] = [
  { id: "bounds", label: () => t("shortcutFixedBounds"), keys: acceleratorToDisplayKeys("Alt", mac), icon: "image" },
  { id: "logs", label: () => t("logs"), keys: acceleratorToDisplayKeys(FIXED_LOGS_ACCEL, mac), icon: "terminal" },
  {
    id: "devtools",
    label: () => t("shortcutFixedDevtools"),
    keys: acceleratorToDisplayKeys(FIXED_DEVTOOLS_ACCEL, mac),
    icon: "devtools",
  },
];

const listedActions = computed(() => SHORTCUT_ACTIONS.filter((action) => isListedShortcut(action.id)));
const conflicts = computed(() => new Set(collectShortcutConflicts(shortcutBindings.value)));

const editing = ref(false);
const editingId = ref<ShortcutActionId | null>(null);
const draft = ref("");
const occupied = ref(false);
const recordBox = ref<HTMLElement | null>(null);

function keysFor(action: ShortcutActionId): string[] {
  return acceleratorToDisplayKeys(shortcutBindings.value[action], mac);
}

function close() {
  cancelEdit();
  shortcutsOpen.value = false;
}

async function startEdit(action: ShortcutActionId) {
  editingId.value = action;
  draft.value = "";
  occupied.value = false;
  editing.value = true;
  await nextTick();
  recordBox.value?.focus();
}

function cancelEdit() {
  editing.value = false;
  editingId.value = null;
  draft.value = "";
  occupied.value = false;
}

function confirmEdit() {
  if (!editingId.value || !draft.value) return;
  const ok = setShortcutBinding(editingId.value, draft.value);
  if (!ok) {
    occupied.value = true;
    return;
  }
  cancelEdit();
}

function onRecordKey(event: KeyboardEvent) {
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    cancelEdit();
    return;
  }
  if (event.key === "Enter") {
    confirmEdit();
    return;
  }
  const accel = keyboardEventToAccelerator(event);
  if (!accel || accel === "Escape") return;
  draft.value = accel;
  // Live conflict feedback: show as soon as a chord is pressed.
  occupied.value = isShortcutAccelOccupied(accel, editingId.value ?? undefined);
}

function resetAll() {
  resetShortcutBindings();
  cancelEdit();
}

let removeRecordEsc: (() => void) | undefined;

watch(shortcutsOpen, (open) => {
  if (!open) cancelEdit();
});

watch(editing, (on) => {
  removeRecordEsc?.();
  removeRecordEsc = undefined;
  if (!on) return;
  const onEsc = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    cancelEdit();
  };
  window.addEventListener("keydown", onEsc, true);
  removeRecordEsc = () => window.removeEventListener("keydown", onEsc, true);
});

onBeforeUnmount(() => {
  removeRecordEsc?.();
});
</script>

<template>
  <AppModal
    v-model="shortcutsOpen"
    :title="t('shortcutsTitle')"
    max-width="560px"
    :body-scroll="false"
    panel-class="shortcuts-dialog"
  >
    <div class="shortcut-table-block">
      <div class="shortcut-table-head">
        <table class="shortcut-table">
          <colgroup>
            <col class="col-icon" />
            <col class="col-desc" />
            <col class="col-keys" />
          </colgroup>
          <thead>
            <tr>
              <th class="col-icon" aria-hidden="true"></th>
              <th class="col-desc">{{ t('shortcutColAction') }}</th>
              <th class="col-keys">{{ t('shortcutColKeys') }}</th>
            </tr>
          </thead>
        </table>
      </div>
      <div class="shortcut-table-wrap">
        <table class="shortcut-table">
          <colgroup>
            <col class="col-icon" />
            <col class="col-desc" />
            <col class="col-keys" />
          </colgroup>
          <tbody>
            <tr
              v-for="action in listedActions"
              :key="action.id"
              class="shortcut-row"
              :class="{ conflict: conflicts.has(action.id) }"
              :title="t('shortcutEdit')"
              tabindex="0"
              @click="startEdit(action.id)"
              @keydown.enter.prevent="startEdit(action.id)"
              @keydown.space.prevent="startEdit(action.id)"
            >
              <td class="shortcut-icon" :class="{ 'rotate-ccw': action.id === 'rotateCcw' }">
                <Icon v-if="action.icon" :name="action.icon" />
              </td>
              <td class="shortcut-desc">{{ t(action.labelKey) }}</td>
              <td class="shortcut-keys">
                <kbd
                  v-for="key in keysFor(action.id)"
                  :key="`${action.id}-${key}`"
                  class="shortcut-key"
                >{{ key }}</kbd>
              </td>
            </tr>
            <tr v-for="row in fixedRows" :key="row.id" class="fixed">
              <td class="shortcut-icon">
                <Icon v-if="row.icon" :name="row.icon" />
              </td>
              <td class="shortcut-desc">{{ row.label() }}</td>
              <td class="shortcut-keys">
                <kbd
                  v-for="key in row.keys"
                  :key="`${row.id}-${key}`"
                  class="shortcut-key"
                >{{ key }}</kbd>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <template #footer>
      <div class="shortcut-footer">
        <button type="button" class="ghost" @click="resetAll">{{ t('shortcutReset') }}</button>
        <button type="button" class="ghost" @click="close">{{ t('close') }}</button>
      </div>
    </template>
  </AppModal>

  <AppModal
    v-model="editing"
    max-width="420px"
    :mask-closable="true"
    panel-class="shortcut-record-dialog"
  >
    <p class="shortcut-record-hint">{{ t('shortcutRecordHint') }}</p>
    <div
      ref="recordBox"
      class="shortcut-record-box"
      tabindex="0"
      @keydown="onRecordKey"
    >
      <span v-if="draft" class="shortcut-record-value">{{ acceleratorToDisplayText(draft) }}</span>
      <span class="shortcut-record-caret" aria-hidden="true">|</span>
    </div>
    <p v-if="occupied" class="shortcut-record-error">{{ t('shortcutOccupied') }}</p>
  </AppModal>
</template>

<style>
.app-modal-panel.shortcuts-dialog {
  overflow: hidden;
}

.app-modal-panel.shortcuts-dialog .app-modal-body {
  gap: 0;
}

.app-modal-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}

.app-modal-actions.end {
  justify-content: flex-end;
}

.app-modal-actions-end {
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 8px;
  margin-left: auto;
}

.shortcut-table-block {
  flex: 0 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 0;
}

.shortcut-table-head {
  flex: none;
  /* Keep header columns aligned with body when the classic scrollbar is always shown. */
  padding-right: 28px;
  box-sizing: border-box;
}

.shortcut-table-wrap {
  flex: 1 1 auto;
  overflow-x: hidden;
  overflow-y: scroll;
  min-height: 0;
  box-sizing: border-box;
  /* Space between keycaps and scrollbar track (not outside the dialog). */
  padding-right: 10px;
}

.shortcut-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}

.shortcut-table col.col-icon {
  width: 28px;
}

.shortcut-table col.col-keys {
  width: 42%;
}

.shortcut-table th,
.shortcut-table td {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line);
  vertical-align: middle;
}

.shortcut-table thead th {
  color: var(--muted);
  font-size: 12px;
  font-weight: 600;
  background: var(--dialog);
}

.shortcut-table .col-icon {
  width: 28px;
  padding-right: 0;
}

.shortcut-table .col-desc {
  text-align: left;
}

.shortcut-table .col-keys {
  width: 42%;
  text-align: right;
}

.shortcut-icon {
  width: 28px;
  color: var(--muted);
  vertical-align: middle;
}

.shortcut-icon.rotate-ccw .asset-icon {
  transform: scaleX(-1);
}

.shortcut-desc {
  color: var(--ink);
  font-size: 13px;
  padding-right: 10px;
}

.shortcut-keys {
  text-align: right;
  white-space: nowrap;
}

.shortcut-key {
  display: inline-block;
  position: relative;
  padding: 4px 8px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--line);
  background: var(--field);
  color: var(--ink);
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
    "Courier New", monospace;
  font-size: 12px;
  line-height: 1;
}

.shortcut-keys:not(.alt) .shortcut-key + .shortcut-key {
  margin-left: 16px;
}

.shortcut-keys:not(.alt) .shortcut-key + .shortcut-key::before {
  content: "+";
  position: absolute;
  left: -12px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--muted);
  font-size: 12px;
  font-weight: 600;
  font-family: inherit;
}

.shortcut-or {
  display: inline-block;
  margin: 0 8px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 600;
  vertical-align: middle;
}

.shortcut-table tr.shortcut-row {
  cursor: pointer;
}

.shortcut-table tr.shortcut-row:hover,
.shortcut-table tr.shortcut-row:focus-visible {
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}

.shortcut-table tr.shortcut-row:focus-visible {
  outline: none;
}

.shortcut-table tr.conflict .shortcut-desc,
.shortcut-table tr.conflict .shortcut-key {
  color: #c44;
}

.shortcut-table tr.fixed .shortcut-desc {
  color: var(--muted);
}

.shortcut-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  padding: 0;
  margin: 0;
  flex: none;
}

.shortcut-record-hint {
  margin: 0;
  color: var(--ink);
  font-size: 13px;
  line-height: 1.45;
}

.shortcut-record-box {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 40px;
  padding: 8px 12px;
  border: 1px solid var(--accent);
  border-radius: var(--radius-lg);
  background: var(--dialog);
  outline: none;
  box-shadow: none;
}

.shortcut-record-box:focus,
.shortcut-record-box:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: none;
}

.shortcut-record-value {
  color: var(--ink);
  font-size: 14px;
  line-height: 1;
  white-space: nowrap;
}

.shortcut-record-caret {
  color: var(--ink);
  font-size: 14px;
  line-height: 1;
  animation: shortcut-caret-blink 1s step-end infinite;
}

@keyframes shortcut-caret-blink {
  50% {
    opacity: 0;
  }
}

.shortcut-record-error {
  margin: 0;
  color: #c44;
  font-size: 12px;
}
</style>
