import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadNetworkConfig } from "../render/assets";
import { networkPayload } from "../render/assets";
import {
  normalizeOcrEngineId,
  requiredOcrAssetIdsForEngine,
  type OcrEngineId,
} from "./engines";

function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export interface OcrAssetInfo {
  id: string;
  kind: "detector" | "ocr";
  installed: boolean;
  outdated: boolean;
  bytes: number;
  /** Zip size from data manifest (preferred for install/update UI). */
  approxBytes?: number;
  path: string | null;
  version?: string | null;
}

export interface OcrAssetCatalogItem {
  id: string;
  kind: "detector" | "ocr";
  /** Pack stem under SuperManGaData/data/{stem}-v{ver}/ */
  packStem: string;
  nameKey:
    | "ocrAssetDetector"
    | "ocrAssetDetectorDefault"
    | "ocrAssetDetectorYolo"
    | "ocrAssetDetectorSaberYolo"
    | "ocrAssetMangaOcr"
    | "ocrAssetPaddle"
    | "ocrAssetMit48"
    | "ocrAssetEasyOcr";
  descriptionKey:
    | "ocrAssetDetectorHint"
    | "ocrAssetDetectorDefaultHint"
    | "ocrAssetDetectorYoloHint"
    | "ocrAssetDetectorSaberYoloHint"
    | "ocrAssetMangaOcrHint"
    | "ocrAssetPaddleHint"
    | "ocrAssetMit48Hint"
    | "ocrAssetEasyOcrHint";
  approxBytes: number;
  /** Required files inside the versioned pack folder. */
  files: readonly string[];
  engines: OcrEngineId[];
}

export const OCR_ASSET_CATALOG: OcrAssetCatalogItem[] = [
  {
    id: "detector-dbnet",
    kind: "detector",
    packStem: "detector-dbnet",
    nameKey: "ocrAssetDetectorDefault",
    descriptionKey: "ocrAssetDetectorDefaultHint",
    approxBytes: 291_000_000,
    files: ["detect-20241225.onnx"],
    engines: [],
  },
  {
    id: "detector-ctd",
    kind: "detector",
    packStem: "detector-ctd",
    nameKey: "ocrAssetDetector",
    descriptionKey: "ocrAssetDetectorHint",
    approxBytes: 90_300_000,
    files: ["comictextdetector.pt.onnx"],
    engines: [],
  },
  {
    id: "detector-yolo",
    kind: "detector",
    packStem: "detector-yolo",
    nameKey: "ocrAssetDetectorYolo",
    descriptionKey: "ocrAssetDetectorYoloHint",
    approxBytes: 36_200_000,
    files: ["ysgyolo.onnx"],
    engines: [],
  },
  {
    id: "detector-saber-yolo",
    kind: "detector",
    packStem: "detector-saber-yolo",
    nameKey: "ocrAssetDetectorSaberYolo",
    descriptionKey: "ocrAssetDetectorSaberYoloHint",
    approxBytes: 98_000_000,
    files: ["saber_yolo.onnx"],
    engines: [],
  },
  {
    id: "ocr-manga-ocr",
    kind: "ocr",
    packStem: "ocr-manga-ocr",
    nameKey: "ocrAssetMangaOcr",
    descriptionKey: "ocrAssetMangaOcrHint",
    approxBytes: 460_000_000,
    files: [
      "encoder_model.onnx",
      "decoder_model.onnx",
      "vocab.txt",
      "preprocessor_config.json",
    ],
    engines: ["manga-ocr"],
  },
  {
    id: "ocr-paddle",
    kind: "ocr",
    packStem: "ocr-paddle",
    nameKey: "ocrAssetPaddle",
    descriptionKey: "ocrAssetPaddleHint",
    approxBytes: 139_000_000,
    files: ["det.onnx", "rec.onnx", "keys.txt"],
    engines: ["paddle-ocr"],
  },
  {
    id: "ocr-mit48px",
    kind: "ocr",
    packStem: "ocr-mit48px",
    nameKey: "ocrAssetMit48",
    descriptionKey: "ocrAssetMit48Hint",
    approxBytes: 160_000_000,
    files: ["ocr-48px-ctc.onnx", "alphabet-all-v5.txt"],
    engines: ["mit-48px"],
  },
  {
    id: "ocr-easyocr",
    kind: "ocr",
    packStem: "ocr-easyocr",
    nameKey: "ocrAssetEasyOcr",
    descriptionKey: "ocrAssetEasyOcrHint",
    approxBytes: 154_000_000,
    files: [
      "craft.onnx",
      "rec_zh_sim.onnx",
      "rec_japanese.onnx",
      "rec_korean.onnx",
      "rec_latin.onnx",
    ],
    engines: ["easyocr"],
  },
];

export type OcrProgressEvent = {
  kind: string;
  id: string;
  done: number;
  total: number | null;
  percent: number | null;
};

export async function listOcrAssets(): Promise<OcrAssetInfo[]> {
  if (!inTauri()) {
    return OCR_ASSET_CATALOG.map((item) => ({
      id: item.id,
      kind: item.kind,
      installed: localStorage.getItem(`asset:${item.id}`) === "1",
      outdated: false,
      bytes: item.approxBytes,
      path: null,
      version: null,
    }));
  }
  return invoke<OcrAssetInfo[]>("list_ocr_assets", {
    catalog: OCR_ASSET_CATALOG.map((item) => ({
      id: item.id,
      kind: item.kind,
      packStem: item.packStem,
      files: item.files.map((name) => ({ name })),
    })),
  });
}

export async function installOcrAsset(id: string, network: DownloadNetworkConfig): Promise<void> {
  const item = OCR_ASSET_CATALOG.find((entry) => entry.id === id);
  if (!item) throw new Error("unknown asset");
  if (!inTauri()) {
    localStorage.setItem(`asset:${id}`, "1");
    return;
  }
  await invoke("install_ocr_asset", {
    id: item.id,
    packStem: item.packStem,
    network: networkPayload(network),
  });
}

export async function installedOcrAssetIds(): Promise<string[]> {
  const list = await listOcrAssets();
  return list.filter((item) => item.installed).map((item) => item.id);
}

export async function listenAssetProgress(
  handler: (event: OcrProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<OcrProgressEvent>("asset-progress", (event) => {
    if (event.payload.kind === "ocr" || event.payload.kind.startsWith("ocr")) {
      handler(event.payload);
    }
  });
}

export { requiredOcrAssetIdsForEngine, normalizeOcrEngineId };
export type { OcrEngineId };
