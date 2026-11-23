use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager};

use crate::state::{lock, AppState};

#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
const OPEN_EVENT: &str = "open-path";

fn collect_paths(args: impl IntoIterator<Item = String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for maybe_file in args {
        if maybe_file.starts_with('-') {
            continue;
        }
        if let Some(path) = parse_arg_path(&maybe_file) {
            files.push(path);
        }
    }
    files
}

fn parse_arg_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("file://") {
        let decoded = percent_decode(rest);
        #[cfg(windows)]
        let decoded = decoded
            .trim_start_matches('/')
            .replace('/', "\\");
        return Some(PathBuf::from(decoded));
    }
    if trimmed.contains("://") {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (from_hex(bytes[index + 1]), from_hex(bytes[index + 2]))
        {
            out.push((hi << 4) | lo);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn first_openable(paths: &[PathBuf]) -> Option<String> {
    paths.iter().find_map(|path| {
        if is_openable(path) {
            Some(path.to_string_lossy().into_owned())
        } else {
            None
        }
    })
}

fn is_openable(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("zip" | "cbz" | "rar" | "cbr" | "7z" | "epub" | "pdf" | "mobi" | "azw3" | "azw")
    )
}

#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub fn queue_pending(app: &AppHandle, path: String) {
    *lock(&app.state::<AppState>().pending_open) = Some(path.clone());
    let _ = app.emit(OPEN_EVENT, path);
}

/// First openable path from argv (all desktop platforms).
/// macOS Finder / `open -a App file` still use `RunEvent::Opened`; this covers
/// direct binary launches and `open -a App --args /path/to/book.cbz`.
pub fn take_launch_args() -> Option<String> {
    let paths = collect_paths(std::env::args().skip(1));
    first_openable(&paths)
}
