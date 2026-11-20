<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, ref, watch } from "vue";

import {
  comic,
  imageInfoOpen,
  loadPageFileInfo,
  pageFileInfoSnapshot,
  pageIndex,
  type PageFileInfo,
} from "../reader";
import AppModal from "./AppModal.vue";
import Icon from "./Icon.vue";

const { t } = useI18n();
const info = ref<PageFileInfo | null>(null);
const loading = ref(false);
let refreshGen = 0;

const rows = computed(() => {
  const item = info.value;
  return [
    { key: "name", label: t("infoFileName"), value: item?.name?.trim() || "" },
    { key: "path", label: t("infoPath"), value: item?.path?.trim() || "" },
    { key: "props", label: t("infoProps"), value: item ? formatProps(item) : "" },
    { key: "format", label: t("infoFormat"), value: item?.format?.trim() || "" },
    { key: "color", label: t("infoColor"), value: item?.color?.trim() || "" },
    { key: "size", label: t("infoSize"), value: formatBytes(item?.bytes) },
    { key: "time", label: t("infoTime"), value: formatTime(item?.modifiedMs, item?.modifiedText) },
  ];
});

function formatProps(item: PageFileInfo) {
  const width = item.width ?? 0;
  const height = item.height ?? 0;
  if (!(width > 0 && height > 0)) return "";
  const mp = (width * height) / 1_000_000;
  const size = `${width}×${height} (${mp.toFixed(2)} MP)`;
  const bits = item.bitsPerPixel;
  return bits ? `${size} ${bits}bit` : size;
}

function formatBytes(bytes?: number | null) {
  if (typeof bytes !== "number" || !Number.isFinite(bytes) || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function formatTime(ms?: number | null, text?: string | null) {
  if (typeof ms === "number" && Number.isFinite(ms)) {
    return new Date(ms).toLocaleString();
  }
  return text?.trim() || "";
}

async function refresh() {
  if (!imageInfoOpen.value || !comic.value) {
    refreshGen += 1;
    info.value = null;
    loading.value = false;
    return;
  }
  const token = ++refreshGen;
  info.value = pageFileInfoSnapshot(pageIndex.value);
  loading.value = true;
  try {
    const next = await loadPageFileInfo(pageIndex.value);
    if (token !== refreshGen) return;
    info.value = next;
  } finally {
    if (token === refreshGen) loading.value = false;
  }
}

watch([imageInfoOpen, pageIndex, comic], () => {
  void refresh();
});
</script>

<template>
  <AppModal v-model="imageInfoOpen" :title="t('imageInfo')" max-width="520px" panel-class="image-info-dialog">
    <table v-if="comic" class="info-table">
      <tbody>
        <tr v-for="row in rows" :key="row.key">
          <th scope="row">{{ row.label }}</th>
          <td>
            <template v-if="row.value">{{ row.value }}</template>
            <span v-else-if="loading" class="info-loading" aria-busy="true">
              <Icon name="loadingDotsRotate" />
            </span>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="muted">{{ t('noComic') }}</p>
  </AppModal>
</template>

<style>
.info-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.info-table th,
.info-table td {
  padding: 6px 0;
  line-height: 1.4;
  vertical-align: top;
}

.info-table th {
  width: 1%;
  padding-right: 16px;
  white-space: nowrap;
  text-align: right;
  font-weight: 500;
  color: var(--muted);
}

.info-table td {
  word-break: break-all;
}

.info-loading {
  display: inline-flex;
  align-items: center;
  height: 1.4em;
  overflow: hidden;
  vertical-align: top;
  line-height: 0;
  color: var(--muted);
}

.info-loading .asset-icon,
.info-loading .asset-icon svg {
  display: block;
  width: 1.4em;
  height: 1.4em;
}
</style>
