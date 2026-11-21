import { isWindowsDesktop } from "../platform";
import {
  requiredDetectorAssetIds,
  type TextDetectorId,
} from "./detectors";
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

export type TranslationPipelineSelection = {
  ocrEngine: OcrEngineId;
  textDetector: TextDetectorId;
  enableAuxYolo?: boolean;
  enableSaberYoloRefine?: boolean;
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

export function isTextDetectorReady(
  primary: TextDetectorId,
  ctx: OcrReadinessContext,
  options?: { auxYolo?: boolean; saberRefine?: boolean },
): boolean {
  const required = requiredDetectorAssetIds(primary, options);
  if (!required.length) return true;
  if (!ctx.runtimeReady) return false;
  return required.every((id) => ctx.installedAssetIds.includes(id));
}

/** Selected OCR + detector (+ optional refine/aux) can run with current installs. */
export function isTranslationPipelineReady(
  selection: TranslationPipelineSelection,
  ctx: OcrReadinessContext,
): boolean {
  if (!isOcrEngineReady(selection.ocrEngine, ctx)) return false;
  return isTextDetectorReady(selection.textDetector, ctx, {
    auxYolo: !!selection.enableAuxYolo,
    saberRefine: !!selection.enableSaberYoloRefine,
  });
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

