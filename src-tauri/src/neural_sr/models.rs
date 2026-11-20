use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::download::NetworkConfig;
use super::runtime::library_path;
use crate::data_packs;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub kind: String,
    /// Pack stem e.g. `neural-manga-janai-2x`.
    pub pack_stem: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAssetInfo {
    pub id: String,
    pub kind: String,
    pub installed: bool,
    pub outdated: bool,
    pub bytes: u64,
    pub approx_bytes: u64,
    pub path: Option<String>,
    pub version: Option<String>,
}

fn find_onnx(dir: &std::path::Path) -> Option<PathBuf> {
    let mut best: Option<(i32, PathBuf)> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !name.ends_with(".onnx") {
                continue;
            }
            let mut score = 0;
            if name.contains("2x") || name.contains("_x2") {
                score += 300;
            } else if name.contains("4x") || name.contains("_x4") {
                score += 50;
            }
            if name.contains("dynamo") {
                score -= 80;
            }
            if name.contains("bf16") {
                score -= 60;
            }
            if name.contains("fp16") && !name.contains("bf16") {
                score += 40;
            }
            if best.as_ref().is_none_or(|(best_score, _)| score > *best_score) {
                best = Some((score, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

fn stem_has_any_version(app: &AppHandle, stem: &str) -> bool {
    let Ok(root) = data_packs::data_root(app) else {
        return false;
    };
    let prefix = format!("{stem}-v");
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(&prefix) && e.path().is_dir())
    })
}

pub fn model_onnx_path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    let stem = data_packs::pack_stem("neural", id);
    let folder = data_packs::resolve_pack_dir(app, &stem).ok()?;
    find_onnx(&folder)
}

pub fn model_scale(app: &AppHandle, id: &str) -> f32 {
    let Some(onnx) = model_onnx_path(app, id) else {
        return if id.contains("4x") { 4.0 } else { 2.0 };
    };
    let name = onnx
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("2x") || name.contains("_x2") {
        2.0
    } else if name.contains("4x") || name.contains("_x4") {
        4.0
    } else if id.contains("4") {
        4.0
    } else {
        2.0
    }
}

#[tauri::command]
pub fn list_image_assets(app: AppHandle, catalog: Vec<CatalogItem>) -> Result<Vec<ImageAssetInfo>, String> {
    let manifest = data_packs::load_manifest(&app)?;
    let mut out = Vec::with_capacity(catalog.len());
    for item in catalog {
        let stem = if item.pack_stem.trim().is_empty() {
            data_packs::pack_stem("neural", &item.id)
        } else {
            item.pack_stem.clone()
        };
        let meta = data_packs::parse_stem(&stem)
            .and_then(|(section, key)| manifest.pack(section, key));
        let version = meta.map(|m| m.version.clone());
        let approx_bytes = meta.map(|m| m.bytes).unwrap_or(0);
        let folder = data_packs::resolve_pack_dir(&app, &stem).ok();
        let onnx = folder.as_ref().and_then(|f| find_onnx(f));
        let installed = onnx.as_ref().is_some_and(|p| p.is_file());
        let outdated = !installed && stem_has_any_version(&app, &stem);
        let bytes = if installed {
            folder.as_ref().map(|f| data_packs::dir_bytes(f)).unwrap_or(0)
        } else {
            0
        };
        out.push(ImageAssetInfo {
            id: item.id,
            kind: item.kind,
            installed,
            outdated,
            bytes,
            approx_bytes,
            path: onnx.map(|p| p.to_string_lossy().into_owned()),
            version,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn install_image_asset(
    app: AppHandle,
    id: String,
    pack_stem: String,
    network: NetworkConfig,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if library_path(&app).is_none() {
            return Err("neural runtime is not ready; initialize it first".into());
        }
        let stem = if pack_stem.trim().is_empty() {
            data_packs::pack_stem("neural", &id)
        } else {
            pack_stem
        };
        let dest = data_packs::install_pack(&app, "model", &id, &stem, &network)?;
        if find_onnx(&dest).is_none() {
            return Err("no .onnx found in neural pack".into());
        }
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn remove_image_asset(app: AppHandle, id: String, pack_stem: String) -> Result<(), String> {
    let stem = if pack_stem.trim().is_empty() {
        data_packs::pack_stem("neural", &id)
    } else {
        pack_stem
    };
    data_packs::remove_stem_versions(&app, &stem)
}
