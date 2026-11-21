use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::archive::{self, names};
use crate::protocol;
use crate::state::{lock, AppState, Session};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedComic {
    pub session_id: u64,
    pub title: String,
    pub path: String,
    pub pages: Vec<archive::PageDto>,
    pub extracting: bool,
    pub initial_index: Option<usize>,
    pub protocol_origin: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageFileInfo {
    pub name: String,
    pub path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bits_per_pixel: Option<u16>,
    pub format: Option<String>,
    pub color: Option<String>,
    pub bytes: Option<u64>,
    pub modified_ms: Option<i64>,
    pub modified_text: Option<String>,
}

#[tauri::command]
pub async fn open_path(
    window: tauri::WebviewWindow,
    app: AppHandle,
    path: String,
    read_subfolder: bool,
    pdf_rasterize: Option<bool>,
    locale: String,
    password: Option<String>,
) -> Result<OpenedComic, String> {
    let owner = window.label().to_string();
    let pdf_rasterize = pdf_rasterize.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        crate::app_log::scope(owner, || {
            open_path_sync(app, path, read_subfolder, pdf_rasterize, locale, password)
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

fn open_path_sync(
    app: AppHandle,
    path: String,
    read_subfolder: bool,
    pdf_rasterize: bool,
    locale: String,
    password: Option<String>,
) -> Result<OpenedComic, String> {
    let state = app.state::<AppState>();
    let session_id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let loaded = archive::open(
        app.clone(),
        PathBuf::from(&path).as_path(),
        read_subfolder,
        pdf_rasterize,
        &locale,
        session_id,
        password.as_deref(),
    )?;
    let opened = OpenedComic {
        session_id,
        title: loaded.title.clone(),
        path: loaded.root.to_string_lossy().to_string(),
        pages: loaded.pages.clone(),
        extracting: loaded.extracting,
        initial_index: loaded.initial_index,
        protocol_origin: protocol::origin().to_string(),
    };
    state.measuring.store(0, Ordering::SeqCst);
    if let Some(previous) = lock(&state.session).replace(Session {
        id: session_id,
        pages: loaded.pages,
        backend: loaded.backend,
    }) {
        previous.backend.cancel();
    }
    Ok(opened)
}

#[tauri::command]
pub fn load_bookmarks(path: String) -> Result<String, String> {
    let file = names::sibling_bookmarks(PathBuf::from(&path).as_path());
    if !file.is_file() {
        return Ok(String::new());
    }
    fs::read_to_string(file).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_bookmarks(path: String, content: String) -> Result<(), String> {
    let file = names::sibling_bookmarks(PathBuf::from(&path).as_path());
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(file, content).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn delete_bookmarks(path: String) -> Result<(), String> {
    let file = names::sibling_bookmarks(PathBuf::from(&path).as_path());
    if file.is_file() {
        fs::remove_file(file).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn start_measure(
    window: tauri::WebviewWindow,
    app: AppHandle,
    session_id: u64,
) -> Result<(), String> {
    let owner = window.label().to_string();
    crate::app_log::scope(owner, || archive::start_measure(app, session_id))
}

#[tauri::command]
pub fn set_extract_priority(
    app: AppHandle,
    session_id: u64,
    pages: Vec<usize>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let guard = lock(&state.session);
    let session = guard.as_ref().ok_or_else(|| "no comic".to_string())?;
    if session.id != session_id {
        return Err("stale comic".into());
    }
    session.backend.set_extract_priority(pages);
    Ok(())
}

#[tauri::command]
pub async fn save_page(app: AppHandle, session_id: u64, index: usize, dest: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let bytes = {
            let guard = lock(&state.session);
            let session = guard.as_ref().ok_or("no comic")?;
            if session.id != session_id {
                return Err("stale comic".to_string());
            }
            session.backend.read(index)?
        };
        fs::write(dest, bytes).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn page_file_info(app: AppHandle, session_id: u64, index: usize) -> Result<PageFileInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (backend, page) = {
            let guard = lock(&state.session);
            let session = guard.as_ref().ok_or("no comic")?;
            if session.id != session_id {
                return Err("stale comic".to_string());
            }
            let page = session.pages.get(index).ok_or("page is out of range")?.clone();
            (session.backend.clone(), page)
        };
        let stat = backend.file_stat(index, &page.rel_path);
        let bytes = backend.read(index).ok();
        let size = stat.bytes.or_else(|| bytes.as_ref().map(|b| b.len() as u64));
        let probe = bytes.as_deref().and_then(probe_image);
        let format = probe
            .as_ref()
            .and_then(|p| p.3.clone())
            .or_else(|| format_from_name(&page.name));
        Ok(PageFileInfo {
            name: page.name,
            path: stat.path,
            width: probe.as_ref().map(|p| p.0).or(Some(page.width).filter(|w| *w > 0)),
            height: probe.as_ref().map(|p| p.1).or(Some(page.height).filter(|h| *h > 0)),
            bits_per_pixel: probe.as_ref().and_then(|p| p.2),
            format,
            color: probe.as_ref().and_then(|p| p.4.clone()),
            bytes: size,
            modified_ms: stat.modified_ms,
            modified_text: stat.modified_text,
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

fn format_from_name(name: &str) -> Option<String> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("JPEG".into()),
        "png" => Some("PNG".into()),
        "webp" => Some("WebP".into()),
        "gif" => Some("GIF".into()),
        "bmp" => Some("BMP".into()),
        _ => None,
    }
}

fn sniff_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("PNG");
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return Some("JPEG");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("WebP");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("GIF");
    }
    if bytes.starts_with(b"BM") {
        return Some("BMP");
    }
    None
}

fn color_label(color: image::ColorType) -> &'static str {
    match color {
        image::ColorType::L8 | image::ColorType::L16 => "Grayscale",
        image::ColorType::La8 | image::ColorType::La16 => "Gray+Alpha",
        image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => "RGB",
        image::ColorType::Rgba8 | image::ColorType::Rgba16 | image::ColorType::Rgba32F => "RGBA",
        _ => "RGB",
    }
}

fn format_label(fmt: image::ImageFormat) -> &'static str {
    match fmt {
        image::ImageFormat::Jpeg => "JPEG",
        image::ImageFormat::Png => "PNG",
        image::ImageFormat::Gif => "GIF",
        image::ImageFormat::WebP => "WebP",
        image::ImageFormat::Bmp => "BMP",
        image::ImageFormat::Tiff => "TIFF",
        _ => "Image",
    }
}

fn probe_image(bytes: &[u8]) -> Option<(u32, u32, Option<u16>, Option<String>, Option<String>)> {
    let sniffed = sniff_format(bytes).map(str::to_string);
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let format = reader.format().map(format_label).map(str::to_string).or(sniffed.clone());
    let decoded = reader.decode().ok()?;
    Some((
        decoded.width(),
        decoded.height(),
        Some(decoded.color().bits_per_pixel()),
        format.or(sniffed),
        Some(color_label(decoded.color()).to_string()),
    ))
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    SystemInfo {
        version: env!("APP_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[tauri::command]
pub fn take_pending_open(app: AppHandle) -> Option<String> {
    lock(&app.state::<AppState>().pending_open).take()
}

#[tauri::command]
pub fn is_primary_instance() -> bool {
    crate::instance::is_primary_instance()
}

#[tauri::command]
pub fn get_shell_integration() -> crate::association::ShellIntegration {
    crate::association::get_shell_integration()
}

#[tauri::command]
pub fn set_context_menu(enabled: bool, label: String) -> Result<(), String> {
    crate::association::set_context_menu(enabled, &label)
}

#[tauri::command]
pub fn apply_file_associations(extensions: Vec<String>, description: String) -> Result<(), String> {
    crate::association::apply_file_associations(&extensions, &description)
}

fn file_mtime_ms(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn collect_dir_stamp(
    dir: &std::path::Path,
    root: &std::path::Path,
    recursive: bool,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if archive::names::is_junk(&rel) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if recursive {
                collect_dir_stamp(&path, root, true, out)?;
            }
            continue;
        }
        if archive::names::is_image(&rel) {
            out.push(format!("{}:{}:{}", rel, meta.len(), file_mtime_ms(&meta)));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn relaunch_app(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub fn path_stamp(path: String, read_subfolder: bool) -> Result<String, String> {
    let root = PathBuf::from(&path);
    let meta = fs::metadata(&root).map_err(|err| err.to_string())?;
    if meta.is_file() {
        return Ok(format!("f:{}:{}", meta.len(), file_mtime_ms(&meta)));
    }
    let mut parts = Vec::new();
    collect_dir_stamp(&root, &root, read_subfolder, &mut parts)?;
    parts.sort();
    Ok(parts.join("|"))
}
