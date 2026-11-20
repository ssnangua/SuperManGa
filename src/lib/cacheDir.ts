import { invoke } from "@tauri-apps/api/core";

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Resolved absolute data root for this launch. */
export async function getCacheDir(): Promise<string> {
  if (!inTauri()) return "";
  return invoke<string>("get_cache_dir");
}

/** Default data root for this launch (exe-adjacent or OS fallback). Not a custom pin. */
export async function getDefaultDataDir(): Promise<string> {
  if (!inTauri()) return "";
  return invoke<string>("get_default_data_dir");
}

/** Absolute path to reader.json inside the data root. */
export async function getReaderStorePath(): Promise<string> {
  if (!inTauri()) return "reader.json";
  return invoke<string>("get_reader_store_path");
}

/** Empty path clears custom and returns to default. */
export async function setCacheDir(path: string): Promise<string> {
  if (!inTauri()) return path;
  return invoke<string>("set_cache_dir", { path });
}

export async function migrateCacheDir(from: string, to: string): Promise<string> {
  if (!inTauri()) return to;
  return invoke<string>("migrate_cache_dir", { from, to });
}
