//! PDF support: prefer local pdfium for full rasterization; fall back to extracting
//! embedded page images (typical for scanned comics) when pdfium is unavailable.
//! Runtime download from GitHub is best-effort only (often blocked on some networks).

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use pdfium_render::prelude::*;
use tauri::{AppHandle, Manager};

use super::extract::ExtractJob;
use super::names::tr;

const MAX_EDGE: f32 = 4096.0;
const PDFIUM_VERSION: &str = "chromium/8152";

pub enum PdfPlan {
    Pdfium { page_count: usize },
    Embedded { images: Vec<EmbeddedImage> },
}

pub struct EmbeddedImage {
    pub rel: String,
    pub bytes: Vec<u8>,
}

/// Stable error token for the frontend password dialog (locale-independent).
pub const ERR_PASSWORD_REQUIRED: &str = "pdf_password_required";
/// Stable error token when a password was supplied but rejected.
pub const ERR_PASSWORD_INCORRECT: &str = "pdf_password_incorrect";

pub fn plan_open(
    app: &AppHandle,
    path: &Path,
    locale: &str,
    password: Option<&str>,
) -> Result<PdfPlan, String> {
    let encrypted = pdf_is_encrypted(path);

    // Prefer a locally available pdfium (no network). Download is last resort only.
    if let Ok(pdfium) = try_load_pdfium_local(app) {
        return plan_with_pdfium(pdfium, path, locale, password);
    }

    // Encrypted PDFs cannot be unlocked by raw stream scraping — ciphertext may
    // still contain accidental JPEG markers and would open as broken pages.
    if encrypted {
        // Prompt for a password before any network download so the UI responds
        // immediately; retry with the password then attempts pdfium download.
        if password.is_none() {
            return Err(ERR_PASSWORD_REQUIRED.to_string());
        }
        if let Ok(pdfium) = try_load_pdfium_with_download(app) {
            return plan_with_pdfium(pdfium, path, locale, password);
        }
        return Err(tr(
            locale,
            &format!(
                "无法打开加密 PDF：未找到 {}。请重新安装应用，或将该文件放到程序目录后重试。",
                library_file_name()
            ),
            &format!(
                "Cannot open encrypted PDF: {} is missing. Reinstall the app, or place the library next to the executable and retry.",
                library_file_name()
            ),
        ));
    }

    let images = extract_embedded_images(path)?;
    if !images.is_empty() {
        return Ok(PdfPlan::Embedded { images });
    }

    // Text/vector PDFs need rasterization — attempt download only when needed.
    if let Ok(pdfium) = try_load_pdfium_with_download(app) {
        return plan_with_pdfium(pdfium, path, locale, password);
    }

    Err(tr(
        locale,
        &format!(
            "无法打开此 PDF：未找到 {}，且无法抽取页面图片。请重新安装应用，或将该文件放到程序目录后重试。",
            library_file_name()
        ),
        &format!(
            "Cannot open this PDF: {} is missing and no embedded page images were found. Reinstall the app, or place the library next to the executable and retry.",
            library_file_name()
        ),
    ))
}

/// Detect standard PDF password encryption via `/Encrypt` in the trailer / catalog.
fn pdf_is_encrypted(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    let len = meta.len();
    let tail_len = 256 * 1024u64;
    let start = len.saturating_sub(tail_len);
    if start > 0 && file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return false;
    }
    looks_like_encrypt_ref(&buf)
}

fn looks_like_encrypt_ref(haystack: &[u8]) -> bool {
    let mut search = 0;
    while let Some(rel) = find_bytes(&haystack[search..], b"/Encrypt") {
        let at = search + rel + b"/Encrypt".len();
        let rest = skip_ws(&haystack[at..]);
        let encrypted = rest
            .first()
            .map(|byte| byte.is_ascii_digit() || *byte == b'<')
            .unwrap_or(false);
        if encrypted {
            return true;
        }
        search = at;
    }
    false
}

fn plan_with_pdfium(
    pdfium: Pdfium,
    path: &Path,
    locale: &str,
    password: Option<&str>,
) -> Result<PdfPlan, String> {
    let document = pdfium
        .load_pdf_from_file(path, password)
        .map_err(|err| format_pdf_err(err, locale, password.is_some()))?;
    let page_count = document.pages().len() as usize;
    if page_count == 0 {
        return Err(tr(locale, "没有找到图片", "No images found"));
    }
    Ok(PdfPlan::Pdfium { page_count })
}

pub fn list_rels_for_pdfium(page_count: usize) -> Vec<String> {
    (0..page_count)
        .map(|index| format!("page-{index:05}.jpg"))
        .collect()
}

pub fn list_rels_for_embedded(images: &[EmbeddedImage]) -> Vec<String> {
    images.iter().map(|image| image.rel.clone()).collect()
}

pub fn render_pages(path: PathBuf, page_count: usize, password: Option<String>, job: ExtractJob) {
    if let Err(err) = render_pages_inner(&path, page_count, password.as_deref(), &job) {
        if job.cancelled() {
            job.abort();
        } else {
            job.fail(err);
        }
    }
}

pub fn write_embedded(images: Vec<EmbeddedImage>, job: ExtractJob) {
    let mut files = Vec::with_capacity(images.len());
    for (index, image) in images.into_iter().enumerate() {
        if job.cancelled() {
            job.abort();
            return;
        }
        let (dest, name) = job.destination(index, &image.rel);
        if let Err(err) = fs::write(&dest, &image.bytes) {
            job.fail(err.to_string());
            return;
        }
        files.push(name);
        job.mark_written(index, dest);
    }
    job.succeed(files);
}

fn render_pages_inner(
    path: &Path,
    page_count: usize,
    password: Option<&str>,
    job: &ExtractJob,
) -> Result<(), String> {
    let pdfium = try_load_pdfium(&job.app)?;
    let document = pdfium
        .load_pdf_from_file(path, password)
        .map_err(|err| format_pdf_err(err, "en", password.is_some()))?;
    let available = document.pages().len() as usize;
    if available != page_count {
        return Err(format!(
            "PDF page count mismatch (expected {page_count}, got {available})"
        ));
    }
    let mut files = Vec::with_capacity(page_count);
    for index in 0..page_count {
        if job.cancelled() {
            job.abort();
            return Ok(());
        }
        let page = document
            .pages()
            .get(index as u16)
            .map_err(|err| err.to_string())?;
        let width = page.width().value;
        let height = page.height().value;
        let scale = scale_for(width, height);
        let config = PdfRenderConfig::new()
            .set_target_width((width * scale).round() as i32)
            .set_maximum_height((height * scale).round() as i32);
        let image = page
            .render_with_config(&config)
            .map_err(|err| err.to_string())?
            .as_image();
        let original = format!("page-{index:05}.jpg");
        let (dest, name) = job.destination(index, &original);
        save_jpeg(&image, &dest)?;
        files.push(name);
        job.mark_written(index, dest);
    }
    job.succeed(files);
    Ok(())
}

fn is_password_error(err: &PdfiumError) -> bool {
    matches!(
        err,
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)
    ) || err.to_string().to_ascii_lowercase().contains("password")
}

fn format_pdf_err(err: PdfiumError, _locale: &str, had_password: bool) -> String {
    if is_password_error(&err) {
        if had_password {
            ERR_PASSWORD_INCORRECT.to_string()
        } else {
            ERR_PASSWORD_REQUIRED.to_string()
        }
    } else {
        err.to_string()
    }
}

fn scale_for(width: f32, height: f32) -> f32 {
    let long = width.max(height).max(1.0);
    let dpi_scale: f32 = 150.0 / 72.0;
    dpi_scale.min(MAX_EDGE / long)
}

fn save_jpeg(image: &image::DynamicImage, path: &Path) -> Result<(), String> {
    let rgb = image.to_rgb8();
    let mut file = fs::File::create(path).map_err(|err| err.to_string())?;
    let mut cursor = std::io::BufWriter::new(&mut file);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 92);
    rgb.write_with_encoder(encoder)
        .map_err(|err| err.to_string())?;
    cursor.flush().map_err(|err| err.to_string())?;
    Ok(())
}

fn try_load_pdfium(app: &AppHandle) -> Result<Pdfium, String> {
    try_load_pdfium_local(app).or_else(|_| try_load_pdfium_with_download(app))
}

fn try_load_pdfium_local(app: &AppHandle) -> Result<Pdfium, String> {
    let mut errors = Vec::new();
    for dir in candidate_dirs(app) {
        match bind_pdfium(&dir) {
            Ok(pdfium) => return Ok(pdfium),
            Err(err) => errors.push(format!("{} ({err})", dir.display())),
        }
    }
    Err(format!("pdfium not available locally: {}", errors.join("; ")))
}

fn try_load_pdfium_with_download(app: &AppHandle) -> Result<Pdfium, String> {
    if let Ok(cache) = pdfium_cache_dir(app) {
        let _ = ensure_pdfium_downloaded(&cache);
    }
    try_load_pdfium_local(app)
}

fn candidate_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // Prefer shipped / next-to-exe copies over a runtime download cache.
    if let Ok(resource) = app.path().resource_dir() {
        dirs.push(resource.join("pdfium"));
        dirs.push(resource);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
            dirs.push(parent.join("pdfium"));
            // `tauri build --no-bundle` / `tauri dev` may leave resources under the crate root.
            dirs.push(parent.join("../resources/pdfium"));
            dirs.push(parent.join("../../resources/pdfium"));
        }
    }
    if let Ok(dir) = pdfium_cache_dir(app) {
        dirs.push(dir);
    }
    dirs
}

fn pdfium_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::app_cache::subdir(app, crate::app_cache::DIR_PDFIUM)?
        .join(PDFIUM_VERSION.replace('/', "-"));
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn bind_pdfium(dir: &Path) -> Result<Pdfium, String> {
    let bare = dir.join(library_file_name());
    let named = PathBuf::from(Pdfium::pdfium_platform_library_name_at_path(dir));
    for candidate in [&named, &bare] {
        if candidate.exists() {
            let bindings =
                Pdfium::bind_to_library(candidate).map_err(|err| err.to_string())?;
            return Ok(Pdfium::new(bindings));
        }
        for nested in ["bin", "lib"] {
            let path = dir.join(nested).join(library_file_name());
            if path.exists() {
                let bindings = Pdfium::bind_to_library(&path).map_err(|err| err.to_string())?;
                return Ok(Pdfium::new(bindings));
            }
        }
    }
    Err("library not found".into())
}

fn ensure_pdfium_downloaded(dir: &Path) -> Result<(), String> {
    if bind_pdfium(dir).is_ok() {
        return Ok(());
    }
    download_pdfium(dir)?;
    // Flatten nested extract layout.
    let bare = dir.join(library_file_name());
    if !bare.exists() {
        for candidate in ["bin", "lib"] {
            let nested = dir.join(candidate).join(library_file_name());
            if nested.exists() {
                fs::copy(&nested, &bare).map_err(|err| err.to_string())?;
                break;
            }
        }
    }
    if bind_pdfium(dir).is_ok() {
        Ok(())
    } else {
        Err("pdfium library missing after download".into())
    }
}

fn library_file_name() -> &'static str {
    #[cfg(windows)]
    {
        "pdfium.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libpdfium.dylib"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "libpdfium.so"
    }
}

fn download_pdfium(dir: &Path) -> Result<(), String> {
    let asset = pdfium_asset_name()?;
    let official = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_VERSION}/{asset}"
    );
    let mirrors = [
        format!("https://ghfast.top/{official}"),
        format!("https://gh-proxy.com/{official}"),
        format!("https://mirror.ghproxy.com/{official}"),
        official.clone(),
    ];
    let mut last_err = String::from("no mirror tried");
    for url in mirrors {
        match download_url(&url) {
            Ok(bytes) if bytes.len() > 1_000_000 => {
                let archive_path = dir.join(asset);
                fs::write(&archive_path, &bytes).map_err(|err| err.to_string())?;
                extract_pdfium_archive(&archive_path, dir)?;
                let _ = fs::remove_file(&archive_path);
                return Ok(());
            }
            Ok(bytes) => {
                last_err = format!("download too small from {url} ({} bytes)", bytes.len());
            }
            Err(err) => last_err = format!("{url}: {err}"),
        }
    }
    Err(format!("failed to download pdfium: {last_err}"))
}

fn download_url(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(60))
        .build();
    let response = agent.get(url).call().map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    Ok(bytes)
}

fn extract_pdfium_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".zip") {
        let file = fs::File::open(archive).map_err(|err| err.to_string())?;
        let mut zip =
            zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|err| err.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|err| err.to_string())?;
            let out_path = match entry.enclosed_name() {
                Some(path) => dest.join(path),
                None => continue,
            };
            if entry.is_dir() {
                fs::create_dir_all(&out_path).map_err(|err| err.to_string())?;
                continue;
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            let mut out = fs::File::create(&out_path).map_err(|err| err.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|err| err.to_string())?;
        }
        return Ok(());
    }
    let file = fs::File::open(archive).map_err(|err| err.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest).map_err(|err| err.to_string())?;
    Ok(())
}

fn pdfium_asset_name() -> Result<&'static str, String> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        return Ok("pdfium-win-x64.tgz");
    }
    #[cfg(all(windows, target_arch = "x86"))]
    {
        return Ok("pdfium-win-x86.tgz");
    }
    #[cfg(all(windows, target_arch = "aarch64"))]
    {
        return Ok("pdfium-win-arm64.tgz");
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Ok("pdfium-mac-x64.tgz");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Ok("pdfium-mac-arm64.tgz");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Ok("pdfium-linux-x64.tgz");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Ok("pdfium-linux-arm64.tgz");
    }
    #[allow(unreachable_code)]
    Err("unsupported platform for pdfium download".into())
}

/// Extract embedded JPEG page images from comic/scan PDFs without pdfium.
///
/// Prefer `/Subtype /Image` + `/DCTDecode` streams (one page image each). Some
/// exporters concatenate a second full-bleed JPEG inside the same stream — only
/// the first JPEG (matching `/Height` when present) is kept.
fn extract_embedded_images(path: &Path) -> Result<Vec<EmbeddedImage>, String> {
    let data = fs::read(path).map_err(|err| err.to_string())?;
    let mut images = extract_dct_image_streams(&data);
    if images.is_empty() {
        // Fallback for odd PDFs without parseable Image dicts.
        images = extract_jpeg_blobs(&data);
        let max_len = images.iter().map(Vec::len).max().unwrap_or(0);
        let min_keep = (max_len / 5).max(8_192);
        images.retain(|bytes| bytes.len() >= min_keep);
    }
    if images.is_empty() {
        return Ok(Vec::new());
    }
    Ok(images
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| EmbeddedImage {
            rel: format!("page-{:05}.jpg", index + 1),
            bytes,
        })
        .collect())
}

fn extract_dct_image_streams(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut search_from = 0;
    while search_from < data.len() {
        let Some(rel) = find_bytes(&data[search_from..], b"/Subtype") else {
            break;
        };
        let subtype_at = search_from + rel;
        let after = skip_ws(&data[subtype_at + b"/Subtype".len()..]);
        if !after.starts_with(b"/Image") {
            search_from = subtype_at + b"/Subtype".len();
            continue;
        }
        let dict_start = match rfind_bytes(&data[..subtype_at], b"<<") {
            Some(pos) => pos,
            None => {
                search_from = subtype_at + b"/Subtype".len();
                continue;
            }
        };
        let dict_end = match find_bytes(&data[subtype_at..], b">>") {
            Some(rel) => subtype_at + rel + 2,
            None => {
                search_from = subtype_at + b"/Subtype".len();
                continue;
            }
        };
        let dict = &data[dict_start..dict_end];
        search_from = dict_end;
        if find_bytes(dict, b"/DCTDecode").is_none() {
            continue;
        }
        let Some(length) = dict_int(dict, b"/Length") else {
            continue;
        };
        let length = length as usize;
        let expect_h = dict_int(dict, b"/Height");
        let expect_w = dict_int(dict, b"/Width");
        let after_dict = skip_ws(&data[dict_end..]);
        let Some(stream_body) = after_dict.strip_prefix(b"stream") else {
            continue;
        };
        let stream_body = if stream_body.starts_with(b"\r\n") {
            &stream_body[2..]
        } else if stream_body.starts_with(b"\n") {
            &stream_body[1..]
        } else if stream_body.starts_with(b"\r") {
            &stream_body[1..]
        } else {
            continue;
        };
        if stream_body.len() < length {
            continue;
        }
        let payload = &stream_body[..length];
        if let Some(jpeg) = first_jpeg_in(payload, expect_w, expect_h) {
            if jpeg.len() >= 1024 {
                out.push(jpeg);
            }
        }
    }
    out
}

fn first_jpeg_in(data: &[u8], expect_w: Option<i64>, expect_h: Option<i64>) -> Option<Vec<u8>> {
    let blobs = extract_jpeg_blobs(data);
    if blobs.is_empty() {
        return None;
    }
    if expect_w.is_some() || expect_h.is_some() {
        if let Some(matched) = blobs.iter().find(|blob| {
            jpeg_dimensions(blob)
                .map(|(w, h)| {
                    expect_w.map(|ew| ew as u32 == w).unwrap_or(true)
                        && expect_h.map(|eh| eh as u32 == h).unwrap_or(true)
                })
                .unwrap_or(false)
        }) {
            return Some(matched.clone());
        }
    }
    // Exporters sometimes append a second full-bleed JPEG; the page image is first.
    Some(blobs.into_iter().next().unwrap())
}

fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xD9 {
            break;
        }
        if marker == 0x00 || marker == 0xFF || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 1;
            continue;
        }
        if i + 3 >= data.len() {
            break;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if len < 2 || i + 2 + len > data.len() {
            i += 1;
            continue;
        }
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) && len >= 7 {
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Some((width, height));
        }
        i += 2 + len;
    }
    None
}

fn dict_int(dict: &[u8], key: &[u8]) -> Option<i64> {
    let rel = find_bytes(dict, key)?;
    let rest = skip_ws(&dict[rel + key.len()..]);
    let mut end = 0;
    while end < rest.len() && rest[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        return None;
    }
    std::str::from_utf8(&rest[..end])
        .ok()?
        .parse()
        .ok()
}

fn skip_ws(data: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < data.len() && data[i].is_ascii_whitespace() {
        i += 1;
    }
    &data[i..]
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|&i| &haystack[i..i + needle.len()] == needle)
}

fn extract_jpeg_blobs(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] != 0xFF || data[i + 1] != 0xD8 {
            i += 1;
            continue;
        }
        let start = i;
        i += 2;
        let mut end = None;
        while i + 1 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            if marker == 0xD9 {
                end = Some(i + 2);
                break;
            }
            if marker == 0xD8 {
                break;
            }
            if marker == 0x00 || marker == 0xFF || (0xD0..=0xD7).contains(&marker) || marker == 0x01
            {
                i += 1;
                continue;
            }
            if i + 3 >= data.len() {
                break;
            }
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if len < 2 || i + 2 + len > data.len() {
                i += 1;
                continue;
            }
            i += 2 + len;
        }
        if let Some(end) = end {
            let blob = data[start..end].to_vec();
            if blob.len() >= 1024 {
                out.push(blob);
            }
            i = end;
        } else {
            i = start + 2;
        }
    }
    out
}
