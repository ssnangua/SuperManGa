//! Per-reader log buffers and paired log windows.
//!
//! Owner = reader webview label (usually `main`). Log window label = `log-{owner}`.
//! Unscoped Rust logs (no task owner) go to stderr only.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindowBuilder};

const CAPACITY: usize = 5_000;
const EVENT: &str = "app-log";
const CLEAR_EVENT: &str = "app-log-clear";
const LOG_PREFIX: &str = "log-";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub ts_ms: u64,
    pub level: String,
    pub source: String,
    pub message: String,
    pub owner: String,
}

struct LogBuffer {
    next_id: u64,
    lines: VecDeque<LogEntry>,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self {
            next_id: 1,
            lines: VecDeque::with_capacity(1024),
        }
    }
}

struct LogStore {
    by_owner: HashMap<String, LogBuffer>,
}

static STORE: OnceLock<Mutex<LogStore>> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

thread_local! {
    static TASK_OWNER: Cell<Option<String>> = const { Cell::new(None) };
}

fn store() -> &'static Mutex<LogStore> {
    STORE.get_or_init(|| Mutex::new(LogStore {
        by_owner: HashMap::new(),
    }))
}

pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let _ = store();
}

pub fn log_window_label(owner: &str) -> String {
    format!("{LOG_PREFIX}{owner}")
}

pub fn is_log_window_label(label: &str) -> bool {
    label.starts_with(LOG_PREFIX)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Run `f` with `owner` as the current task log owner (for nested `info`/`warn`/`error`).
pub fn scope<R>(owner: impl Into<String>, f: impl FnOnce() -> R) -> R {
    let owner = owner.into();
    let prev = TASK_OWNER.with(|cell| cell.replace(Some(owner)));
    let result = f();
    TASK_OWNER.with(|cell| cell.set(prev));
    result
}

pub fn current_owner() -> Option<String> {
    TASK_OWNER.with(|cell| {
        let cur = cell.take();
        let cloned = cur.clone();
        cell.set(cur);
        cloned
    })
}

fn push_for(owner: &str, level: &str, source: &str, message: String) -> LogEntry {
    let entry = LogEntry {
        id: 0,
        ts_ms: now_ms(),
        level: level.to_string(),
        source: source.to_string(),
        message,
        owner: owner.to_string(),
    };
    let mut guard = store().lock().expect("log store poisoned");
    let buf = guard.by_owner.entry(owner.to_string()).or_default();
    let id = buf.next_id;
    buf.next_id = buf.next_id.saturating_add(1);
    let entry = LogEntry { id, ..entry };
    if buf.lines.len() >= CAPACITY {
        buf.lines.pop_front();
    }
    buf.lines.push_back(entry.clone());
    entry
}

fn emit_to_owner(owner: &str, event: &str, entry: &LogEntry) {
    if let Some(app) = APP.get() {
        let label = log_window_label(owner);
        let _ = app.emit_to(label.as_str(), event, entry);
    }
}

fn emit_clear(owner: &str) {
    if let Some(app) = APP.get() {
        let label = log_window_label(owner);
        let _ = app.emit_to(label.as_str(), CLEAR_EVENT, ());
    }
}

/// Write into a specific owner's buffer and emit to its log window.
pub fn write_for(owner: &str, level: &str, source: &str, message: impl AsRef<str>) {
    let message = message.as_ref().to_string();
    eprintln!("[{owner}/{source}] {message}");
    let entry = push_for(owner, level, source, message);
    emit_to_owner(owner, EVENT, &entry);
}

/// Rust-side log: uses task owner when set; otherwise stderr only.
pub fn write(level: &str, source: &str, message: impl AsRef<str>) {
    let message = message.as_ref().to_string();
    if let Some(owner) = current_owner() {
        write_for(&owner, level, source, message);
        return;
    }
    eprintln!("[{source}] {message}");
}

pub fn info(source: &str, message: impl AsRef<str>) {
    write("info", source, message);
}

pub fn warn(source: &str, message: impl AsRef<str>) {
    write("warn", source, message);
}

pub fn error(source: &str, message: impl AsRef<str>) {
    write("error", source, message);
}

pub fn info_for(owner: &str, source: &str, message: impl AsRef<str>) {
    write_for(owner, "info", source, message);
}

/// Emit a payload only to the paired log window for `owner`.
pub fn emit_to_log_window<S: Serialize + Clone>(owner: &str, event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let label = log_window_label(owner);
        let _ = app.emit_to(label.as_str(), event, payload);
    }
}

/// Emit UI sync (theme/locale) only to the paired log window.
pub fn emit_ui_sync_to_owner(owner: &str, payload: impl Serialize + Clone) {
    emit_to_log_window(owner, "app-ui-sync", payload);
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSyncPayload {
    pub theme: Option<String>,
    pub locale: Option<String>,
}

#[tauri::command]
pub fn sync_log_window_ui(owner: String, theme: Option<String>, locale: Option<String>) {
    let owner = owner.trim().to_string();
    if owner.is_empty() || is_log_window_label(&owner) {
        return;
    }
    emit_ui_sync_to_owner(
        &owner,
        UiSyncPayload { theme, locale },
    );
}

fn normalize_owner(owner: &str, fallback: &str) -> String {
    let trimmed = owner.trim();
    if trimmed.is_empty() || is_log_window_label(trimmed) {
        fallback.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
pub fn append_log(level: String, source: String, message: String, owner: Option<String>) -> LogEntry {
    // Frontend already prints via console.*; do not eprintln again.
    let level = match level.as_str() {
        "error" | "warn" | "info" | "debug" => level.as_str(),
        _ => "log",
    };
    let owner = normalize_owner(owner.as_deref().unwrap_or(""), &source);
    if owner.is_empty() || is_log_window_label(&owner) {
        return LogEntry {
            id: 0,
            ts_ms: now_ms(),
            level: level.to_string(),
            source,
            message,
            owner: String::new(),
        };
    }
    let entry = push_for(&owner, level, &source, message);
    emit_to_owner(&owner, EVENT, &entry);
    entry
}

#[tauri::command]
pub fn get_log_history(owner: String) -> Vec<LogEntry> {
    let owner = owner.trim().to_string();
    if owner.is_empty() {
        return Vec::new();
    }
    store()
        .lock()
        .expect("log store poisoned")
        .by_owner
        .get(&owner)
        .map(|buf| buf.lines.iter().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_logs(owner: String) {
    let owner = owner.trim().to_string();
    if owner.is_empty() {
        return;
    }
    if let Ok(mut guard) = store().lock() {
        if let Some(buf) = guard.by_owner.get_mut(&owner) {
            buf.lines.clear();
        }
    }
    emit_clear(&owner);
}

fn drop_owner_buffer(owner: &str) {
    if let Ok(mut guard) = store().lock() {
        guard.by_owner.remove(owner);
    }
}

fn parse_theme(theme: Option<&str>) -> Theme {
    match theme.map(str::trim) {
        Some("light") => Theme::Light,
        _ => Theme::Dark,
    }
}

/// Must be async on Windows — sync command + WebviewWindowBuilder deadlocks
/// (white frozen window that cannot be closed). See WebviewWindowBuilder::new docs.
#[tauri::command]
pub async fn open_log_window(
    app: AppHandle,
    owner: String,
    title: String,
    theme: Option<String>,
) -> Result<(), String> {
    let owner = owner.trim().to_string();
    if owner.is_empty() || is_log_window_label(&owner) {
        return Err("invalid log owner".into());
    }
    let label = log_window_label(&owner);
    let native_theme = parse_theme(theme.as_deref());
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        let _ = existing.set_theme(Some(native_theme));
        if !title.is_empty() {
            let _ = existing.set_title(&title);
        }
        return Ok(());
    }

    let title = if title.is_empty() {
        "Logs".to_string()
    } else {
        title
    };

    let url = format!("log.html?owner={}", urlencoding_owner(&owner));
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(900.0, 560.0)
        .min_inner_size(480.0, 280.0)
        .resizable(true)
        .decorations(true)
        .focused(true)
        .visible(true)
        .theme(Some(native_theme))
        .build()
        .map_err(|err| err.to_string())?;
    #[cfg(windows)]
    crate::window_state::apply_native_window_icons(&window);
    Ok(())
}

#[tauri::command]
pub fn close_log_window(app: AppHandle, owner: String) -> Result<(), String> {
    let owner = owner.trim().to_string();
    if owner.is_empty() {
        return Ok(());
    }
    let label = log_window_label(&owner);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.close();
    }
    drop_owner_buffer(&owner);
    Ok(())
}

fn urlencoding_owner(owner: &str) -> String {
    // Labels are simple identifiers (main, etc.); keep encoding minimal.
    owner
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}
