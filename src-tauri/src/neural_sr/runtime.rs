use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::cuda::{platform_supports_cuda, recommended_flavor, RuntimeFlavor};
use super::download::NetworkConfig;
use super::ort_env::{active_accelerator, recommended_accelerator};
use crate::data_packs;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeuralRuntimeStatus {
    pub ready: bool,
    /// Recommended flavor missing/outdated while something else (or older version) is present.
    pub outdated: bool,
    pub version: Option<String>,
    pub library_path: Option<String>,
    pub bytes: u64,
    pub approx_bytes: u64,
    pub flavor: String,
    pub recommended_flavor: String,
    /// Active hardware EP when ready: `none` | `cuda` | `coreml`.
    pub accelerator: String,
    /// Preferred EP for this machine (even before init): `none` | `cuda` | `coreml`.
    pub recommended_accelerator: String,
    /// This OS/arch can use CUDA ORT packs (Windows/Linux x64).
    pub cuda_capable: bool,
    pub gpu: bool,
}

fn runtime_approx_bytes(flavor: RuntimeFlavor) -> u64 {
    match flavor {
        RuntimeFlavor::Cuda12 | RuntimeFlavor::Cuda13 => 155_000_000,
        RuntimeFlavor::Cpu => 80_000_000,
    }
}

fn host_os() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "linux"
    }
}

fn host_arch() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        "unknown"
    }
}

/// Manifest / zip key: `{os}-{arch}-{flavor}` e.g. `macos-arm64-cpu`.
fn runtime_pack_key(flavor: RuntimeFlavor) -> String {
    format!("{}-{}-{}", host_os(), host_arch(), flavor.as_str())
}

fn runtime_stem(flavor: RuntimeFlavor) -> String {
    data_packs::pack_stem("runtime", &runtime_pack_key(flavor))
}

pub fn library_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libonnxruntime.dylib"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "libonnxruntime.so"
    }
}

fn flavor_library(app: &AppHandle, flavor: RuntimeFlavor) -> Option<PathBuf> {
    let stem = runtime_stem(flavor);
    let dir = data_packs::resolve_pack_dir(app, &stem).ok()?;
    let lib = dir.join(library_name());
    lib.is_file().then_some(lib)
}

fn stem_has_any_version(app: &AppHandle, stem: &str) -> bool {
    let Ok(root) = data_packs::data_root(app) else {
        return false;
    };
    let prefix = format!("{stem}-v");
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(&prefix) && e.path().is_dir())
    })
}

/// Prefer recommended flavor; fall back to CPU at the current manifest version.
fn select_runtime(app: &AppHandle) -> Option<(RuntimeFlavor, PathBuf)> {
    let want = recommended_flavor();
    if let Some(lib) = flavor_library(app, want) {
        return Some((want, lib));
    }
    if want != RuntimeFlavor::Cpu {
        if let Some(lib) = flavor_library(app, RuntimeFlavor::Cpu) {
            return Some((RuntimeFlavor::Cpu, lib));
        }
    }
    None
}

fn runtime_outdated(app: &AppHandle, selected: Option<RuntimeFlavor>) -> bool {
    let want = recommended_flavor();
    if flavor_library(app, want).is_some() {
        return false;
    }
    if let Some(flavor) = selected {
        if want.is_cuda() && flavor != want {
            return true;
        }
    }
    selected.is_some() || stem_has_any_version(app, &runtime_stem(want))
}

pub fn installed_flavor(app: &AppHandle) -> RuntimeFlavor {
    select_runtime(app)
        .map(|(flavor, _)| flavor)
        .unwrap_or(RuntimeFlavor::Cpu)
}

pub fn library_path(app: &AppHandle) -> Option<PathBuf> {
    select_runtime(app).map(|(_, lib)| lib)
}

pub fn installed_runtime_version(app: &AppHandle) -> Option<String> {
    let flavor = installed_flavor(app);
    let key = runtime_pack_key(flavor);
    let manifest = data_packs::load_manifest(app).ok()?;
    manifest
        .version("runtime", &key)
        .map(|s| s.to_string())
        .or_else(|| {
            // Fall back: parse from folder name if selected
            let lib = library_path(app)?;
            let folder = lib.parent()?.file_name()?.to_str()?;
            folder
                .rsplit_once("-v")
                .map(|(_, ver)| ver.to_string())
        })
}

fn find_library(dir: &Path) -> Option<PathBuf> {
    let want = library_name();
    if dir.join(want).is_file() {
        return Some(dir.join(want));
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = fs::read_dir(&cur) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|s| s.to_str()) == Some(want) {
                return Some(path);
            }
        }
    }
    None
}

#[tauri::command]
pub fn neural_runtime_status(app: AppHandle) -> Result<NeuralRuntimeStatus, String> {
    let _ = data_packs::load_manifest(&app)?;
    let recommended = recommended_flavor();
    let manifest = data_packs::load_manifest(&app)?;
    let pack_meta = manifest.pack("runtime", &runtime_pack_key(recommended));
    let approx_bytes = pack_meta
        .map(|m| m.bytes)
        .filter(|&n| n > 0)
        .unwrap_or_else(|| runtime_approx_bytes(recommended));
    let selected = select_runtime(&app);
    let outdated = runtime_outdated(&app, selected.as_ref().map(|(f, _)| *f));
    let rec_accel = recommended_accelerator();
    let cuda_capable = platform_supports_cuda();
    let Some((flavor, lib)) = selected else {
        return Ok(NeuralRuntimeStatus {
            ready: false,
            outdated: false,
            version: pack_meta.map(|m| m.version.clone()),
            library_path: None,
            bytes: 0,
            approx_bytes,
            flavor: RuntimeFlavor::Cpu.as_str().into(),
            recommended_flavor: recommended.as_str().into(),
            accelerator: "none".into(),
            recommended_accelerator: rec_accel.as_str().into(),
            cuda_capable,
            gpu: false,
        });
    };
    let bytes = fs::metadata(&lib).map(|m| m.len()).unwrap_or(0);
    let version = installed_runtime_version(&app);
    let accel = active_accelerator(&app);
    Ok(NeuralRuntimeStatus {
        ready: true,
        outdated,
        version,
        library_path: Some(lib.to_string_lossy().into_owned()),
        bytes,
        approx_bytes,
        flavor: flavor.as_str().into(),
        recommended_flavor: recommended.as_str().into(),
        accelerator: accel.as_str().into(),
        recommended_accelerator: rec_accel.as_str().into(),
        cuda_capable,
        gpu: accel.is_gpu(),
    })
}

#[tauri::command]
pub async fn init_neural_runtime(
    app: AppHandle,
    network: NetworkConfig,
) -> Result<NeuralRuntimeStatus, String> {
    tauri::async_runtime::spawn_blocking(move || init_neural_runtime_sync(app, network))
        .await
        .map_err(|err| err.to_string())?
}

fn emit_runtime_progress(app: &AppHandle, percent: Option<u32>, done: u64, total: Option<u64>) {
    let _ = app.emit(
        "asset-progress",
        super::ProgressEvent {
            kind: "runtime".into(),
            id: "ort".into(),
            done,
            total,
            percent,
        },
    );
}

fn init_neural_runtime_sync(
    app: AppHandle,
    network: NetworkConfig,
) -> Result<NeuralRuntimeStatus, String> {
    let flavor = recommended_flavor();
    let stem = runtime_stem(flavor);

    if let Some(existing) = flavor_library(&app, flavor) {
        emit_runtime_progress(&app, Some(100), 1, Some(1));
        crate::app_log::info(
            "neural",
            format!(
                "runtime already present flavor={} path={}",
                flavor.as_str(),
                existing.display()
            ),
        );
        return neural_runtime_status(app);
    }

    let dest = data_packs::install_pack(&app, "runtime", "ort", &stem, &network)?;
    // Ensure library is at pack root (flatten accidental nested folders).
    if find_library(&dest).is_none() {
        return Err("ORT library not found in downloaded pack".into());
    }
    // Also fetch CPU when installing CUDA so offline/no-GPU machines can fall back later?
    // Plan: only install recommended. CPU-only machines get CPU.

    emit_runtime_progress(&app, Some(100), 1, Some(1));
    crate::app_log::info(
        "neural",
        format!("runtime ready flavor={} dir={}", flavor.as_str(), dest.display()),
    );
    neural_runtime_status(app)
}
