//! Unified application data root (settings + caches).
//!
//! Default (release): `<exe_dir>/SuperManGaData` when writable.
//! Fallback / debug: `<os_app_data_dir>/SuperManGaData`.
//!
//! Layout under the root:
//! - `reader.json`    settings / window state (plugin-store)
//! - `secrets/`       translation API key file fallback
//! - `comic-cache/`   archive extract / PDF render pages
//! - `data/`          versioned runtime / detector / OCR / neural packs + manifest.json
//! - `upscaled/`      neural upscale results
//! - `ocr-cache/`     per-page OCR result JSON
//! - `pdfium/`        pdfium native libraries
//!
//! Custom path locator stays in the fixed OS app data dir (`data-root.json`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const DATA_DIR_NAME: &str = "SuperManGaData";
pub const DIR_COMIC: &str = "comic-cache";
pub const DIR_DATA: &str = "data";
pub const DIR_UPSCALED: &str = "upscaled";
pub const DIR_OCR_CACHE: &str = "ocr-cache";
pub const DIR_PDFIUM: &str = "pdfium";
pub const STORE_FILE: &str = "reader.json";
pub const SECRETS_DIR: &str = "secrets";

const ALL_SUBDIRS: &[&str] = &[
    DIR_COMIC,
    DIR_DATA,
    DIR_UPSCALED,
    DIR_OCR_CACHE,
    DIR_PDFIUM,
];

const CONFIG_NAMES: &[&str] = &[STORE_FILE, SECRETS_DIR];
const LOCATOR_NAME: &str = "data-root.json";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataRootState {
    /// Explicit custom data root. Empty/None → resolve default each launch.
    #[serde(default)]
    pub data_dir: Option<String>,
}

fn os_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|err| err.to_string())
}

fn locator_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = os_app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir.join(LOCATOR_NAME))
}

fn load_state(app: &AppHandle) -> DataRootState {
    let Ok(path) = locator_path(app) else {
        return DataRootState::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return DataRootState::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_state(app: &AppHandle, state: &DataRootState) -> Result<(), String> {
    let path = locator_path(app)?;
    let raw = serde_json::to_string_pretty(state).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

fn custom_dir_raw(state: &DataRootState) -> Option<&str> {
    state
        .data_dir
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
}

fn dir_is_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".jmr-write-test");
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn exe_parent_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|parent| parent.to_path_buf()))
}

/// Default data root for this launch (never persisted as a custom path).
pub fn default_root(app: &AppHandle) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        let dir = os_app_data_dir(app)?.join(DATA_DIR_NAME);
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        return Ok(dir);
    }

    if let Some(parent) = exe_parent_dir() {
        let candidate = parent.join(DATA_DIR_NAME);
        if dir_is_writable(&candidate) {
            return Ok(candidate);
        }
        crate::app_log::warn(
            "data",
            format!(
                "exe-adjacent data dir not writable, falling back to OS app data: {}",
                candidate.display()
            ),
        );
    }

    let dir = os_app_data_dir(app)?.join(DATA_DIR_NAME);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn root_from_state(app: &AppHandle, state: &DataRootState) -> Result<PathBuf, String> {
    if let Some(custom) = custom_dir_raw(state) {
        let path = PathBuf::from(custom);
        fs::create_dir_all(&path).map_err(|err| err.to_string())?;
        return Ok(path);
    }
    default_root(app)
}

pub fn resolve_root(app: &AppHandle) -> Result<PathBuf, String> {
    let state = load_state(app);
    root_from_state(app, &state)
}

/// Absolute path to `reader.json` inside the data root.
pub fn reader_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_root(app)?.join(STORE_FILE))
}

/// `{dataRoot}/{name}`, creating the directory.
pub fn subdir(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let path = resolve_root(app)?.join(name);
    fs::create_dir_all(&path).map_err(|err| err.to_string())?;
    Ok(path)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let ty = entry.file_type().map_err(|err| err.to_string())?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else if ty.is_file() {
            if to.exists() {
                continue;
            }
            fs::copy(entry.path(), &to).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn dir_nonempty(path: &Path) -> bool {
    fs::read_dir(path)
        .ok()
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

fn canonical_or(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    canonical_or(a) == canonical_or(b)
}

fn is_under(path: &Path, ancestor: &Path) -> bool {
    let path = canonical_or(path);
    let ancestor = canonical_or(ancestor);
    path.starts_with(&ancestor)
}

fn move_into(src: &Path, dst: &Path) {
    if !src.is_dir() || same_path(src, dst) {
        return;
    }
    if is_under(dst, src) {
        let _ = copy_dir_recursive(src, dst);
        return;
    }
    if let Err(err) = copy_dir_recursive(src, dst) {
        crate::app_log::warn(
            "data",
            format!(
                "migrate copy failed src={} dst={} err={err}",
                src.display(),
                dst.display()
            ),
        );
        return;
    }
    if let Err(err) = fs::remove_dir_all(src) {
        crate::app_log::warn(
            "data",
            format!("migrate remove failed src={} err={err}", src.display()),
        );
    }
}

fn move_file(src: &Path, dst: &Path) {
    if !src.is_file() || same_path(src, dst) {
        return;
    }
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if dst.exists() {
        let _ = fs::remove_file(src);
        return;
    }
    if fs::rename(src, dst).is_err() {
        if fs::copy(src, dst).is_ok() {
            let _ = fs::remove_file(src);
        }
    }
}

fn remove_if_outside_root(path: &Path, root: &Path) {
    if !path.exists() || same_path(path, root) || is_under(path, root) {
        return;
    }
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn remember_custom(app: &AppHandle, path: Option<PathBuf>) -> Result<PathBuf, String> {
    let mut state = load_state(app);
    match path {
        None => {
            state.data_dir = None;
            save_state(app, &state)?;
            default_root(app)
        }
        Some(dest) => {
            fs::create_dir_all(&dest).map_err(|err| format!("invalid data dir: {err}"))?;
            if !dest.is_dir() {
                return Err("data path is not a directory".into());
            }
            let default = default_root(app)?;
            if same_path(&dest, &default) {
                state.data_dir = None;
                save_state(app, &state)?;
                return Ok(default);
            }
            state.data_dir = Some(dest.to_string_lossy().into_owned());
            save_state(app, &state)?;
            Ok(dest)
        }
    }
}

#[tauri::command]
pub fn get_cache_dir(app: AppHandle) -> Result<String, String> {
    Ok(resolve_root(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_default_data_dir(app: AppHandle) -> Result<String, String> {
    Ok(default_root(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_reader_store_path(app: AppHandle) -> Result<String, String> {
    Ok(reader_store_path(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn set_cache_dir(app: AppHandle, path: String) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(remember_custom(&app, None)?.to_string_lossy().into_owned());
    }
    Ok(remember_custom(&app, Some(PathBuf::from(trimmed)))?.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn migrate_cache_dir(app: AppHandle, from: String, to: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || migrate_cache_dir_sync(app, from, to))
        .await
        .map_err(|err| err.to_string())?
}

fn migrate_cache_dir_sync(app: AppHandle, from: String, to: String) -> Result<String, String> {
    let from_path = PathBuf::from(from.trim());
    let to_raw = to.trim();
    if to_raw.is_empty() {
        return Err("destination path is empty".into());
    }
    let to_path = PathBuf::from(to_raw);
    fs::create_dir_all(&to_path).map_err(|err| err.to_string())?;

    if from_path.is_dir() && !same_path(&from_path, &to_path) {
        for name in ALL_SUBDIRS {
            let src = from_path.join(name);
            let dst = to_path.join(name);
            if dir_nonempty(&src) {
                move_into(&src, &dst);
            } else {
                let _ = fs::create_dir_all(&dst);
                remove_if_outside_root(&src, &to_path);
            }
        }
        for name in CONFIG_NAMES {
            let src = from_path.join(name);
            let dst = to_path.join(name);
            if src.is_dir() {
                move_into(&src, &dst);
            } else if src.is_file() {
                move_file(&src, &dst);
            }
        }
        if !dir_nonempty(&from_path) {
            let _ = fs::remove_dir(&from_path);
        }
    }

    let resolved = remember_custom(&app, Some(to_path))?;
    Ok(resolved.to_string_lossy().into_owned())
}
