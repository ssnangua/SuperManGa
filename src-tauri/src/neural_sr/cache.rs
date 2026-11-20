use std::path::PathBuf;

use tauri::AppHandle;

use crate::data_packs;

/// Neural model packs live under `SuperManGaData/data/neural-*-v*`.
pub fn resolve_cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    data_packs::data_root(app)
}

pub fn resolve_results_root(app: &AppHandle) -> Result<PathBuf, String> {
    crate::app_cache::subdir(app, crate::app_cache::DIR_UPSCALED)
}

/// Runtime packs also live under `data/`.
pub fn resolve_runtime_root(app: &AppHandle) -> Result<PathBuf, String> {
    data_packs::data_root(app)
}

#[tauri::command]
pub fn get_neural_results_dir(app: AppHandle) -> Result<String, String> {
    Ok(resolve_results_root(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_runtime_cache_dir(app: AppHandle) -> Result<String, String> {
    Ok(resolve_runtime_root(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_neural_cache_dir(app: AppHandle) -> Result<String, String> {
    Ok(resolve_cache_root(&app)?.to_string_lossy().into_owned())
}
