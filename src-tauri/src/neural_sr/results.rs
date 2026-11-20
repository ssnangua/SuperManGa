use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use image::RgbaImage;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::cache::resolve_results_root;
use crate::state::{lock, AppState};

pub const CANCELLED: &str = "cancelled";
const WEBP_QUALITY: f32 = 90.0;

fn sanitize_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim_end_matches([' ', '.']).to_string();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "_".into()
    } else {
        trimmed
    }
}

fn result_path(root: &Path, key: &str) -> Option<PathBuf> {
    if key.is_empty() || Path::new(key).is_absolute() {
        return None;
    }
    let mut path = root.to_path_buf();
    let mut parts = 0usize;
    for part in key.replace('\\', "/").split('/') {
        if part.is_empty() {
            continue;
        }
        let safe = sanitize_component(part);
        if safe == ".." {
            return None;
        }
        path.push(safe);
        parts += 1;
        if parts > 24 {
            return None;
        }
    }
    if parts < 4 {
        return None;
    }
    Some(path)
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            total += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        } else if p.is_dir() {
            total += dir_size(&p);
        }
    }
    total
}

fn touch(path: &Path) {
    let now = filetime_now();
    let _ = fs::File::open(path).and_then(|file| file.set_modified(now));
}

fn filetime_now() -> SystemTime {
    SystemTime::now()
}

pub fn encode_webp(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let encoder = webp::Encoder::from_rgba(img.as_raw(), img.width(), img.height());
    Ok(encoder.encode(WEBP_QUALITY).to_vec())
}

pub fn load_cached_webp(root: &Path, key: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = result_path(root, key) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|err| err.to_string())?;
    touch(&path);
    Ok(Some(bytes))
}

pub fn store_cached_webp(app: &AppHandle, key: &str, encoded: &[u8]) -> Result<(), String> {
    if key.is_empty() || encoded.is_empty() {
        return Ok(());
    }
    let root = resolve_results_root(app)?;
    let Some(path) = result_path(&root, key) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension("webp.tmp");
    fs::write(&tmp, encoded).map_err(|err| err.to_string())?;
    fs::rename(&tmp, &path).map_err(|err| {
        let _ = fs::remove_file(&tmp);
        err.to_string()
    })?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeuralResultsStatus {
    pub bytes: u64,
}

#[tauri::command]
pub fn neural_page_fingerprints(app: AppHandle, session_id: u64) -> Result<Vec<String>, String> {
    let state = app.state::<AppState>();
    let guard = lock(&state.session);
    let session = guard.as_ref().ok_or_else(|| "no comic".to_string())?;
    if session.id != session_id {
        return Err("session mismatch".into());
    }
    let mut out = Vec::with_capacity(session.pages.len());
    for (index, page) in session.pages.iter().enumerate() {
        let st = session.backend.file_stat(index, &page.rel_path);
        out.push(format!(
            "{}|{}|{}|{}",
            page.rel_path.replace('\\', "/"),
            st.bytes.unwrap_or(0),
            st.modified_ms.unwrap_or(0),
            st.modified_text.as_deref().unwrap_or("")
        ));
    }
    Ok(out)
}

#[tauri::command]
pub fn neural_results_exists(app: AppHandle, keys: Vec<String>) -> Result<Vec<bool>, String> {
    let root = resolve_results_root(&app)?;
    Ok(keys
        .into_iter()
        .map(|key| result_path(&root, &key).is_some_and(|path| path.is_file()))
        .collect())
}

#[tauri::command]
pub fn neural_results_status(app: AppHandle) -> Result<NeuralResultsStatus, String> {
    let root = resolve_results_root(&app)?;
    Ok(NeuralResultsStatus {
        bytes: dir_size(&root),
    })
}

#[tauri::command]
pub fn clear_neural_results(app: AppHandle) -> Result<(), String> {
    let dir = resolve_results_root(&app)?;
    if dir.is_dir() {
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    }
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

