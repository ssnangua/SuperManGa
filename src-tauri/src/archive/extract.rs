use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use super::cache::{self, CacheBackend};
use super::names::cache_filename;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProgress {
    pub session_id: u64,
    pub done: usize,
    pub total: usize,
    pub finished: bool,
    pub error: Option<String>,
}

pub struct ExtractJob {
    pub cache: Arc<CacheBackend>,
    pub app: AppHandle,
    pub session_id: u64,
    pub total: usize,
    pub work_dir: PathBuf,
    pub link_path: PathBuf,
    pub link_value: String,
    done: AtomicUsize,
}

impl ExtractJob {
    pub fn new(
        cache: Arc<CacheBackend>,
        app: AppHandle,
        session_id: u64,
        total: usize,
        cache_root: &Path,
        key: &str,
    ) -> Result<Self, String> {
        let link_value = format!("{key}-{session_id}");
        let work_dir = cache_root.join(&link_value);
        if work_dir.exists() {
            fs::remove_dir_all(&work_dir).map_err(|err| err.to_string())?;
        }
        fs::create_dir_all(&work_dir).map_err(|err| err.to_string())?;
        cache::evict(cache_root, &work_dir);
        Ok(Self {
            cache,
            app,
            session_id,
            total,
            work_dir,
            link_path: cache_root.join(format!("{key}.link")),
            link_value,
            done: AtomicUsize::new(0),
        })
    }

    pub fn cancelled(&self) -> bool {
        self.cache.is_cancelled()
    }

    pub fn destination(&self, index: usize, original: &str) -> (PathBuf, String) {
        let name = cache_filename(index, original);
        (self.work_dir.join(&name), name)
    }

    pub fn mark_written(&self, index: usize, path: PathBuf) {
        if let Some(size) = super::measure::file(&path) {
            super::measure::emit(&self.app, self.session_id, index, size, None);
        }
        self.cache.mark(index, path);
        let done = self.done.fetch_add(1, Ordering::SeqCst) + 1;
        self.emit(done, false, None);
    }

    pub fn succeed(&self, files: Vec<String>) {
        if self.cancelled() {
            self.abort();
            return;
        }
        if let Err(err) = cache::publish(&self.work_dir, &self.link_path, &self.link_value, &files) {
            self.fail(err);
            return;
        }
        self.cache.finish(None);
        self.emit(self.total, true, None);
    }

    pub fn fail(&self, message: String) {
        self.cache.finish(Some(message.clone()));
        self.emit(self.done.load(Ordering::SeqCst), true, Some(message));
    }

    pub fn abort(&self) {
        let _ = fs::remove_dir_all(&self.work_dir);
        self.cache.cancel();
        self.emit(self.done.load(Ordering::SeqCst), true, None);
    }

    fn emit(&self, done: usize, finished: bool, error: Option<String>) {
        let _ = self.app.emit(
            "extract-progress",
            ExtractProgress {
                session_id: self.session_id,
                done,
                total: self.total,
                finished,
                error,
            },
        );
    }
}
