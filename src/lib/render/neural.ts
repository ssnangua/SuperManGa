import { computed, ref, shallowRef } from "vue";
import {
  cancelNeuralUpscale,
  neuralResultLoad,
  neuralResultsExists,
  neuralUpscale,
} from "./assets";
import { ANIMATED_IMAGE_SKIP, isAnimatedImage, isAnimatedImageSkip } from "./animated";
import { radiateBatchRank } from "../radiateRank";
import { spreadIndexes, type ReadingMode } from "../spread";

/** Interactive draw (current page). */
export const NEURAL_PRIORITY_CURRENT = 0;
/** Prefetch next page after current. */
export const NEURAL_PRIORITY_NEXT = 1;
/** Prefetch previous page after next. */
export const NEURAL_PRIORITY_PREV = 2;
/** Full-book background fill. */
export const NEURAL_PRIORITY_BATCH = 3;

export const NEURAL_TARGET_SIZE_MIN = 512;
export const NEURAL_TARGET_SIZE_MAX = 8192;
export const NEURAL_TARGET_SIZE_DEFAULT = 1600;

export function clampNeuralTargetSize(value: unknown): number {
  const n = Math.round(Number(value));
  if (!(Number.isFinite(n) && n > 0)) return NEURAL_TARGET_SIZE_DEFAULT;
  return Math.min(NEURAL_TARGET_SIZE_MAX, Math.max(NEURAL_TARGET_SIZE_MIN, n));
}

/**
 * Threshold check for whether neural upscale should run.
 * Returns a placeholder size used only for queue bookkeeping; actual output keeps
 * the model's native 2x/4x scale (typically larger than T).
 * Returns null when both sides are already >= T.
 */
export function resolveNeuralOutputSize(
  naturalWidth: number,
  naturalHeight: number,
  targetSize: number,
): { width: number; height: number } | null {
  const T = clampNeuralTargetSize(targetSize);
  if (!(naturalWidth > 0) || !(naturalHeight > 0)) return null;
  if (naturalWidth >= T && naturalHeight >= T) return null;
  const longEdge = Math.max(naturalWidth, naturalHeight);
  if (!(T / longEdge > 1)) return null;
  // Placeholder only — backend keeps model-native output dimensions.
  return { width: naturalWidth, height: naturalHeight };
}

const MAX_CACHE_ENTRIES = 12;
const MAX_CACHE_BYTES = 256 * 1024 * 1024;

export type NeuralFrame = ImageData | ImageBitmap;

type CacheEntry = {
  data: NeuralFrame;
  bytes: number;
  lastUsed: number;
};

const cache = new Map<string, CacheEntry>();
let cacheBytes = 0;

type Waiter = {
  resolve: (data: NeuralFrame) => void;
  reject: (error: unknown) => void;
};

type PendingJob = {
  key: string;
  assetId: string;
  pageIndex: number;
  loadPng: () => Promise<Uint8Array>;
  width: number;
  height: number;
  priority: number;
  jobId: number;
  requeueOnCancel: boolean;
  waiters: Waiter[];
};

const pending = new Map<string, PendingJob>();
let runningKey: string | null = null;
let runningJobId = 0;
let runningPageIndex = -1;
let runningPriority = NEURAL_PRIORITY_BATCH;
let pumping = false;
let pumpEpoch = 0;
let dropping = false;
let nextJobId = 1;
let focusIndex = 0;
let pageTotal = 0;
let focusMode: ReadingMode = "single";
let focusIsolateCover = false;

export const neuralBatchPaused = ref(false);
export const neuralRunningIndex = ref<number | null>(null);
const doneFlags = shallowRef<boolean[]>([]);
const skipFlags = shallowRef<boolean[]>([]);
const marksEpoch = ref(0);

function bumpMarks() {
  marksEpoch.value += 1;
}

function resizeMarks(count: number) {
  if (doneFlags.value.length === count && skipFlags.value.length === count) return;
  doneFlags.value = Array.from({ length: count }, (_, i) => doneFlags.value[i] === true);
  skipFlags.value = Array.from({ length: count }, (_, i) => skipFlags.value[i] === true);
  bumpMarks();
}

function markDone(index: number) {
  if (index < 0) return;
  resizeMarks(Math.max(pageTotal, index + 1));
  if (doneFlags.value[index]) return;
  const next = doneFlags.value.slice();
  next[index] = true;
  doneFlags.value = next;
  bumpMarks();
}

function markSkip(index: number) {
  if (index < 0) return;
  resizeMarks(Math.max(pageTotal, index + 1));
  if (skipFlags.value[index] && doneFlags.value[index]) return;
  const skips = skipFlags.value.slice();
  const dones = doneFlags.value.slice();
  skips[index] = true;
  dones[index] = true;
  skipFlags.value = skips;
  doneFlags.value = dones;
  bumpMarks();
}

function resetMarks(count: number) {
  pageTotal = count;
  doneFlags.value = Array.from({ length: count }, () => false);
  skipFlags.value = Array.from({ length: count }, () => false);
  neuralRunningIndex.value = null;
  bumpMarks();
}

export const neuralDoneFlags = computed(() => {
  void marksEpoch.value;
  return doneFlags.value;
});

export const neuralSkipFlags = computed(() => {
  void marksEpoch.value;
  return skipFlags.value;
});

export function neuralResultUrl(key: string): string {
  if (!key) return "";
  const origin = /Windows|Android/i.test(navigator.userAgent)
    ? "http://neural.localhost"
    : "neural://localhost";
  const path = key
    .replace(/\\/g, "/")
    .split("/")
    .filter(Boolean)
    .map((part) => encodeURIComponent(part))
    .join("/");
  return `${origin}/${path}`;
}

export const neuralSeekActive = computed(() => {
  void marksEpoch.value;
  if (pageTotal <= 0) return false;
  return neuralRunningIndex.value != null || pending.size > 0;
});

export const neuralSeekGradient = computed(() => {
  void marksEpoch.value;
  const n = pageTotal;
  if (n <= 0) return "none";
  const running = neuralRunningIndex.value;
  const done = doneFlags.value;
  type Seg = { start: number; end: number; color: string };
  const segs: Seg[] = [];
  let i = 0;
  while (i < n) {
    const color =
      i === running ? "var(--neural-running)" : done[i] ? "var(--neural-done)" : "";
    if (!color) {
      i += 1;
      continue;
    }
    let end = i + 1;
    while (
      end < n &&
      (end === running
        ? "var(--neural-running)"
        : done[end]
          ? "var(--neural-done)"
          : "") === color
    ) {
      end += 1;
    }
    segs.push({ start: i, end, color });
    i = end;
  }
  if (!segs.length) return "none";
  /** Same ticks as the seek fill: page i sits at i/(n-1), so page i occupies [i, i+1]. */
  const pct = (index: number) => {
    if (n <= 1) return index <= 0 ? 0 : 100;
    return Math.min(100, (index / (n - 1)) * 100);
  };
  const parts: string[] = [];
  let cursor = 0;
  for (const seg of segs) {
    if (seg.start > cursor) {
      parts.push(`transparent ${pct(cursor)}%`, `transparent ${pct(seg.start)}%`);
    }
    parts.push(`${seg.color} ${pct(seg.start)}%`, `${seg.color} ${pct(seg.end)}%`);
    cursor = seg.end;
  }
  if (cursor < n) {
    parts.push(`transparent ${pct(cursor)}%`, "transparent 100%");
  }
  return `linear-gradient(to right, ${parts.join(", ")})`;
});

function isNeuralBitmap(data: NeuralFrame): data is ImageBitmap {
  return typeof ImageBitmap !== "undefined" && data instanceof ImageBitmap;
}

function entryBytes(data: NeuralFrame) {
  if (isNeuralBitmap(data)) return Math.max(1, data.width * data.height * 4);
  return data.data.byteLength;
}

function closeFrame(data: NeuralFrame) {
  if (!isNeuralBitmap(data)) return;
  try {
    data.close();
  } catch {
    // Bitmap may already be closed.
  }
}

function touch(key: string, entry: CacheEntry) {
  entry.lastUsed = performance.now();
  cache.delete(key);
  cache.set(key, entry);
}

function evictIfNeeded() {
  while (cache.size > MAX_CACHE_ENTRIES || cacheBytes > MAX_CACHE_BYTES) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    const entry = cache.get(oldest);
    cache.delete(oldest);
    if (entry) {
      cacheBytes -= entry.bytes;
      closeFrame(entry.data);
    }
  }
}

function getCached(key: string): NeuralFrame | null {
  if (!key) return null;
  const entry = cache.get(key);
  if (!entry) return null;
  touch(key, entry);
  return entry.data;
}

function putCached(key: string, data: NeuralFrame) {
  if (!key) return;
  const existing = cache.get(key);
  if (existing) {
    cacheBytes -= existing.bytes;
    cache.delete(key);
    if (existing.data !== data) closeFrame(existing.data);
  }
  const bytes = entryBytes(data);
  cacheBytes += bytes;
  cache.set(key, { data, bytes, lastUsed: performance.now() });
  evictIfNeeded();
}

export function clearNeuralUpscaleCache() {
  for (const entry of cache.values()) closeFrame(entry.data);
  cache.clear();
  cacheBytes = 0;
}

export function isDetachedImageError(error: unknown): boolean {
  if (!error) return false;
  const name = typeof error === "object" && error && "name" in error ? String((error as { name: unknown }).name) : "";
  const message = error instanceof Error ? error.message : String(error);
  return name === "InvalidStateError" || /detached|InvalidStateError/i.test(message);
}

export function isNeuralCancelled(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error ?? "");
  return message === "cancelled" || /cancelled/i.test(message);
}

function canvasFromImage(
  image: HTMLImageElement | ImageBitmap,
  width: number,
  height: number,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas unsupported");
  ctx.drawImage(image, 0, 0, width, height);
  return canvas;
}

export async function imageToPngBytes(image: HTMLImageElement | ImageBitmap): Promise<Uint8Array> {
  const width = "naturalWidth" in image ? image.naturalWidth : image.width;
  const height = "naturalHeight" in image ? image.naturalHeight : image.height;
  if (width < 1 || height < 1) {
    throw new DOMException("The image source is detached", "InvalidStateError");
  }
  const canvas = canvasFromImage(image, width, height);
  const blob = await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((value) => (value ? resolve(value) : reject(new Error("png encode failed"))), "image/png");
  });
  return new Uint8Array(await blob.arrayBuffer());
}

async function srcToBitmapAndPng(src: string): Promise<{
  width: number;
  height: number;
  png: Uint8Array;
}> {
  // Prefer fetch (works with CORS + CSP connect-src). Fall back to Image when
  // packaged CSP blocks custom-protocol fetch — same idea as PageMedia.ensureSource.
  try {
    const response = await fetch(src);
    if (!response.ok) throw new Error(`fetch ${response.status}`);
    const blob = await response.blob();
    const bytes = new Uint8Array(await blob.arrayBuffer());
    if (isAnimatedImage(bytes)) throw new Error(ANIMATED_IMAGE_SKIP);
    const bitmap = await createImageBitmap(blob);
    try {
      const png = await imageToPngBytes(bitmap);
      return { width: bitmap.width, height: bitmap.height, png };
    } finally {
      bitmap.close();
    }
  } catch (error) {
    if (isAnimatedImageSkip(error)) throw error;
    const image = new Image();
    image.crossOrigin = "anonymous";
    image.decoding = "async";
    image.src = src;
    await image.decode().catch(() => undefined);
    if (!image.naturalWidth) throw error instanceof Error ? error : new Error(String(error));
    const png = await imageToPngBytes(image);
    return { width: image.naturalWidth, height: image.naturalHeight, png };
  }
}

function focusSpread(focus: number, count: number): number[] {
  if (count <= 0) return [];
  if (focusMode !== "double") {
    const clamped = Math.max(0, Math.min(count - 1, focus));
    return [clamped];
  }
  return spreadIndexes(focus, count, "double", focusIsolateCover);
}

function neighborSpread(from: number, count: number): number[] {
  if (count <= 0 || from < 0 || from >= count) return [];
  return focusMode === "double"
    ? spreadIndexes(from, count, "double", focusIsolateCover)
    : [from];
}

function protectedSet(focus: number, count: number): Set<number> {
  const set = new Set<number>();
  if (count <= 0) return set;
  const current = focusSpread(focus, count);
  for (const page of current) set.add(page);
  const first = current[0] ?? focus;
  const last = current[current.length - 1] ?? focus;
  if (last + 1 < count) {
    for (const page of neighborSpread(last + 1, count)) set.add(page);
  }
  if (first > 0) {
    for (const page of neighborSpread(first - 1, count)) set.add(page);
  }
  return set;
}

function priorityForIndex(index: number, focus: number, count: number): number {
  if (count <= 0) return NEURAL_PRIORITY_BATCH;
  const current = focusSpread(focus, count);
  if (current.includes(index)) return NEURAL_PRIORITY_CURRENT;
  const first = current[0] ?? focus;
  const last = current[current.length - 1] ?? focus;
  if (last + 1 < count && neighborSpread(last + 1, count).includes(index)) {
    return NEURAL_PRIORITY_NEXT;
  }
  if (first > 0 && neighborSpread(first - 1, count).includes(index)) {
    return NEURAL_PRIORITY_PREV;
  }
  return NEURAL_PRIORITY_BATCH;
}

function batchRank(index: number, focus: number, count: number): number {
  const current = focusSpread(focus, count);
  const lo = current[0] ?? focus;
  const hi = current[current.length - 1] ?? focus;
  return radiateBatchRank(index, lo, hi);
}

function pickNextJob(): PendingJob | null {
  let best: PendingJob | null = null;
  for (const job of pending.values()) {
    if (job.key === runningKey) continue;
    if (neuralBatchPaused.value && job.priority >= NEURAL_PRIORITY_BATCH) continue;
    if (!best) {
      best = job;
      continue;
    }
    if (job.priority < best.priority) {
      best = job;
      continue;
    }
    if (job.priority === best.priority && job.priority >= NEURAL_PRIORITY_BATCH) {
      if (batchRank(job.pageIndex, focusIndex, pageTotal) < batchRank(best.pageIndex, focusIndex, pageTotal)) {
        best = job;
      }
    }
  }
  return best;
}

function restampPriorities() {
  const protect = protectedSet(focusIndex, pageTotal);
  for (const job of pending.values()) {
    job.priority = priorityForIndex(job.pageIndex, focusIndex, pageTotal);
  }
  if (runningKey && runningPageIndex >= 0 && !protect.has(runningPageIndex)) {
    void cancelNeuralUpscale(runningJobId);
  }
  if (neuralBatchPaused.value && runningPriority >= NEURAL_PRIORITY_BATCH && runningJobId) {
    void cancelNeuralUpscale(runningJobId);
  }
}

async function pumpQueue() {
  if (pumping) return;
  pumping = true;
  const epoch = pumpEpoch;
  try {
    while (epoch === pumpEpoch) {
      const job = pickNextJob();
      if (!job) break;
      runningKey = job.key;
      runningJobId = job.jobId;
      runningPageIndex = job.pageIndex;
      runningPriority = job.priority;
      neuralRunningIndex.value = job.pageIndex;
      bumpMarks();
      let requeue: PendingJob | null = null;
      try {
        if (epoch !== pumpEpoch) break;
        const png = await job.loadPng();
        if (epoch !== pumpEpoch) break;
        if (job.width < 1 || job.height < 1) {
          markSkip(job.pageIndex);
          continue;
        }
        const result = await neuralUpscale(job.assetId, png, job.width, job.height, {
          cacheKey: job.key,
          jobId: job.jobId,
        });
        if (epoch !== pumpEpoch) break;
        const data = await bitmapFromUpscaleResult(job.key, result.webp);
        putCached(job.key, data);
        markDone(job.pageIndex);
        for (const waiter of job.waiters) waiter.resolve(data);
      } catch (error) {
        if (isAnimatedImageSkip(error)) {
          markSkip(job.pageIndex);
          for (const waiter of job.waiters) waiter.reject(error);
          continue;
        }
        const cancelled = isNeuralCancelled(error);
        if (cancelled && job.requeueOnCancel && !dropping && epoch === pumpEpoch) {
          requeue = {
            ...job,
            priority: NEURAL_PRIORITY_BATCH,
            jobId: nextJobId++,
            waiters: [],
          };
        }
        for (const waiter of job.waiters) waiter.reject(error);
      } finally {
        pending.delete(job.key);
        runningKey = null;
        runningJobId = 0;
        runningPageIndex = -1;
        runningPriority = NEURAL_PRIORITY_BATCH;
        neuralRunningIndex.value = null;
        bumpMarks();
      }
      if (
        requeue
        && epoch === pumpEpoch
        && !dropping
        && !pending.has(requeue.key)
        && !getCached(requeue.key)
      ) {
        pending.set(requeue.key, requeue);
      }
    }
  } finally {
    pumping = false;
    if (!dropping && pickNextJob()) void pumpQueue();
  }
}

async function webpToBitmap(webp: number[] | Uint8Array): Promise<ImageBitmap> {
  const bytes = webp instanceof Uint8Array ? webp : Uint8Array.from(webp);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return createImageBitmap(new Blob([copy], { type: "image/webp" }));
}

/** Load a cached result via custom protocol (avoids multi‑MB IPC on Linux WebKit). */
async function webpFromDiskKey(key: string): Promise<ImageBitmap> {
  const url = neuralResultUrl(key);
  if (!url) throw new Error("empty neural cache key");
  const response = await fetch(url);
  if (!response.ok) throw new Error(`neural cache fetch ${response.status}`);
  const blob = await response.blob();
  return createImageBitmap(blob);
}

async function bitmapFromUpscaleResult(
  key: string,
  webp: number[] | Uint8Array | null | undefined,
): Promise<ImageBitmap> {
  if (webp && webp.length > 0) return webpToBitmap(webp);
  if (key) return webpFromDiskKey(key);
  throw new Error("neural upscale returned empty webp");
}

async function loadDiskHit(assetId: string, key: string): Promise<NeuralFrame | null> {
  if (!key) return null;
  const mem = getCached(key);
  if (mem) return mem;
  try {
    // Probe via IPC first — a protocol fetch 404 is harmless but spams the console.
    const [exists] = await neuralResultsExists([key]);
    if (!exists) return null;
    // Prefer protocol fetch; fall back to IPC only if protocol fails.
    try {
      const bitmap = await webpFromDiskKey(key);
      putCached(key, bitmap);
      return bitmap;
    } catch {
      const loaded = await neuralResultLoad(assetId, key);
      if (!loaded?.webp?.length) return null;
      const bitmap = await webpToBitmap(loaded.webp);
      putCached(key, bitmap);
      return bitmap;
    }
  } catch {
    return null;
  }
}

export async function loadNeuralDiskHit(assetId: string, key: string): Promise<NeuralFrame | null> {
  return loadDiskHit(assetId, key);
}

function ignoreIdleQueueError(error: unknown) {
  if (isNeuralCancelled(error) || isAnimatedImageSkip(error)) return;
  console.error("neural queue", error);
}

function enqueue(
  key: string,
  assetId: string,
  width: number,
  height: number,
  priority: number,
  loadPng: () => Promise<Uint8Array>,
  pageIndex: number,
  requeueOnCancel: boolean,
): Promise<NeuralFrame> {
  const hit = getCached(key);
  if (hit) {
    markDone(pageIndex);
    return Promise.resolve(hit);
  }

  const existing = pending.get(key);
  if (existing) {
    existing.priority = Math.min(existing.priority, priority);
    existing.pageIndex = pageIndex;
    if (priority < NEURAL_PRIORITY_BATCH) existing.requeueOnCancel = false;
    return new Promise<NeuralFrame>((resolve, reject) => {
      existing.waiters.push({ resolve, reject });
    });
  }

  return new Promise<NeuralFrame>((resolve, reject) => {
    pending.set(key, {
      key,
      assetId,
      pageIndex,
      loadPng,
      width,
      height,
      priority,
      jobId: nextJobId++,
      requeueOnCancel,
      waiters: [{ resolve, reject }],
    });
    restampPriorities();
    bumpMarks();
    void pumpQueue();
  });
}

export async function runNeuralUpscale(
  image: HTMLImageElement | ImageBitmap | Uint8Array,
  assetId: string,
  targetWidth: number,
  targetHeight: number,
  options?: {
    cacheKey?: string;
    pageIndex?: number;
    priority?: number;
    requeueOnCancel?: boolean;
  },
): Promise<NeuralFrame> {
  const key = options?.cacheKey ?? "";
  const priority = options?.priority ?? NEURAL_PRIORITY_CURRENT;
  const pageIndex = options?.pageIndex ?? -1;
  const disk = await loadDiskHit(assetId, key);
  if (disk) {
    markDone(pageIndex);
    return disk;
  }
  const png = image instanceof Uint8Array ? image : await imageToPngBytes(image);
  return enqueue(
    key || `mem:${assetId}:${targetWidth}x${targetHeight}:${Math.random()}`,
    assetId,
    targetWidth,
    targetHeight,
    priority,
    () => Promise.resolve(png),
    pageIndex,
    options?.requeueOnCancel ?? false,
  );
}

export async function neuralUpscaleForSrc(
  src: string,
  assetId: string,
  targetSize: number,
  options?: { cacheKey?: string; pageIndex?: number },
): Promise<NeuralFrame | null> {
  if (!src) return null;
  let width: number;
  let height: number;
  let png: Uint8Array;
  try {
    ({ width, height, png } = await srcToBitmapAndPng(src));
  } catch (error) {
    if (isAnimatedImageSkip(error)) {
      if (options?.pageIndex != null) markSkip(options.pageIndex);
      return null;
    }
    throw error;
  }
  const out = resolveNeuralOutputSize(width, height, targetSize);
  if (!out) {
    if (options?.pageIndex != null) markSkip(options.pageIndex);
    return null;
  }
  return runNeuralUpscale(png, assetId, out.width, out.height, {
    cacheKey: options?.cacheKey,
    pageIndex: options?.pageIndex,
    priority: NEURAL_PRIORITY_CURRENT,
  });
}

export function prefetchNeuralUpscale(
  src: string,
  assetId: string,
  targetSize: number,
  priority: number,
  options?: { cacheKey?: string; pageIndex?: number },
): void {
  if (!src) return;
  const T = clampNeuralTargetSize(targetSize);
  const pageIndex = options?.pageIndex ?? -1;
  void (async () => {
    try {
      const jobKey = options?.cacheKey || "";
      if (jobKey) {
        const disk = await loadDiskHit(assetId, jobKey);
        if (disk) {
          markDone(pageIndex);
          return;
        }
      }
      const { width, height, png } = await srcToBitmapAndPng(src);
      const out = resolveNeuralOutputSize(width, height, T);
      if (!out) {
        markSkip(pageIndex);
        return;
      }
      const resolvedKey = jobKey || `${assetId}:${pageIndex}:${out.width}x${out.height}`;
      if (getCached(resolvedKey) || pending.has(resolvedKey)) {
        const job = pending.get(resolvedKey);
        if (job) job.priority = Math.min(job.priority, priority);
        return;
      }
      await enqueue(
        resolvedKey,
        assetId,
        out.width,
        out.height,
        priority,
        () => Promise.resolve(png),
        pageIndex,
        true,
      );
    } catch (error) {
      if (isAnimatedImageSkip(error)) {
        markSkip(pageIndex);
        return;
      }
      if (isNeuralCancelled(error)) return;
    }
  })();
}

export async function dropNeuralQueue(options?: { cancelRunning?: boolean }): Promise<void> {
  dropping = true;
  pumpEpoch += 1;
  try {
    if (options?.cancelRunning !== false) {
      await cancelNeuralUpscale(runningJobId || 0);
    }
    const jobs = [...pending.values()];
    pending.clear();
    for (const job of jobs) {
      for (const waiter of job.waiters) waiter.reject(new Error("cancelled"));
    }
  } finally {
    dropping = false;
    runningKey = null;
    runningJobId = 0;
    runningPageIndex = -1;
    neuralRunningIndex.value = null;
    bumpMarks();
  }
}

export function setNeuralFocus(
  index: number,
  count: number,
  options?: { readingMode?: ReadingMode; isolateCover?: boolean },
) {
  focusIndex = Math.max(0, index);
  pageTotal = Math.max(0, count);
  if (options?.readingMode != null) focusMode = options.readingMode;
  if (options?.isolateCover != null) focusIsolateCover = !!options.isolateCover;
  resizeMarks(pageTotal);
  restampPriorities();
  if (neuralBatchPaused.value && runningPriority >= NEURAL_PRIORITY_BATCH && runningJobId) {
    void cancelNeuralUpscale(runningJobId);
  }
  void pumpQueue();
}

export async function applyNeuralExists(indexes: number[], exists: boolean[]) {
  for (let i = 0; i < indexes.length; i += 1) {
    if (exists[i]) markDone(indexes[i]!);
  }
}

export function applyNeuralSkip(index: number) {
  markSkip(index);
}

export function applyNeuralDone(index: number) {
  markDone(index);
}

export function resetNeuralBookMarks(count: number) {
  resetMarks(count);
}

export async function probeNeuralDisk(keys: { index: number; key: string }[]): Promise<number> {
  const valid = keys.filter((item) => item.key);
  if (!valid.length) return 0;
  const flags = await neuralResultsExists(valid.map((item) => item.key));
  applyNeuralExists(
    valid.map((item) => item.index),
    flags,
  );
  return flags.reduce((sum, hit) => sum + (hit ? 1 : 0), 0);
}

function sanitizePathComponent(raw: string): string {
  const cleaned = raw.replace(/[<>:"|?*\\/]/g, "_").replace(/[\u0000-\u001f]/g, "_");
  const trimmed = cleaned.replace(/[. ]+$/g, "");
  if (!trimmed || trimmed === "." || trimmed === "..") return "_";
  return trimmed;
}

export function bookFolderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const base = trimmed.split(/[\\/]/).pop() || "book";
  return sanitizePathComponent(base);
}

function pageWebpRel(relPath: string): string {
  const parts = relPath.replace(/\\/g, "/").split("/").filter(Boolean);
  if (!parts.length) return "page.webp";
  const last = parts.pop() ?? "page";
  const stem = last.replace(/\.[^.]+$/, "") || last;
  return [...parts.map(sanitizePathComponent), `${sanitizePathComponent(stem)}.webp`].join("/");
}

/** Relative path under the upscale image cache: `{model}/{size}/sx/{book}/{page}.webp`. */
export function neuralDiskKey(
  assetId: string,
  targetSize: number,
  bookName: string,
  relPath: string,
): string {
  if (!assetId || !relPath) return "";
  const size = clampNeuralTargetSize(targetSize);
  // `sx` = model-native scale output (no Lanczos shrink). Busts older downscaled caches.
  return `${sanitizePathComponent(assetId)}/${size}/sx/${sanitizePathComponent(bookName)}/${pageWebpRel(relPath)}`;
}

export function enqueueNeuralPage(options: {
  key: string;
  assetId: string;
  pageIndex: number;
  width: number;
  height: number;
  priority: number;
  src: string;
  targetSize: number;
}): void {
  const { key, assetId, pageIndex, width, height, priority, src, targetSize } = options;
  if (!key || !src) return;
  if (getCached(key) || pending.has(key) || doneFlags.value[pageIndex]) {
    if (getCached(key)) markDone(pageIndex);
    const job = pending.get(key);
    if (job) job.priority = Math.min(job.priority, priority);
    return;
  }
  void enqueue(
    key,
    assetId,
    width,
    height,
    priority,
    async () => {
      const loaded = await srcToBitmapAndPng(src);
      const out = resolveNeuralOutputSize(loaded.width, loaded.height, targetSize);
      if (!out) {
        markSkip(pageIndex);
        throw new Error(ANIMATED_IMAGE_SKIP);
      }
      const job = pending.get(key);
      if (job) {
        job.width = out.width;
        job.height = out.height;
      }
      return loaded.png;
    },
    pageIndex,
    true,
  ).catch(ignoreIdleQueueError);
}

export { priorityForIndex, batchRank };
