use tauri::{AppHandle, Manager};

use crate::archive::names::mime_of;
use crate::neural_sr::cache::resolve_results_root;
use crate::neural_sr::results::load_cached_webp;
use crate::state::{lock, AppState};

pub fn origin() -> &'static str {
    if cfg!(any(windows, target_os = "android")) {
        "http://comic.localhost"
    } else {
        "comic://localhost"
    }
}

pub fn neural_respond(app: &AppHandle, uri: &tauri::http::Uri) -> tauri::http::Response<Vec<u8>> {
    match read_neural(app, uri) {
        Ok(bytes) => with_cors(
            tauri::http::Response::builder()
                .status(200)
                .header(tauri::http::header::CONTENT_TYPE, "image/webp")
                .header(tauri::http::header::CACHE_CONTROL, "private, max-age=86400"),
            bytes,
        ),
        Err(message) => with_cors(
            tauri::http::Response::builder()
                .status(404)
                .header(tauri::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header(tauri::http::header::CACHE_CONTROL, "no-store"),
            message.into_bytes(),
        ),
    }
}

fn read_neural(app: &AppHandle, uri: &tauri::http::Uri) -> Result<Vec<u8>, String> {
    let key = neural_key(uri).ok_or_else(|| "bad neural url".to_string())?;
    let root = resolve_results_root(app)?;
    load_cached_webp(&root, &key)?.ok_or_else(|| "missing".to_string())
}

fn neural_key(uri: &tauri::http::Uri) -> Option<String> {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return None;
    }
    let decoded = percent_decode_path(path);
    if decoded.is_empty() || decoded.contains("..") {
        return None;
    }
    Some(decoded)
}

fn percent_decode_path(input: &str) -> String {
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

pub fn respond(app: &AppHandle, uri: &tauri::http::Uri) -> tauri::http::Response<Vec<u8>> {
    match read(app, uri) {
        Ok((mime, bytes)) => with_cors(
            tauri::http::Response::builder()
                .status(200)
                .header(tauri::http::header::CONTENT_TYPE, mime)
                .header(tauri::http::header::CACHE_CONTROL, "private, max-age=86400"),
            bytes,
        ),
        Err(message) => with_cors(
            tauri::http::Response::builder()
                .status(404)
                .header(tauri::http::header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            message.into_bytes(),
        ),
    }
}

fn with_cors(
    builder: tauri::http::response::Builder,
    body: Vec<u8>,
) -> tauri::http::Response<Vec<u8>> {
    builder
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .body(body)
        .unwrap_or_else(|_| empty(500))
}

fn empty(status: u16) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Vec::new())
        .expect("empty response")
}

fn read(app: &AppHandle, uri: &tauri::http::Uri) -> Result<(String, Vec<u8>), String> {
    let (session_id, index) = parse(uri).ok_or_else(|| "bad comic url".to_string())?;
    let state = app.state::<AppState>();
    let backend = {
        let guard = lock(&state.session);
        let session = guard.as_ref().ok_or_else(|| "no comic".to_string())?;
        if session.id != session_id {
            return Err("stale comic".to_string());
        }
        if index >= session.pages.len() {
            return Err("page is out of range".to_string());
        }
        (session.backend.clone(), session.pages[index].name.clone())
    };
    let bytes = backend.0.read(index)?;
    Ok((mime_of(&backend.1).to_string(), bytes))
}

fn parse(uri: &tauri::http::Uri) -> Option<(u64, usize)> {
    let mut parts = uri.path().split('/').filter(|part| !part.is_empty());
    let session_id = parts.next()?.parse().ok()?;
    let index = parts.next()?.parse().ok()?;
    Some((session_id, index))
}
