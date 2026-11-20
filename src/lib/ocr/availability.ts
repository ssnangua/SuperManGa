import { isWindowsDesktop } from "../platform";
import {
  OCR_ENGINE_IDS,
  ocrEngineMeta,
  type OcrEngineId,
} from "./engines";

export type OcrReadinessContext = {
  installedAssetIds: readonly string[];
  /** Shared ORT runtime (same as neural SR). Required by local ONNX OCR packs. */
  runtimeReady: boolean;
};

export function ocrEnginePlatformSupported(id: OcrEngineId): boolean {
  if (id === "windows") return isWindowsDesktop();
  return true;
}

/** Engines that can actually run with the current installs / runtime. */
export function isOcrEngineReady(
  id: OcrEngineId,
  ctx: OcrReadinessContext,
): boolean {
  if (!ocrEnginePlatformSupported(id)) return false;
  if (id === "windows") return true;
  const assetId = ocrEngineMeta(id).assetId;
  if (assetId && !ctx.installedAssetIds.includes(assetId)) return false;
  return ctx.runtimeReady;
}

export function hasReadyOcrEngine(ctx: OcrReadinessContext): boolean {
  return OCR_ENGINE_IDS.some((id) => isOcrEngineReady(id, ctx));
}

/** Prefer the current selection when ready; otherwise first ready engine. */
export function pickReadyOcrEngine(
  preferred: OcrEngineId,
  ctx: OcrReadinessContext,
): OcrEngineId | null {
  if (isOcrEngineReady(preferred, ctx)) return preferred;
  return OCR_ENGINE_IDS.find((id) => isOcrEngineReady(id, ctx)) ?? null;
}

