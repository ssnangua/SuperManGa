//! Shared ONNX Runtime environment for neural SR and OCR.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use tauri::AppHandle;

use super::cuda::{cuda_ep_usable, prepare_runtime_search_path};
use super::runtime::{installed_flavor, installed_runtime_version, library_path};

struct OrtState {
    lib_path: PathBuf,
    version: String,
    env: Result<Environment, String>,
}

static ORT_STATE: OnceLock<OrtState> = OnceLock::new();
static ORT_LOCK: Mutex<()> = Mutex::new(());

pub fn ensure_ort(app: &AppHandle) -> Result<&'static Environment, String> {
    let _guard = ORT_LOCK.lock().map_err(|err| err.to_string())?;
    let lib = library_path(app).ok_or_else(|| {
        "运行环境未就绪，请先在「设置 → 超分」中初始化运行环境".to_string()
    })?;
    let version = installed_runtime_version(app).unwrap_or_default();
    prepare_runtime_search_path(&lib, installed_flavor(app));

    if let Some(state) = ORT_STATE.get() {
        if state.lib_path != lib || state.version != version {
            return Err(
                "运行环境已更新，但当前进程仍在使用旧版 ORT。请完全退出应用后重新打开。".into(),
            );
        }
        return state.env.as_ref().map_err(|err| err.clone());
    }

    let state = ORT_STATE.get_or_init(|| {
        let env = (|| {
            ort::init_from(lib.to_string_lossy().as_ref())
                .map_err(|err| format!("failed to load ORT: {err}"))?
                .build()
                .map_err(|err| format!("failed to init ORT env: {err}"))
        })();
        OrtState {
            lib_path: lib.clone(),
            version: version.clone(),
            env,
        }
    });
    state.env.as_ref().map_err(|err| err.clone())
}

pub fn want_cuda(app: &AppHandle) -> bool {
    cuda_ep_usable(installed_flavor(app))
}

pub fn build_session(env: &Environment, onnx: &Path, use_cuda: bool) -> Result<Session, String> {
    let mut builder = Session::builder(env)
        .map_err(|err| err.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level2)
        .unwrap_or_else(|err| err.recover())
        .with_intra_threads(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
        )
        .unwrap_or_else(|err| err.recover());
    if use_cuda {
        builder = builder
            .with_execution_providers([ort::ep::CUDA::default().build()])
            .unwrap_or_else(|err| err.recover());
    }
    builder
        .commit_from_file(onnx)
        .map_err(|err| format!("failed to load model: {err}"))
}

pub fn is_cuda_runtime_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("cudnn")
        || lower.contains("cublas")
        || lower.contains("cuda execution provider")
        || lower.contains("cuda_kernel")
}

pub fn open_session(app: &AppHandle, onnx: &Path) -> Result<Session, String> {
    let env = ensure_ort(app)?;
    let use_cuda = want_cuda(app);
    match build_session(env, onnx, use_cuda) {
        Ok(s) => Ok(s),
        Err(err) if use_cuda && is_cuda_runtime_error(&err) => build_session(env, onnx, false),
        Err(err) => Err(err),
    }
}
