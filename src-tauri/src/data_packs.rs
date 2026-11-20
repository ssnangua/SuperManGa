//! Unified data packs under `SuperManGaData/data/`.
//!
//! Remote: GitHub Release tag `data` — `{stem}-v{version}.zip` + `manifest.json`.
//! Local: `data/{stem}-v{version}/` (same stem as the zip).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use std::time::Duration;

use crate::app_cache;
use crate::neural_sr::download::{
    build_agent_with_timeout, download_to_file_with_options, NetworkConfig,
};

pub const RELEASE_BASE: &str =
    "https://github.com/ssnangua/SuperManGa/releases/download/data";
pub const MANIFEST_FILE: &str = "manifest.json";
const SEED_MANIFEST: &str = include_str!("../resources/data-manifest.json");
/// Soft-refresh must fail fast offline (same idea as app update check).
const MANIFEST_REFRESH_TIMEOUT: Duration = Duration::from_secs(8);

/// One pack entry in the remote/local data manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMeta {
    pub version: String,
    /// Compressed zip size in bytes (for UI). `0` = unknown / skip size check.
    #[serde(default)]
    pub bytes: u64,
    /// Hex sha256 of the zip archive. Empty = skip checksum.
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataManifest {
    #[serde(default = "default_schema")]
    pub schema: u32,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub runtime: BTreeMap<String, PackMeta>,
    #[serde(default)]
    pub detector: BTreeMap<String, PackMeta>,
    #[serde(default)]
    pub ocr: BTreeMap<String, PackMeta>,
    #[serde(default)]
    pub neural: BTreeMap<String, PackMeta>,
}

fn default_schema() -> u32 {
    2
}

impl DataManifest {
    fn section_map(&self, section: &str) -> Option<&BTreeMap<String, PackMeta>> {
        match section {
            "runtime" => Some(&self.runtime),
            "detector" => Some(&self.detector),
            "ocr" => Some(&self.ocr),
            "neural" => Some(&self.neural),
            _ => None,
        }
    }

    pub fn pack(&self, section: &str, key: &str) -> Option<&PackMeta> {
        self.section_map(section)?.get(key)
    }

    pub fn version(&self, section: &str, key: &str) -> Option<&str> {
        let v = self.section_map(section)?.get(key)?.version.trim();
        (!v.is_empty()).then_some(v)
    }
}

pub fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    app_cache::subdir(app, app_cache::DIR_DATA)
}

pub fn manifest_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_root(app)?.join(MANIFEST_FILE))
}

pub fn pack_stem(section: &str, key: &str) -> String {
    format!("{section}-{key}")
}

pub fn pack_folder_name(stem: &str, version: &str) -> String {
    format!("{stem}-v{version}")
}

pub fn pack_url(stem: &str, version: &str) -> String {
    format!("{RELEASE_BASE}/{stem}-v{version}.zip")
}

/// Parse `detector-dbnet` / `runtime-cuda12` / `neural-manga-janai-2x` → (section, key).
pub fn parse_stem(stem: &str) -> Option<(&str, &str)> {
    for section in ["runtime", "detector", "ocr", "neural"] {
        let prefix = format!("{section}-");
        if let Some(key) = stem.strip_prefix(&prefix) {
            if !key.is_empty() {
                return Some((section, key));
            }
        }
    }
    None
}

fn parse_manifest_raw(raw: &str) -> Result<DataManifest, String> {
    let trimmed = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    serde_json::from_str(trimmed).map_err(|err| format!("invalid data manifest: {err}"))
}

fn write_manifest(path: &Path, manifest: &DataManifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let raw = serde_json::to_string_pretty(manifest).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

/// Ensure local manifest exists (seed from embedded JSON). Returns current manifest.
pub fn load_manifest(app: &AppHandle) -> Result<DataManifest, String> {
    let path = manifest_path(app)?;
    if !path.is_file() {
        let seed = parse_manifest_raw(SEED_MANIFEST)?;
        write_manifest(&path, &seed)?;
        return Ok(seed);
    }
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    match parse_manifest_raw(&raw) {
        Ok(m) => Ok(m),
        Err(_) => {
            let seed = parse_manifest_raw(SEED_MANIFEST)?;
            write_manifest(&path, &seed)?;
            Ok(seed)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestRefreshResult {
    pub updated: bool,
    pub manifest: DataManifest,
}

/// Fetch remote manifest and write it only when it differs from local.
/// Failures leave the local file untouched.
/// Uses a short ureq timeout and must be called from a blocking/async worker — never the UI thread.
pub fn refresh_manifest(
    app: &AppHandle,
    network: &NetworkConfig,
) -> Result<ManifestRefreshResult, String> {
    let local = load_manifest(app)?;
    let url = format!("{RELEASE_BASE}/{MANIFEST_FILE}");
    let agent = build_agent_with_timeout(network, MANIFEST_REFRESH_TIMEOUT)?;
    let response = agent
        .get(&url)
        .set("User-Agent", "supermanga-data-manifest")
        .call()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("manifest HTTP {status}"));
    }
    let raw = response.into_string().map_err(|err| err.to_string())?;
    let parsed = parse_manifest_raw(&raw)?;
    if parsed == local {
        return Ok(ManifestRefreshResult {
            updated: false,
            manifest: local,
        });
    }
    let final_path = manifest_path(app)?;
    write_manifest(&final_path, &parsed)?;
    Ok(ManifestRefreshResult {
        updated: true,
        manifest: parsed,
    })
}

/// Async so ureq cannot block the UI thread when DNS/proxy hangs offline.
#[tauri::command]
pub async fn refresh_data_manifest(
    app: AppHandle,
    network: NetworkConfig,
) -> Result<ManifestRefreshResult, String> {
    tauri::async_runtime::spawn_blocking(move || refresh_manifest(&app, &network))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn get_data_manifest(app: AppHandle) -> Result<DataManifest, String> {
    load_manifest(&app)
}

pub fn resolve_pack_dir(app: &AppHandle, stem: &str) -> Result<PathBuf, String> {
    let (section, key) = parse_stem(stem).ok_or_else(|| format!("unknown pack stem: {stem}"))?;
    let manifest = load_manifest(app)?;
    let version = manifest
        .version(section, key)
        .ok_or_else(|| format!("no version in manifest for {stem}"))?;
    Ok(data_root(app)?.join(pack_folder_name(stem, version)))
}

pub fn dir_bytes(path: &Path) -> u64 {
    if !path.is_dir() {
        return 0;
    }
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            total += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        } else if p.is_dir() {
            total += dir_bytes(&p);
        }
    }
    total
}

/// True when the versioned folder exists and every required file is present.
pub fn pack_files_ready(folder: &Path, required: &[&str]) -> bool {
    if !folder.is_dir() {
        return false;
    }
    if required.is_empty() {
        return fs::read_dir(folder)
            .ok()
            .map(|mut it| it.next().is_some())
            .unwrap_or(false);
    }
    required.iter().all(|name| folder.join(name).is_file())
}

pub fn is_stem_installed(app: &AppHandle, stem: &str, required: &[&str]) -> bool {
    let Ok(folder) = resolve_pack_dir(app, stem) else {
        return false;
    };
    pack_files_ready(&folder, required)
}

pub fn pack_file(app: &AppHandle, stem: &str, file_name: &str) -> Result<PathBuf, String> {
    let folder = resolve_pack_dir(app, stem)?;
    let path = folder.join(file_name);
    if !path.is_file() {
        return Err(format!("pack file missing: {stem}/{file_name}"));
    }
    Ok(path)
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    let file = fs::File::open(archive).map_err(|err| err.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|err| err.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|err| err.to_string())?;
        let name = entry.name().replace('\\', "/");
        // Skip nested absolute / zip-slip
        if name.contains("..") {
            continue;
        }
        let out = dest.join(entry.mangled_name());
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|err| err.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            let mut outfile = fs::File::create(&out).map_err(|err| err.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|err| err.to_string())?;
            outfile.flush().map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

/// Remove sibling folders `stem-v*` except `keep_version`.
pub fn remove_old_versions(root: &Path, stem: &str, keep_version: &str) {
    let keep = pack_folder_name(stem, keep_version);
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let prefix = format!("{stem}-v");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name == keep || !name.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Download `{stem}-v{version}.zip` into `data/{stem}-v{version}/`.
pub fn install_pack(
    app: &AppHandle,
    progress_kind: &str,
    progress_id: &str,
    stem: &str,
    network: &NetworkConfig,
) -> Result<PathBuf, String> {
    let (section, key) = parse_stem(stem).ok_or_else(|| format!("unknown pack stem: {stem}"))?;
    let manifest = load_manifest(app)?;
    let meta = manifest
        .pack(section, key)
        .ok_or_else(|| format!("no version in manifest for {stem}"))?;
    let version = meta.version.trim().to_string();
    if version.is_empty() {
        return Err(format!("empty version in manifest for {stem}"));
    }
    let root = data_root(app)?;
    let folder_name = pack_folder_name(stem, &version);
    let dest_dir = root.join(&folder_name);
    if dest_dir.is_dir() && pack_files_ready(&dest_dir, &[]) {
        // Already present — still prune other versions.
        remove_old_versions(&root, stem, &version);
        return Ok(dest_dir);
    }

    let url = pack_url(stem, &version);
    let archive = root.join(format!("{folder_name}.zip"));
    let expected_bytes = meta.bytes;
    let expected_sha = {
        let s = meta.sha256.trim();
        (!s.is_empty()).then(|| s.to_string())
    };
    download_to_file_with_options(
        app,
        progress_kind,
        progress_id,
        &url,
        &archive,
        expected_sha.as_deref(),
        network,
        false,
    )?;

    if expected_bytes > 0 {
        let actual = fs::metadata(&archive)
            .map(|m| m.len())
            .map_err(|err| err.to_string())?;
        if actual != expected_bytes {
            let _ = fs::remove_file(&archive);
            return Err(format!(
                "size mismatch for {stem}: expected {expected_bytes}, got {actual}"
            ));
        }
    }

    let extract_tmp = root.join(format!("{folder_name}.extract"));
    if extract_tmp.exists() {
        let _ = fs::remove_dir_all(&extract_tmp);
    }
    extract_zip(&archive, &extract_tmp)?;

    // If the zip nested a single top-level folder, unwrap when it looks like the pack.
    let source = {
        let mut children: Vec<PathBuf> = fs::read_dir(&extract_tmp)
            .map_err(|err| err.to_string())?
            .flatten()
            .map(|e| e.path())
            .collect();
        if children.len() == 1 && children[0].is_dir() {
            children.remove(0)
        } else {
            extract_tmp.clone()
        }
    };

    if dest_dir.exists() {
        let _ = fs::remove_dir_all(&dest_dir);
    }
    if fs::rename(&source, &dest_dir).is_err() {
        copy_dir_recursive(&source, &dest_dir)?;
    }
    let _ = fs::remove_dir_all(&extract_tmp);
    let _ = fs::remove_file(&archive);
    remove_old_versions(&root, stem, &version);
    Ok(dest_dir)
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
            fs::copy(entry.path(), &to).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

pub fn remove_stem_versions(app: &AppHandle, stem: &str) -> Result<(), String> {
    let root = data_root(app)?;
    let prefix = format!("{stem}-v");
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}
