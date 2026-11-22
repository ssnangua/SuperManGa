/**
 * Shared in-flight asset downloads (runtime / models / OCR).
 * Survives settings-tab unmount and dedupes concurrent starts of the same job.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ref } from "vue";

export type AssetProgressPayload = {
  kind: string;
  id: string;
  done: number;
  total: number | null;
  percent: number | null;
};

/** Active UI job key: `runtime` | `model:{id}` | `ocr:{id}`. */
export const assetDownloadKey = ref<string | null>(null);
export const assetDownloadPercent = ref<number | null>(null);

const inflight = new Map<string, Promise<unknown>>();
let progressUnlisten: UnlistenFn | null = null;

export function assetJobKey(kind: string, id: string): string {
  if (kind === "runtime") return "runtime";
  return `${kind}:${id}`;
}

export function isAssetDownloading(key: string): boolean {
  // Read reactive key so Vue templates/computeds re-render on start/finish.
  const active = assetDownloadKey.value;
  return active === key || inflight.has(key);
}

export function assetDownloadPercentFor(key: string): number | null {
  if (assetDownloadKey.value !== key) return null;
  return assetDownloadPercent.value;
}

async function ensureProgressListen(): Promise<void> {
  if (progressUnlisten) return;
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  progressUnlisten = await listen<AssetProgressPayload>("asset-progress", (event) => {
    const key = assetJobKey(event.payload.kind, event.payload.id);
    if (!inflight.has(key)) return;
    if (assetDownloadKey.value !== key) {
      // Prefer showing the job that just emitted if UI key is stale/empty.
      if (!assetDownloadKey.value || !inflight.has(assetDownloadKey.value)) {
        assetDownloadKey.value = key;
      }
    }
    if (assetDownloadKey.value === key) {
      assetDownloadPercent.value = event.payload.percent;
    }
  });
}

/**
 * Run a download job. Concurrent calls with the same key share one Promise.
 */
export function runAssetDownload<T>(
  kind: string,
  id: string,
  run: () => Promise<T>,
): Promise<T> {
  const key = assetJobKey(kind, id);
  const existing = inflight.get(key);
  if (existing) return existing as Promise<T>;

  void ensureProgressListen();
  assetDownloadKey.value = key;
  assetDownloadPercent.value = null;

  const promise = (async () => {
    try {
      return await run();
    } finally {
      inflight.delete(key);
      if (assetDownloadKey.value === key) {
        const next = inflight.keys().next();
        assetDownloadKey.value = next.done ? null : next.value;
        assetDownloadPercent.value = null;
      }
    }
  })();

  inflight.set(key, promise);
  return promise;
}
