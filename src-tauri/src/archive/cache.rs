use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
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
    /// Preferred page indexes (front = highest). Demand from `wait` / UI bumps this.
    priority: Mutex<Vec<usize>>,
    /// Index currently being extracted, or -1.
    running: AtomicIsize,
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
            priority: Mutex::new(Vec::new()),
            running: AtomicIsize::new(-1),
        }
    }

    pub fn len(&self) -> usize {
        let guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.paths.len()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn set_priority(&self, pages: Vec<usize>) {
        let mut guard = self.priority.lock().unwrap_or_else(|err| err.into_inner());
        *guard = pages;
    }

    pub fn boost_priority(&self, index: usize) {
        let mut guard = self.priority.lock().unwrap_or_else(|err| err.into_inner());
        guard.retain(|&page| page != index);
        guard.insert(0, index);
    }

    pub fn set_running(&self, index: Option<usize>) {
        self.running.store(
            index.map(|value| value as isize).unwrap_or(-1),
            Ordering::SeqCst,
        );
    }

    pub fn running(&self) -> Option<usize> {
        let value = self.running.load(Ordering::SeqCst);
        if value < 0 {
            None
        } else {
            Some(value as usize)
        }
    }

    /// Next page that still needs extraction (UI priority order first, then lowest index).
    /// Frontend typically sends a full radiate-from-focus list while extracting.
    pub fn next_unready(&self) -> Option<usize> {
        let inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        let priority = self.priority.lock().unwrap_or_else(|err| err.into_inner());
        for &page in priority.iter() {
            if inner.paths.get(page).is_some_and(|slot| slot.is_none()) {
                return Some(page);
            }
        }
        inner.paths.iter().position(|slot| slot.is_none())
    }

    pub fn mark(&self, index: usize, path: PathBuf) {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(slot) = guard.paths.get_mut(index) {
            *slot = Some(path);
        }
        self.cv.notify_all();
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.running.store(-1, Ordering::SeqCst);
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.finished = true;
        if guard.error.is_none() {
            guard.error = Some("cancelled".to_string());
        }
        self.cv.notify_all();
    }

    pub fn finish(&self, error: Option<String>) {
        self.running.store(-1, Ordering::SeqCst);
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.finished = true;
        if error.is_some() {
            guard.error = error;
        }
        self.cv.notify_all();
    }

    pub fn wait(&self, index: usize) -> Result<PathBuf, String> {
        // Prefer the page the UI is actually trying to show.
        self.boost_priority(index);
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

/// PDF rasterize vs image-extract must not share a cache directory.
pub fn pdf_cache_key(path: &Path, rasterize: bool) -> String {
    let mode = if rasterize { "pdf-r" } else { "pdf-i" };
    format!("{}-{mode}", cache_key(path))
}

pub fn pdf_other_cache_key(path: &Path, rasterize: bool) -> String {
    pdf_cache_key(path, !rasterize)
}

/// Remove a completed cache link and any in-progress dirs for `key`.
pub fn purge_key(cache_root: &Path, key: &str) {
    let link_path = cache_root.join(format!("{key}.link"));
    if let Ok(link) = fs::read_to_string(&link_path) {
        let dir = cache_root.join(link.trim());
        let _ = fs::remove_dir_all(dir);
    }
    let _ = fs::remove_file(&link_path);
    let prefix = format!("{key}-");
    let Ok(entries) = fs::read_dir(cache_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) {
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
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
