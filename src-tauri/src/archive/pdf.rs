//! PDF support:
//! - Path B1 (rasterize on): local/downloaded pdfium full-page rasterization.
//! - Path B2 (rasterize off + pdfium): decrypt if needed, export embedded image
//!   *files* only (jpg/png/gif/webp). No page rasterization fallback.
//! - Path A (rasterize off, unencrypted, no pdfium): lopdf page-order extract,
//!   then byte-scrape if the page tree yields nothing.
//! Rasterize and encryption both require pdfium (download attempted when missing).
//! Runtime download from GitHub is best-effort only (often blocked on some networks).

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use pdfium_render::prelude::*;
use tauri::{AppHandle, Manager};

use super::extract::ExtractJob;
use super::names::tr;

const MAX_EDGE: f32 = 4096.0;
const PDFIUM_VERSION: &str = "chromium/8152";
const MIN_IMAGE_BYTES: usize = 32;
/// Cap decoded Path A image streams (decompression-bomb guard).
const MAX_PATH_A_DECODE: usize = 64 * 1024 * 1024;
const MAX_FORM_DEPTH: usize = 32;

pub enum PdfPlan {
    /// B1: full-book pdfium rasterize (MediaBox sizes up front; ExtractJob prefers current page).
    Pdfium {
        page_count: usize,
        sizes: Vec<(u32, u32)>,
    },
    /// B2: export image XObject file bytes via pdfium (rels listed; bytes written in job).
    PdfiumImages { rels: Vec<String> },
    /// Path A: embedded image files in page order (no pdfium).
    Embedded { images: Vec<EmbeddedImage> },
}

pub struct EmbeddedImage {
    pub rel: String,
    pub bytes: Vec<u8>,
}

pub use super::{ERR_PASSWORD_INCORRECT, ERR_PASSWORD_REQUIRED};

pub fn plan_open(
    app: &AppHandle,
    path: &Path,
    locale: &str,
    password: Option<&str>,
    rasterize: bool,
) -> Result<PdfPlan, String> {
    let encrypted = pdf_is_encrypted(path);
    if encrypted && password.is_none() {
        return Err(ERR_PASSWORD_REQUIRED.to_string());
    }

    // Prefer local pdfium for B1/B2 whenever present.
    if let Ok(pdfium) = try_load_pdfium_local(app) {
        return plan_with_pdfium(pdfium, path, locale, password, rasterize);
    }

    // Rasterize and encryption both hard-require pdfium — try a best-effort download.
    if rasterize || encrypted {
        if let Ok(pdfium) = try_load_pdfium_with_download(app) {
            return plan_with_pdfium(pdfium, path, locale, password, rasterize);
        }
        if encrypted {
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
        return Err(tr(
            locale,
            &format!(
                "PDF 栅格化需要 {}。请重新安装应用，或将该文件放到程序目录后重试；也可在设置中关闭「PDF 使用栅格化」。",
                library_file_name()
            ),
            &format!(
                "PDF rasterize requires {}. Reinstall the app, or place the library next to the executable and retry; or turn off Rasterize PDF in Settings.",
                library_file_name()
            ),
        ));
    }

    // Path A: rasterize off + unencrypted + no pdfium.
    let images = extract_embedded_images(path)?;
    if !images.is_empty() {
        return Ok(PdfPlan::Embedded { images });
    }

    // Path A empty: last chance download for B2 extract.
    if let Ok(pdfium) = try_load_pdfium_with_download(app) {
        return plan_with_pdfium(pdfium, path, locale, password, false);
    }

    Err(tr(
        locale,
        &format!(
            "无法打开此 PDF：未找到可导出的原始图片，且未找到 {}。请重新安装应用，或将该文件放到程序目录后重试。",
            library_file_name()
        ),
        &format!(
            "Cannot open this PDF: no extractable images found, and {} is missing. Reinstall the app, or place the library next to the executable and retry.",
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
    rasterize: bool,
) -> Result<PdfPlan, String> {
    let document = pdfium
        .load_pdf_from_file(path, password)
        .map_err(|err| format_pdf_err(err, locale, password.is_some()))?;
    if rasterize {
        let page_count = document.pages().len() as usize;
        if page_count == 0 {
            return Err(tr(locale, "没有找到图片", "No images found"));
        }
        let mut sizes = Vec::with_capacity(page_count);
        for index in 0..page_count {
            let page = document
                .pages()
                .get(index as u16)
                .map_err(|err| err.to_string())?;
            sizes.push(page_pixel_size(&page));
        }
        return Ok(PdfPlan::Pdfium { page_count, sizes });
    }

    // B2: list exportable image files only (bytes written later in the extract job).
    let rels = list_pdfium_image_rels(&document);
    if rels.is_empty() {
        return Err(tr(
            locale,
            "未找到可导出的原始图片。",
            "No extractable images found.",
        ));
    }
    Ok(PdfPlan::PdfiumImages { rels })
}

pub fn list_rels_for_pdfium(page_count: usize) -> Vec<String> {
    (0..page_count)
        .map(|index| format!("page-{index:05}.jpg"))
        .collect()
}

pub fn list_rels_for_embedded(images: &[EmbeddedImage]) -> Vec<String> {
    images.iter().map(|image| image.rel.clone()).collect()
}

fn page_pixel_size(page: &PdfPage<'_>) -> (u32, u32) {
    let width = page.width().value;
    let height = page.height().value;
    let scale = scale_for(width, height);
    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
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

pub fn extract_pdfium_images(path: PathBuf, password: Option<String>, job: ExtractJob) {
    if let Err(err) = extract_pdfium_images_inner(&path, password.as_deref(), &job) {
        if job.cancelled() {
            job.abort();
        } else {
            job.fail(err);
        }
    }
}

pub fn write_embedded(images: Vec<EmbeddedImage>, job: ExtractJob) {
    let total = images.len();
    let mut files = vec![None; total];
    let mut remaining: Vec<Option<EmbeddedImage>> = images.into_iter().map(Some).collect();
    while let Some(index) = job.next_index() {
        if job.cancelled() {
            job.abort();
            return;
        }
        let Some(image) = remaining.get_mut(index).and_then(|slot| slot.take()) else {
            job.fail(format!("embedded image {index} missing"));
            return;
        };
        job.begin_page(index);
        let (dest, name) = job.destination(index, &image.rel);
        if let Err(err) = fs::write(&dest, &image.bytes) {
            job.fail(err.to_string());
            return;
        }
        files[index] = Some(name);
        job.mark_written(index, dest);
    }
    if job.cancelled() {
        job.abort();
        return;
    }
    let files = match files.into_iter().collect::<Option<Vec<_>>>() {
        Some(files) => files,
        None => {
            job.fail("embedded image extract incomplete".into());
            return;
        }
    };
    job.succeed(files);
}

fn list_pdfium_image_rels(document: &PdfDocument<'_>) -> Vec<String> {
    let mut rels = Vec::new();
    let mut index = 0usize;
    for page in document.pages().iter() {
        visit_page_objects(page.objects().iter(), &mut |img| {
            if let Some(ext) = image_object_file_ext(img) {
                index += 1;
                rels.push(format!("img-{index:05}.{ext}"));
            }
        });
    }
    rels
}

fn extract_pdfium_images_inner(
    path: &Path,
    password: Option<&str>,
    job: &ExtractJob,
) -> Result<(), String> {
    let pdfium = try_load_pdfium(&job.app)?;
    let document = pdfium
        .load_pdf_from_file(path, password)
        .map_err(|err| format_pdf_err(err, "en", password.is_some()))?;
    let mut files = Vec::new();
    let mut index = 0usize;
    for page in document.pages().iter() {
        let mut err: Option<String> = None;
        visit_page_objects(page.objects().iter(), &mut |img| {
            if err.is_some() {
                return;
            }
            if job.cancelled() {
                return;
            }
            let Some((ext, bytes)) = image_object_file_bytes(img) else {
                return;
            };
            job.begin_page(index);
            let rel = format!("img-{:05}.{ext}", index + 1);
            let (dest, name) = job.destination(index, &rel);
            if let Err(write_err) = fs::write(&dest, &bytes) {
                err = Some(write_err.to_string());
                return;
            }
            files.push(name);
            job.mark_written(index, dest);
            index += 1;
        });
        if job.cancelled() {
            job.abort();
            return Ok(());
        }
        if let Some(write_err) = err {
            return Err(write_err);
        }
    }
    if files.is_empty() {
        return Err("No extractable images found.".into());
    }
    job.succeed(files);
    Ok(())
}

fn visit_page_objects<'a>(
    objects: impl Iterator<Item = PdfPageObject<'a>>,
    visit: &mut dyn FnMut(&PdfPageImageObject<'_>),
) {
    for object in objects {
        if let Some(image) = object.as_image_object() {
            visit(image);
        } else if let Some(form) = object.as_x_object_form_object() {
            visit_page_objects(form.iter(), visit);
        }
    }
}

fn image_object_file_ext(img: &PdfPageImageObject<'_>) -> Option<&'static str> {
    image_object_file_bytes(img).map(|(ext, _)| ext)
}

fn image_object_file_bytes(img: &PdfPageImageObject<'_>) -> Option<(&'static str, Vec<u8>)> {
    let raw = img.get_raw_image_data().ok()?;
    if raw.len() < MIN_IMAGE_BYTES {
        return None;
    }
    sniff_image_ext(&raw).map(|ext| (ext, raw))
}

fn sniff_image_ext(data: &[u8]) -> Option<&'static str> {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some("jpg");
    }
    if data.len() >= 8
        && data[0] == 0x89
        && data[1] == b'P'
        && data[2] == b'N'
        && data[3] == b'G'
        && data[4] == b'\r'
        && data[5] == b'\n'
        && data[6] == 0x1a
        && data[7] == b'\n'
    {
        return Some("png");
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
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
    let mut files = vec![None; page_count];
    while let Some(index) = job.next_index() {
        if job.cancelled() {
            job.abort();
            return Ok(());
        }
        job.begin_page(index);
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
        files[index] = Some(name);
        job.mark_written(index, dest);
    }
    if job.cancelled() {
        job.abort();
        return Ok(());
    }
    let files = files
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            name.ok_or_else(|| format!("PDF page {index} was not rendered"))
        })
        .collect::<Result<Vec<_>, _>>()?;
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

/// Path A: extract embedded image *files* (jpg/png/gif/webp) without pdfium.
///
/// Prefer lopdf page-tree order (and content-stream `Do` order within a page).
/// Fall back to raw byte scrape when the page tree yields nothing.
fn extract_embedded_images(path: &Path) -> Result<Vec<EmbeddedImage>, String> {
    if let Ok(images) = extract_embedded_images_lopdf(path) {
        if !images.is_empty() {
            return Ok(images);
        }
    }
    extract_embedded_images_bytescrape(path)
}

fn extract_embedded_images_lopdf(path: &Path) -> Result<Vec<EmbeddedImage>, String> {
    let doc = Document::load(path).map_err(|err| err.to_string())?;
    let mut files: Vec<(&'static str, Vec<u8>)> = Vec::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    for (_num, page_id) in doc.get_pages() {
        collect_page_file_images(&doc, page_id, &mut files, &mut seen);
    }
    Ok(files
        .into_iter()
        .enumerate()
        .map(|(index, (ext, bytes))| EmbeddedImage {
            rel: format!("page-{:05}.{ext}", index + 1),
            bytes,
        })
        .collect())
}

fn collect_page_file_images(
    doc: &Document,
    page_id: ObjectId,
    out: &mut Vec<(&'static str, Vec<u8>)>,
    seen: &mut HashSet<ObjectId>,
) {
    let Some(resources) = page_resources(doc, page_id) else {
        return;
    };
    let mut emitted = false;
    // Prefer paint order from the page content stream (`/Name Do`).
    let content = doc.get_page_content(page_id);
    if let Ok(parsed) = Content::decode(&content) {
        for operation in parsed.operations {
            if operation.operator != "Do" {
                continue;
            }
            let Some(Object::Name(name)) = operation.operands.first() else {
                continue;
            };
            if push_named_xobject_images(doc, resources, name, out, seen, 0) {
                emitted = true;
            }
        }
    }
    // No usable Do ops (or only non-file images): still collect Image XObjects.
    if !emitted {
        visit_resource_images(doc, resources, out, seen, 0);
    }
}

fn page_resources<'a>(doc: &'a Document, page_id: ObjectId) -> Option<&'a Dictionary> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    let mut guard = HashSet::new();
    guard.insert(page_id);
    loop {
        if let Ok(resources) = doc.get_dict_in_dict(node, b"Resources") {
            return Some(resources);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        if !guard.insert(parent) {
            return None;
        }
        node = doc.get_dictionary(parent).ok()?;
    }
}

fn push_named_xobject_images(
    doc: &Document,
    resources: &Dictionary,
    name: &[u8],
    out: &mut Vec<(&'static str, Vec<u8>)>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) -> bool {
    if depth > MAX_FORM_DEPTH {
        return false;
    }
    let Ok(xobjects) = doc.get_dict_in_dict(resources, b"XObject") else {
        return false;
    };
    let Ok(entry) = xobjects.get(name) else {
        return false;
    };
    let Ok(id) = entry.as_reference() else {
        return false;
    };
    push_xobject_id(doc, id, out, seen, depth)
}

fn visit_resource_images(
    doc: &Document,
    resources: &Dictionary,
    out: &mut Vec<(&'static str, Vec<u8>)>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) {
    if depth > MAX_FORM_DEPTH {
        return;
    }
    let Ok(xobjects) = doc.get_dict_in_dict(resources, b"XObject") else {
        return;
    };
    for (_, value) in xobjects.iter() {
        let Ok(id) = value.as_reference() else {
            continue;
        };
        let _ = push_xobject_id(doc, id, out, seen, depth);
    }
}

fn push_xobject_id(
    doc: &Document,
    id: ObjectId,
    out: &mut Vec<(&'static str, Vec<u8>)>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) -> bool {
    if depth > MAX_FORM_DEPTH || !seen.insert(id) {
        return false;
    }
    let Ok(object) = doc.get_object(id) else {
        return false;
    };
    let Ok(stream) = object.as_stream() else {
        return false;
    };
    let subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|obj| obj.as_name().ok());
    match subtype {
        Some(b"Image") => {
            if let Some(file) = stream_image_file_bytes(stream) {
                out.push(file);
                true
            } else {
                false
            }
        }
        Some(b"Form") => {
            let before = out.len();
            if let Ok(form_res) = doc.get_dict_in_dict(&stream.dict, b"Resources") {
                visit_resource_images(doc, form_res, out, seen, depth + 1);
            }
            out.len() > before
        }
        _ => false,
    }
}

fn stream_image_file_bytes(stream: &Stream) -> Option<(&'static str, Vec<u8>)> {
    let expect_w = stream.dict.get(b"Width").ok().and_then(|o| o.as_i64().ok());
    let expect_h = stream.dict.get(b"Height").ok().and_then(|o| o.as_i64().ok());
    // DCTDecode payloads are already JPEG file bytes — sniff raw first.
    if let Some(ext) = sniff_image_ext(&stream.content) {
        if stream.content.len() >= MIN_IMAGE_BYTES {
            return Some((ext, stream.content.clone()));
        }
    }
    if let Some(jpeg) = first_jpeg_in(&stream.content, expect_w, expect_h) {
        if jpeg.len() >= MIN_IMAGE_BYTES {
            return Some(("jpg", jpeg));
        }
    }
    let decoded = stream
        .decompressed_content_with_limit(MAX_PATH_A_DECODE)
        .ok()?;
    if let Some(ext) = sniff_image_ext(&decoded) {
        if decoded.len() >= MIN_IMAGE_BYTES {
            return Some((ext, decoded));
        }
    }
    first_jpeg_in(&decoded, expect_w, expect_h)
        .filter(|jpeg| jpeg.len() >= MIN_IMAGE_BYTES)
        .map(|jpeg| ("jpg", jpeg))
}

fn extract_embedded_images_bytescrape(path: &Path) -> Result<Vec<EmbeddedImage>, String> {
    let data = fs::read(path).map_err(|err| err.to_string())?;
    let mut images = extract_image_file_streams(&data);
    if images.is_empty() {
        // Fallback for odd PDFs without parseable Image dicts (JPEG only).
        let blobs = extract_jpeg_blobs(&data);
        let max_len = blobs.iter().map(Vec::len).max().unwrap_or(0);
        let min_keep = (max_len / 5).max(8_192);
        images = blobs
            .into_iter()
            .filter(|bytes| bytes.len() >= min_keep)
            .filter_map(|bytes| sniff_image_ext(&bytes).map(|ext| (ext, bytes)))
            .collect();
    }
    if images.is_empty() {
        return Ok(Vec::new());
    }
    Ok(images
        .into_iter()
        .enumerate()
        .map(|(index, (ext, bytes))| EmbeddedImage {
            rel: format!("page-{:05}.{ext}", index + 1),
            bytes,
        })
        .collect())
}

fn extract_image_file_streams(data: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
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
        if find_bytes(dict, b"/DCTDecode").is_some() {
            if let Some(jpeg) = first_jpeg_in(payload, expect_w, expect_h) {
                if jpeg.len() >= 1024 {
                    out.push(("jpg", jpeg));
                }
            }
            continue;
        }
        if let Some(ext) = sniff_image_ext(payload) {
            if payload.len() >= 1024 {
                out.push((ext, payload.to_vec()));
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
