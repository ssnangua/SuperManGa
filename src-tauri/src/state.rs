use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::archive::{Backend, PageDto};

pub struct Session {
    pub id: u64,
    pub pages: Vec<PageDto>,
    pub backend: Arc<Backend>,
}

pub struct AppState {
    pub session: Mutex<Option<Session>>,
    pub next_id: AtomicU64,
    pub measuring: AtomicU64,
    pub pending_open: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
            measuring: AtomicU64::new(0),
            pending_open: Mutex::new(None),
        }
    }
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}
