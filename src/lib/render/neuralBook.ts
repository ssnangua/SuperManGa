import {
  applyNeuralSkip,
  bookFolderName,
  dropNeuralQueue,
  enqueueNeuralPage,
  neuralBatchPaused,
  neuralDiskKey,
  NEURAL_PRIORITY_BATCH,
  priorityForIndex,
  probeNeuralDisk,
  resetNeuralBookMarks,
  resolveNeuralOutputSize,
  setNeuralFocus,
} from "./neural";
import { isNeuralProfile, neuralAssetId, type RenderProfile } from "./profile";
import type { ReadingMode } from "../spread";

export type NeuralBookPage = {
  width: number;
  height: number;
  relPath?: string;
  name?: string;
};

export type NeuralBookContext = {
  enabled: boolean;
  inTauri: boolean;
  sessionId: number;
  path: string;
  readSubfolder: boolean;
  pages: NeuralBookPage[];
  focusIndex: number;
  readingMode: ReadingMode;
  isolateCover: boolean;
  renderProfile: RenderProfile;
  targetSize: number;
  imageSrc: (index: number) => string;
};

let boundSession = -1;
let boundAsset = "";
let boundTarget = 0;
let pageRels: string[] = [];
let syncing = false;
let queued: NeuralBookContext | null = null;

export async function prepareNeuralIdentity(input: {
  sessionId: number;
  path: string;
  pageCount: number;
  pages?: NeuralBookPage[];
}): Promise<void> {
  boundSession = input.sessionId;
  if (input.pages?.length) {
    pageRels = input.pages.map((page, i) => page.relPath || page.name || String(i));
  } else if (pageRels.length !== input.pageCount) {
    pageRels = Array.from({ length: input.pageCount }, (_, i) => pageRels[i] || String(i));
  }
}

export async function syncNeuralBook(ctx: NeuralBookContext): Promise<void> {
  queued = ctx;
  if (syncing) return;
  syncing = true;
  try {
    while (queued) {
      ctx = queued;
      queued = null;
      await runNeuralBook(ctx);
    }
  } finally {
    syncing = false;
  }
}

async function runNeuralBook(ctx: NeuralBookContext): Promise<void> {
  const assetId = isNeuralProfile(ctx.renderProfile) ? neuralAssetId(ctx.renderProfile) : "";
  const neural = ctx.enabled && !!assetId;
  const count = ctx.pages.length;
  if (!ctx.inTauri || count < 1) {
    boundSession = -1;
    boundAsset = "";
    boundTarget = 0;
    pageRels = [];
    await dropNeuralQueue({ cancelRunning: true });
    resetNeuralBookMarks(0);
    return;
  }
  if (!neural) {
    // Leaving neural: cancel work, but keep disk marks so the seek bar can restore on return.
    await dropNeuralQueue({ cancelRunning: true });
    return;
  }

  const assetChanged = !!boundAsset && boundAsset !== assetId;
  const targetChanged = !!boundTarget && boundTarget !== ctx.targetSize;
  if (assetChanged || targetChanged || boundSession !== ctx.sessionId) {
    await dropNeuralQueue({ cancelRunning: true });
    resetNeuralBookMarks(count);
  }
  boundAsset = assetId;
  boundTarget = ctx.targetSize;
  await prepareNeuralIdentity({
    sessionId: ctx.sessionId,
    path: ctx.path,
    pageCount: count,
    pages: ctx.pages,
  });
  const focusOpts = {
    readingMode: ctx.readingMode,
    isolateCover: ctx.isolateCover,
  };
  setNeuralFocus(ctx.focusIndex, count, focusOpts);

  const book = bookFolderName(ctx.path);
  const probeKeys: { index: number; key: string }[] = [];
  const keys: { index: number; key: string; width: number; height: number }[] = [];
  for (let i = 0; i < count; i += 1) {
    const page = ctx.pages[i]!;
    const rel = page.relPath || page.name || String(i);
    const key = neuralDiskKey(assetId, ctx.targetSize, book, rel);
    if (key) probeKeys.push({ index: i, key });
    const out = resolveNeuralOutputSize(page.width, page.height, ctx.targetSize);
    if (page.width > 0 && page.height > 0 && !out) {
      console.info(
        `[neural] skip page=${i + 1} ${page.width}x${page.height} already >= target=${ctx.targetSize}`,
      );
      applyNeuralSkip(i);
      continue;
    }
    if (!key) continue;
    keys.push({
      index: i,
      key,
      width: out?.width ?? 0,
      height: out?.height ?? 0,
    });
  }
  try {
    const diskHits = await probeNeuralDisk(probeKeys);
    console.info(
      `[neural] book=${book} asset=${assetId} target=${ctx.targetSize} pages=${count} disk-hits=${diskHits}/${probeKeys.length}`,
    );
  } catch (error) {
    console.error("neural disk probe failed", error);
  }

  const ranked = keys.slice().sort((a, b) => {
    const pa = priorityForIndex(a.index, ctx.focusIndex, count);
    const pb = priorityForIndex(b.index, ctx.focusIndex, count);
    if (pa !== pb) return pa - pb;
    return a.index - b.index;
  });
  for (const item of ranked) {
    const priority = priorityForIndex(item.index, ctx.focusIndex, count);
    // When book pre-upscale is off, still queue the current / next / prev spreads.
    if (neuralBatchPaused.value && priority >= NEURAL_PRIORITY_BATCH) continue;
    enqueueNeuralPage({
      key: item.key,
      assetId,
      pageIndex: item.index,
      width: item.width,
      height: item.height,
      priority,
      src: ctx.imageSrc(item.index),
      targetSize: ctx.targetSize,
    });
  }
  setNeuralFocus(ctx.focusIndex, count, focusOpts);
}
