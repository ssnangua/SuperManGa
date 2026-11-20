use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::names::cache_filename;

pub const CACHE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Manifest {
    complete: bool,
    files: Vec<String>,
}

struct CacheInner {
    paths: Vec<Option<PathBuf>>,
    finished: bool,
    error: Option<String>,
}

pub struct CacheBackend {
    inner: Mutex<CacheInner>,
    cv: Condvar,
    cancel: std::sync::atomic::AtomicBool,
}

impl CacheBackend {
    pub fn new(count: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner {
                paths: vec![None; count],
                finished: false,
                error: None,
            }),
            cv: Condvar::new(),
            cancel: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn len(&self) -> usize {
        let guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.paths.len()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn mark(&self, index: usize, path: PathBuf) {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(slot) = guard.paths.get_mut(index) {
            *slot = Some(path);
        }
        self.cv.notify_all();
    }

    pub fn cancel(&self) {
        self.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.finished = true;
        if guard.error.is_none() {
            guard.error = Some("cancelled".to_string());
        }
        self.cv.notify_all();
    }

    pub fn finish(&self, error: Option<String>) {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.finished = true;
        if error.is_some() {
            guard.error = error;
        }
        self.cv.notify_all();
    }

    pub fn wait(&self, index: usize) -> Result<PathBuf, String> {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        loop {
            if let Some(Some(path)) = guard.paths.get(index) {
                return Ok(path.clone());
            }
            if guard.finished || self.is_cancelled() {
                return Err(guard
                    .error
                    .clone()
                    .unwrap_or_else(|| "page is not available".to_string()));
            }
            guard = self.cv.wait(guard).unwrap_or_else(|err| err.into_inner());
        }
    }
}

pub fn cache_key(path: &Path) -> String {
    let meta = fs::metadata(path).ok();
    let len = meta.as_ref().map(|meta| meta.len()).unwrap_or(0);
    let modified = meta
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for byte in len.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for byte in modified.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn try_reuse(cache_root: &Path, key: &str, count: usize) -> Option<Vec<PathBuf>> {
    let link = fs::read_to_string(cache_root.join(format!("{key}.link"))).ok()?;
    let dir = cache_root.join(link.trim());
    let manifest: Manifest =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).ok()?).ok()?;
    if !manifest.complete || manifest.files.len() != count {
        return None;
    }
    let paths: Vec<_> = manifest.files.iter().map(|file| dir.join(file)).collect();
    if paths.iter().all(|path| path.is_file()) {
        Some(paths)
    } else {
        None
    }
}

pub fn publish(work_dir: &Path, link_path: &Path, link_value: &str, files: &[String]) -> Result<(), String> {
    let manifest = Manifest {
        complete: true,
        files: files.to_vec(),
    };
    let text = serde_json::to_string(&manifest).map_err(|err| err.to_string())?;
    fs::write(work_dir.join("manifest.json"), text).map_err(|err| err.to_string())?;
    fs::write(link_path, link_value).map_err(|err| err.to_string())?;
    Ok(())
}

pub fn file_names(originals: &[String]) -> Vec<String> {
    originals
        .iter()
        .enumerate()
        .map(|(index, name)| cache_filename(index, name))
        .collect()
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += dir_size(&path);
        } else {
            total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        }
    }
    total
}

pub fn evict(root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path == keep {
            continue;
        }
        let modified = entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        dirs.push((modified, dir_size(&path), path));
    }
    let mut total: u64 = dirs.iter().map(|(_, size, _)| size).sum();
    dirs.sort_by_key(|(modified, _, _)| *modified);
    for (_, size, path) in dirs {
        if total <= CACHE_LIMIT {
            break;
        }
        if fs::remove_dir_all(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}
