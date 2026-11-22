import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { runAssetDownload } from "../assetDownload";

function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export interface ImageAssetInfo {
  id: string;
  kind: "neural" | "runtime";
  installed: boolean;
  outdated: boolean;
  bytes: number;
  /** Zip size from data manifest (preferred for install/update UI). */
  approxBytes?: number;
  path: string | null;
  version?: string | null;
}

export interface ImageAssetCatalogItem {
  id: string;
  kind: "neural" | "runtime";
  packStem: string;
  nameKey: "assetMangaJanai" | "assetRealEsrgan";
  descriptionKey: "assetMangaJanaiHint" | "assetRealEsrganHint";
  approxBytes: number;
  scale: number;
}

export interface DownloadNetworkConfig {
  proxyEnabled: boolean;
  proxyType: "http" | "socks5" | "socks4";
  proxyHost: string;
  proxyPort: number;
  proxyUser: string;
  proxyPass: string;
}

export interface NeuralRuntimeStatus {
  ready: boolean;
  outdated: boolean;
  version: string | null;
  libraryPath: string | null;
  bytes: number;
  approxBytes: number;
  flavor: string;
  recommendedFlavor: string;
  /** Active EP when ready: `none` | `cuda` | `coreml`. */
  accelerator: string;
  /** Preferred EP for this machine: `none` | `cuda` | `coreml`. */
  recommendedAccelerator: string;
  /** Windows/Linux x64 can use CUDA ORT packs. */
  cudaCapable: boolean;
  gpu: boolean;
}

export interface NeuralProgressEvent {
  kind: string;
  id: string;
  done: number;
  total: number | null;
  percent: number | null;
}

export const IMAGE_ASSET_CATALOG: ImageAssetCatalogItem[] = [
  {
    id: "manga-janai-2x",
    kind: "neural",
    packStem: "neural-manga-janai-2x",
    nameKey: "assetMangaJanai",
    descriptionKey: "assetMangaJanaiHint",
    approxBytes: 80_000_000,
    scale: 2,
  },
  {
    id: "real-esrgan-anime-2x",
    kind: "neural",
    packStem: "neural-real-esrgan-anime-2x",
    nameKey: "assetRealEsrgan",
    descriptionKey: "assetRealEsrganHint",
    approxBytes: 17_900_000,
    scale: 4,
  },
];

export function networkPayload(cfg: DownloadNetworkConfig) {
  return {
    proxyEnabled: cfg.proxyEnabled,
    proxyType: cfg.proxyType,
    proxyHost: cfg.proxyHost.trim() || null,
    proxyPort: cfg.proxyPort || null,
    proxyUser: cfg.proxyUser.trim() || null,
    proxyPass: cfg.proxyPass || null,
  };
}

export async function listImageAssets(): Promise<ImageAssetInfo[]> {
  if (!inTauri()) {
    return IMAGE_ASSET_CATALOG.map((item) => ({
      id: item.id,
      kind: item.kind,
      installed: localStorage.getItem(`asset:${item.id}`) === "1",
      outdated: false,
      bytes: item.approxBytes,
      path: null,
      version: null,
    }));
  }
  return invoke<ImageAssetInfo[]>("list_image_assets", {
    catalog: IMAGE_ASSET_CATALOG.map((item) => ({
      id: item.id,
      kind: item.kind,
      packStem: item.packStem,
      scale: item.scale,
    })),
  });
}

export async function installImageAsset(
  id: string,
  network: DownloadNetworkConfig,
): Promise<void> {
  const item = IMAGE_ASSET_CATALOG.find((entry) => entry.id === id);
  if (!item) throw new Error("unknown asset");
  return runAssetDownload("model", id, async () => {
    if (!inTauri()) {
      localStorage.setItem(`asset:${id}`, "1");
      return;
    }
    await invoke("install_image_asset", {
      id: item.id,
      packStem: item.packStem,
      network: networkPayload(network),
    });
  });
}

export async function neuralRuntimeStatus(): Promise<NeuralRuntimeStatus> {
  if (!inTauri()) {
    return {
      ready: false,
      outdated: false,
      version: null,
      libraryPath: null,
      bytes: 0,
      approxBytes: 80_000_000,
      flavor: "cpu",
      recommendedFlavor: "cpu",
      accelerator: "none",
      recommendedAccelerator: "none",
      cudaCapable: false,
      gpu: false,
    };
  }
  return invoke<NeuralRuntimeStatus>("neural_runtime_status");
}

export async function initNeuralRuntime(
  network: DownloadNetworkConfig,
): Promise<NeuralRuntimeStatus> {
  return runAssetDownload("runtime", "ort", async () => {
    if (!inTauri()) {
      return neuralRuntimeStatus();
    }
    return invoke<NeuralRuntimeStatus>("init_neural_runtime", {
      network: networkPayload(network),
    });
  });
}

export type FetchDataManifestResult = {
  updated: boolean;
};

export async function fetchDataManifest(
  network: DownloadNetworkConfig,
): Promise<FetchDataManifestResult | null> {
  if (!inTauri()) return null;
  return invoke<FetchDataManifestResult>("fetch_data_manifest", {
    network: networkPayload(network),
  });
}

export async function installedNeuralIds(): Promise<string[]> {
  const list = await listImageAssets();
  return list.filter((item) => item.installed && item.kind === "neural").map((item) => item.id);
}

export async function testDownloadProxy(
  network: DownloadNetworkConfig,
  testUrl: string,
): Promise<string> {
  if (!inTauri()) return "HTTP 200";
  return invoke<string>("test_download_proxy", {
    cfg: networkPayload(network),
    testUrl,
  });
}

export type NeuralUpscaleResult = { webp: number[] | Uint8Array };

export async function neuralUpscale(
  assetId: string,
  imagePng: number[] | Uint8Array,
  targetWidth: number,
  targetHeight: number,
  opts?: { cacheKey?: string; jobId?: number },
): Promise<NeuralUpscaleResult> {
  return invoke<NeuralUpscaleResult>("neural_upscale", {
    assetId,
    imagePng: Array.from(imagePng as Uint8Array),
    targetWidth,
    targetHeight,
    cacheKey: opts?.cacheKey ?? null,
    jobId: opts?.jobId ?? null,
  });
}

export async function cancelNeuralUpscale(jobId: number): Promise<void> {
  await invoke("cancel_neural_upscale", { jobId });
}

export async function neuralResultLoad(
  assetId: string,
  cacheKey: string,
): Promise<{ webp: number[] | Uint8Array } | null> {
  return invoke("neural_result_load", { assetId, cacheKey });
}

export async function neuralResultsExists(keys: string[]): Promise<boolean[]> {
  return invoke<boolean[]>("neural_results_exists", { keys });
}

export async function neuralResultsStatus(): Promise<{ bytes: number }> {
  if (!inTauri()) return { bytes: 0 };
  return invoke<{ bytes: number }>("neural_results_status");
}

export async function clearNeuralResults(): Promise<void> {
  if (!inTauri()) return;
  await invoke("clear_neural_results");
}

export async function listenNeuralProgress(
  handler: (event: NeuralProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<NeuralProgressEvent>("asset-progress", (event) => {
    if (
      event.payload.kind === "runtime" ||
      event.payload.kind === "model" ||
      event.payload.kind === "data"
    ) {
      handler(event.payload);
    }
  });
}
