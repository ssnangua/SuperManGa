//! Detect a usable CUDA toolkit so we can download the matching ORT GPU package.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFlavor {
    Cpu,
    Cuda12,
    Cuda13,
}

impl RuntimeFlavor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda12 => "cuda12",
            Self::Cuda13 => "cuda13",
        }
    }

    pub fn is_cuda(self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

struct DetectedCuda {
    flavor: RuntimeFlavor,
    bin_dir: Option<PathBuf>,
}

static DETECTED: OnceLock<DetectedCuda> = OnceLock::new();

fn gpu_ort_supported() -> bool {
    cfg!(all(
        any(target_os = "windows", target_os = "linux"),
        target_arch = "x86_64"
    ))
}

pub fn recommended_flavor() -> RuntimeFlavor {
    detect().flavor
}

pub fn cuda_bin_dir() -> Option<PathBuf> {
    detect().bin_dir.clone()
}

fn detect() -> &'static DetectedCuda {
    DETECTED.get_or_init(|| {
        if !gpu_ort_supported() {
            return DetectedCuda {
                flavor: RuntimeFlavor::Cpu,
                bin_dir: None,
            };
        }
        let mut best: Option<(u32, u32, PathBuf)> = None;
        for (maj, min, root) in collect_cuda_roots() {
            let better = match best {
                None => true,
                Some((bmaj, bmin, _)) => (maj, min) > (bmaj, bmin),
            };
            if better {
                best = Some((maj, min, root));
            }
        }
        if let Some((maj, min, root)) = best {
            let flavor = flavor_for(maj, min);
            let bin = cuda_bin_in(&root);
            if flavor.is_cuda() && cuda_libs_ready(flavor) {
                return DetectedCuda {
                    flavor,
                    bin_dir: bin,
                };
            }
        }
        if has_cudart(13) && cuda_libs_ready(RuntimeFlavor::Cuda13) {
            return DetectedCuda {
                flavor: RuntimeFlavor::Cuda13,
                bin_dir: None,
            };
        }
        DetectedCuda {
            flavor: RuntimeFlavor::Cpu,
            bin_dir: None,
        }
    })
}

fn flavor_for(major: u32, minor: u32) -> RuntimeFlavor {
    if major >= 13 {
        RuntimeFlavor::Cuda13
    } else if major == 12 && minor >= 8 {
        RuntimeFlavor::Cuda12
    } else {
        RuntimeFlavor::Cpu
    }
}

fn cuda_bin_in(root: &Path) -> Option<PathBuf> {
    let bin = root.join("bin");
    // CUDA 13 Windows layout: bin/x64/cudart64_13.dll
    let bin_x64 = bin.join("x64");
    if bin_x64.is_dir() {
        return Some(bin_x64);
    }
    if bin.is_dir() {
        return Some(bin);
    }
    let lib64 = root.join("lib64");
    if lib64.is_dir() {
        return Some(lib64);
    }
    None
}

fn collect_cuda_roots() -> Vec<(u32, u32, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(path) = env::var("CUDA_PATH") {
        push_root(&mut out, PathBuf::from(path));
    }
    for (key, value) in env::vars() {
        if key.starts_with("CUDA_PATH") {
            push_root(&mut out, PathBuf::from(value));
        }
    }
    #[cfg(windows)]
    {
        let base = PathBuf::from(r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA");
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                push_root(&mut out, entry.path());
            }
        }
    }
    #[cfg(unix)]
    {
        push_root(&mut out, PathBuf::from("/usr/local/cuda"));
        if let Ok(entries) = std::fs::read_dir("/usr/local") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("cuda") {
                    push_root(&mut out, entry.path());
                }
            }
        }
    }
    out
}

fn push_root(out: &mut Vec<(u32, u32, PathBuf)>, path: PathBuf) {
    if !path.exists() {
        return;
    }
    let label = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if let Some((maj, min)) = parse_cuda_ver(label) {
        out.push((maj, min, path));
        return;
    }
    if let Some((maj, min)) = path
        .to_str()
        .and_then(|s| s.split(['/', '\\']).rev().find_map(parse_cuda_ver))
    {
        out.push((maj, min, path));
    }
}

fn parse_cuda_ver(raw: &str) -> Option<(u32, u32)> {
    let s = raw
        .trim()
        .trim_start_matches('v')
        .trim_start_matches("V")
        .trim_start_matches("cuda-")
        .trim_start_matches("cuda");
    let mut nums = s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
    let major: u32 = nums.next()?.parse().ok()?;
    if !(11..=13).contains(&major) {
        return None;
    }
    let minor: u32 = nums.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

fn has_cudart(major: u32) -> bool {
    #[cfg(windows)]
    {
        let name = format!("cudart64_{major}.dll");
        if dll_on_path(&name) {
            return true;
        }
        if let Ok(cuda) = env::var("CUDA_PATH") {
            if PathBuf::from(cuda).join("bin").join(&name).is_file() {
                return true;
            }
        }
        false
    }
    #[cfg(not(windows))]
    {
        let name = format!("libcudart.so.{major}");
        if let Ok(ld) = env::var("LD_LIBRARY_PATH") {
            for dir in ld.split(':') {
                if Path::new(dir).join(&name).is_file() {
                    return true;
                }
            }
        }
        Path::new("/usr/lib").join(&name).is_file()
            || Path::new("/usr/lib/x86_64-linux-gnu").join(&name).is_file()
            || Path::new("/usr/local/cuda/lib64").join(&name).is_file()
    }
}

#[cfg(windows)]
fn dll_on_path(name: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(';') {
            if Path::new(dir).join(name).is_file() {
                return true;
            }
        }
    }
    false
}

pub fn prepend_search_path(dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    let dir = dir.to_string_lossy();
    #[cfg(windows)]
    {
        let old = env::var("PATH").unwrap_or_default();
        if old.split(';').any(|p| p == dir.as_ref()) {
            return;
        }
        unsafe {
            env::set_var("PATH", format!("{dir};{old}"));
        }
    }
    #[cfg(not(windows))]
    {
        let key = "LD_LIBRARY_PATH";
        let old = env::var(key).unwrap_or_default();
        if old.split(':').any(|p| p == dir.as_ref()) {
            return;
        }
        let next = if old.is_empty() {
            dir.into_owned()
        } else {
            format!("{dir}:{old}")
        };
        unsafe {
            env::set_var(key, next);
        }
    }
}

pub fn prepare_runtime_search_path(lib: &Path, flavor: RuntimeFlavor) {
    if let Some(dir) = lib.parent() {
        prepend_search_path(dir);
    }
    if let Some(bin) = cuda_bin_dir() {
        prepend_search_path(&bin);
    }
    if let Some(dir) = find_cublas_dir(flavor) {
        prepend_search_path(&dir);
    }
    if let Some(dir) = find_cudnn_dir(flavor) {
        prepend_search_path(&dir);
    }
}

pub fn cuda_ep_usable(flavor: RuntimeFlavor) -> bool {
    flavor.is_cuda() && cuda_libs_ready(flavor)
}

fn cuda_libs_ready(flavor: RuntimeFlavor) -> bool {
    find_cublas_dir(flavor).is_some() && find_cudnn_dir(flavor).is_some()
}

fn cublas_lt_name(flavor: RuntimeFlavor) -> Option<&'static str> {
    #[cfg(windows)]
    {
        return match flavor {
            RuntimeFlavor::Cuda12 => Some("cublasLt64_12.dll"),
            RuntimeFlavor::Cuda13 => Some("cublasLt64_13.dll"),
            RuntimeFlavor::Cpu => None,
        };
    }
    #[cfg(not(windows))]
    match flavor {
        RuntimeFlavor::Cuda12 => Some("libcublasLt.so.12"),
        RuntimeFlavor::Cuda13 => Some("libcublasLt.so.13"),
        RuntimeFlavor::Cpu => None,
    }
}

fn path_cuda_major(path: &Path) -> Option<u32> {
    path.components().rev().find_map(|component| {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with("13.") || name.as_ref() == "13" {
            Some(13)
        } else if name.starts_with("12.") {
            Some(12)
        } else {
            None
        }
    })
}

fn pick_matching_dir(dirs: &[PathBuf], file_name: &str, flavor: RuntimeFlavor) -> Option<PathBuf> {
    let want = match flavor {
        RuntimeFlavor::Cuda12 => 12,
        RuntimeFlavor::Cuda13 => 13,
        RuntimeFlavor::Cpu => return None,
    };
    let mut generic = None;
    for dir in dirs {
        if !dir.join(file_name).is_file() {
            continue;
        }
        match path_cuda_major(dir) {
            Some(major) if major == want => return Some(dir.clone()),
            Some(_) => {}
            None if generic.is_none() => generic = Some(dir.clone()),
            None => {}
        }
    }
    generic
}

fn collect_named_dirs(root: &Path, file_name: &str, depth: u32, out: &mut Vec<PathBuf>) {
    if depth == 0 || !root.is_dir() {
        return;
    }
    if root.join(file_name).is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_named_dirs(&path, file_name, depth.saturating_sub(1), out);
        }
    }
}

fn find_cublas_dir(flavor: RuntimeFlavor) -> Option<PathBuf> {
    let name = cublas_lt_name(flavor)?;
    let mut dirs = Vec::new();
    if let Ok(cuda) = env::var("CUDA_PATH") {
        let root = PathBuf::from(cuda);
        dirs.push(root.join("bin"));
        dirs.push(root.join("bin").join("x64"));
        dirs.push(root.join("lib"));
        dirs.push(root.join("lib64"));
        dirs.push(root.join("lib").join("x64"));
    }
    for (_, _, root) in collect_cuda_roots() {
        dirs.push(root.join("bin"));
        dirs.push(root.join("bin").join("x64"));
        dirs.push(root.join("lib64"));
    }
    #[cfg(windows)]
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(';') {
            if !dir.is_empty() {
                dirs.push(PathBuf::from(dir));
            }
        }
    }
    pick_matching_dir(&dirs, name, flavor)
}

fn find_cudnn_dir(flavor: RuntimeFlavor) -> Option<PathBuf> {
    #[cfg(windows)]
    const NAME: &str = "cudnn64_9.dll";
    #[cfg(not(windows))]
    const NAME: &str = "libcudnn.so.9";

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(cuda) = env::var("CUDA_PATH") {
        let root = PathBuf::from(cuda);
        dirs.push(root.join("bin"));
        dirs.push(root.join("bin").join("x64"));
        dirs.push(root.join("lib"));
        dirs.push(root.join("lib64"));
    }
    for (key, value) in env::vars() {
        if key.starts_with("CUDNN") {
            let p = PathBuf::from(value);
            dirs.push(p.clone());
            dirs.push(p.join("bin"));
            dirs.push(p.join("lib"));
            dirs.push(p.join("lib64"));
        }
    }
    #[cfg(windows)]
    {
        collect_named_dirs(
            Path::new(r"C:\Program Files\NVIDIA\CUDNN"),
            NAME,
            4,
            &mut dirs,
        );
        for (_, _, root) in collect_cuda_roots() {
            dirs.push(root.join("bin"));
            dirs.push(root.join("bin").join("x64"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(ld) = env::var("LD_LIBRARY_PATH") {
            for dir in ld.split(':') {
                dirs.push(PathBuf::from(dir));
            }
        }
        dirs.push(PathBuf::from("/usr/lib"));
        dirs.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        dirs.push(PathBuf::from("/usr/local/cuda/lib64"));
        dirs.push(PathBuf::from("/usr/local/cudnn/lib"));
        collect_named_dirs(Path::new("/usr/local/cudnn"), NAME, 4, &mut dirs);
    }

    pick_matching_dir(&dirs, NAME, flavor)
}
