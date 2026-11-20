<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { appTip } from "../lib/appTip";
import { maskModalOpen, maskModalPopoverZIndex } from "../lib/maskModal";

const el = ref<HTMLElement | null>(null);
const left = ref(0);
const top = ref(0);

const visible = computed(() => !!appTip.value?.text);

/** Chrome tips use --z-tip; tips over a mask modal use the popover layer. */
const tipStyle = computed(() => {
  const z = maskModalOpen.value ? maskModalPopoverZIndex() : undefined;
  return {
    left: `${left.value}px`,
    top: `${top.value}px`,
    ...(z != null ? { zIndex: z } : {}),
  };
});

async function layout() {
  const model = appTip.value;
  const tip = el.value;
  if (!model || !tip) return;
  await nextTick();
  const pad = 4;
  const gap = model.gap ?? 8;
  const width = tip.offsetWidth;
  const height = tip.offsetHeight;
  let x = model.x - width / 2;
  if (x < pad) x = pad;
  if (x + width > window.innerWidth - pad) x = window.innerWidth - pad - width;
  let y =
    model.placement === "below" ? model.y + gap : model.y - gap - height;
  if (y < pad) y = model.y + gap;
  if (y + height > window.innerHeight - pad) {
    y = Math.max(pad, model.y - gap - height);
  }
  left.value = Math.round(x);
  top.value = Math.round(y);
}

watch(appTip, () => {
  void layout();
}, { flush: "post" });

watch(visible, (on) => {
  if (on) void layout();
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible"
      ref="el"
      class="app-tip"
      role="tooltip"
      :style="tipStyle"
    >{{ appTip!.text }}</div>
  </Teleport>
</template>

<style>
.app-tip {
  position: fixed;
  z-index: var(--z-tip);
  box-sizing: border-box;
  max-width: calc(100vw - 8px);
  padding: 2px 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  border-radius: 2px;
  border: 1px solid var(--line);
  background: var(--popup);
  color: var(--popup-ink);
  box-shadow: var(--popup-shadow);
  font-size: 12px;
  line-height: 18px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  pointer-events: none;
}
</style>
