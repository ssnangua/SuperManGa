/**
 * Shared in-flight asset downloads (runtime / models / OCR).
 * Survives settings-tab unmount; same key is deduped; different keys may run in parallel.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { reactive } from "vue";

export type AssetProgressPayload = {
  kind: string;
  id: string;
  done: number;
  total: number | null;
  percent: number | null;
};

/** Busy flags per job key (`runtime` | `model:{id}` | `ocr:{id}`). */
const busy = reactive<Record<string, true>>({});
/** Progress percent per job key (null = indeterminate). */
const percents = reactive<Record<string, number | null>>({});

const inflight = new Map<string, Promise<unknown>>();
let progressUnlisten: UnlistenFn | null = null;

export function assetJobKey(kind: string, id: string): string {
  if (kind === "runtime") return "runtime";
  return `${kind}:${id}`;
}

export function isAssetDownloading(key: string): boolean {
  return !!busy[key];
}

export function assetDownloadPercentFor(key: string): number | null {
  return Object.prototype.hasOwnProperty.call(percents, key) ? percents[key] : null;
}

async function ensureProgressListen(): Promise<void> {
  if (progressUnlisten) return;
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  progressUnlisten = await listen<AssetProgressPayload>("asset-progress", (event) => {
    const key = assetJobKey(event.payload.kind, event.payload.id);
    if (!inflight.has(key)) return;
    percents[key] = event.payload.percent;
  });
}

/**
 * Run a download job. Concurrent calls with the same key share one Promise;
 * different keys run in parallel.
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
  busy[key] = true;
  percents[key] = null;

  const promise = (async () => {
    try {
      return await run();
    } finally {
      inflight.delete(key);
      delete busy[key];
      delete percents[key];
    }
  })();

  inflight.set(key, promise);
  return promise;
}
