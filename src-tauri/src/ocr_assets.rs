use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::data_packs;
use crate::neural_sr::download::NetworkConfig;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFileRef {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub kind: String,
    /// Pack stem, e.g. `detector-dbnet` / `ocr-manga-ocr`.
    pub pack_stem: String,
    #[serde(default)]
    pub files: Vec<CatalogFileRef>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrAssetInfo {
    pub id: String,
    pub kind: String,
    pub installed: bool,
    /// Current manifest version pack not ready, but an older stem folder exists.
    pub outdated: bool,
    /// Installed on-disk size (0 if not installed).
    pub bytes: u64,
    /// Zip size from data manifest (for install/update UI).
    pub approx_bytes: u64,
    pub path: Option<String>,
    pub version: Option<String>,
}

fn required_names(files: &[CatalogFileRef]) -> Vec<&str> {
    files.iter().map(|f| f.name.as_str()).collect()
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


/// True when the OCR/detector pack for the current manifest version is installed.
pub fn is_pack_installed(app: &AppHandle, stem: &str) -> bool {
    data_packs::is_stem_installed(app, stem, &[])
}

pub fn pack_file(app: &AppHandle, stem: &str, file_name: &str) -> Result<PathBuf, String> {
    data_packs::pack_file(app, stem, file_name)
}

#[tauri::command]
pub fn get_ocr_cache_dir(app: AppHandle) -> Result<String, String> {
    Ok(data_packs::data_root(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn list_ocr_assets(app: AppHandle, catalog: Vec<CatalogItem>) -> Result<Vec<OcrAssetInfo>, String> {
    let manifest = data_packs::load_manifest(&app)?;
    let mut out = Vec::with_capacity(catalog.len());
    for item in catalog {
        let meta = data_packs::parse_stem(&item.pack_stem)
            .and_then(|(section, key)| manifest.pack(section, key));
        let version = meta.map(|m| m.version.clone());
        let approx_bytes = meta.map(|m| m.bytes).unwrap_or(0);
        let required = required_names(&item.files);
        let folder = data_packs::resolve_pack_dir(&app, &item.pack_stem).ok();
        let installed = folder
            .as_ref()
            .is_some_and(|f| data_packs::pack_files_ready(f, &required));
        let outdated = !installed && stem_has_any_version(&app, &item.pack_stem);
        let bytes = if installed {
            folder.as_ref().map(|f| data_packs::dir_bytes(f)).unwrap_or(0)
        } else {
            0
        };
        out.push(OcrAssetInfo {
            id: item.id,
            kind: item.kind,
            installed,
            outdated,
            bytes,
            approx_bytes,
            path: installed
                .then(|| folder.map(|f| f.to_string_lossy().into_owned()))
                .flatten(),
            version,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn install_ocr_asset(
    app: AppHandle,
    id: String,
    pack_stem: String,
    network: NetworkConfig,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        data_packs::install_pack(&app, "ocr", &id, &pack_stem, &network)?;
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn remove_ocr_asset(app: AppHandle, id: String, pack_stem: String) -> Result<(), String> {
    let _ = id;
    data_packs::remove_stem_versions(&app, &pack_stem)
}
