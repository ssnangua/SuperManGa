mod cache;
mod cbz;
mod epub;
mod extract;
mod folder;
mod measure;
mod mobi;
pub(crate) mod names;
mod pdf;
mod rar;
mod sevenz;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use cache::CacheBackend;
use cbz::ZipBook;
use extract::ExtractJob;
use mobi::MobiBook;
use names::{file_title, tr};
use sevenz::SevenBook;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDto {
    pub name: String,
    pub rel_path: String,
    pub width: u32,
    pub height: u32,
}

pub enum Backend {
    Files(Vec<PathBuf>),
    Zip(ZipBook),
    Seven(SevenBook),
    Mobi(MobiBook),
    Cache(Arc<CacheBackend>),
}

impl Backend {
    pub fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        match self {
            Backend::Files(paths) => {
                let path = paths.get(index).ok_or("page is out of range")?;
                std::fs::read(path).map_err(|err| err.to_string())
            }
            Backend::Zip(book) => book.read(index),
            Backend::Seven(book) => book.read(index),
            Backend::Mobi(book) => book.read(index),
            Backend::Cache(cache) => {
                let path = cache.wait(index)?;
                std::fs::read(path).map_err(|err| err.to_string())
            }
        }
    }

    pub fn file_stat(&self, index: usize, rel: &str) -> FileStat {
        match self {
            Backend::Files(paths) => paths
                .get(index)
                .map(|path| stat_path(path))
                .unwrap_or_default(),
            Backend::Zip(book) => zip_stat(book, index, rel),
            Backend::Cache(cache) => cache
                .wait(index)
                .ok()
                .map(|path| stat_path(&path))
                .unwrap_or_default(),
            Backend::Seven(_) | Backend::Mobi(_) => FileStat::default(),
        }
    }

    pub fn cancel(&self) {
        if let Backend::Cache(cache) = self {
            cache.cancel();
        }
    }

    pub fn page_count(&self) -> usize {
        match self {
            Backend::Files(paths) => paths.len(),
            Backend::Zip(book) => book.indices.len(),
            Backend::Seven(book) => book.names.len(),
            Backend::Mobi(book) => book.ranges.len(),
            Backend::Cache(cache) => cache.len(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileStat {
    pub path: Option<String>,
    pub bytes: Option<u64>,
    pub modified_ms: Option<i64>,
    pub modified_text: Option<String>,
}

fn join_inner(root: &Path, rel: &str) -> String {
    let inner = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
    format!("{}{}{}", root.display(), std::path::MAIN_SEPARATOR, inner)
}

fn system_time_ms(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

fn stat_path(path: &Path) -> FileStat {
    let meta = std::fs::metadata(path).ok();
    FileStat {
        path: Some(path.to_string_lossy().into_owned()),
        bytes: meta.as_ref().map(|m| m.len()),
        modified_ms: meta
            .and_then(|m| m.modified().ok())
            .and_then(system_time_ms),
        modified_text: None,
    }
}

fn zip_stat(book: &ZipBook, index: usize, rel: &str) -> FileStat {
    let Ok(file) = std::fs::File::open(&book.path) else {
        return FileStat {
            path: Some(join_inner(&book.path, rel)),
            ..FileStat::default()
        };
    };
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return FileStat {
            path: Some(join_inner(&book.path, rel)),
            ..FileStat::default()
        };
    };
    let Some(&zip_index) = book.indices.get(index) else {
        return FileStat::default();
    };
    let Ok(entry) = archive.by_index(zip_index) else {
        return FileStat {
            path: Some(join_inner(&book.path, rel)),
            ..FileStat::default()
        };
    };
    let modified_text = entry.last_modified().map(|dt| {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second()
        )
    });
    FileStat {
        path: Some(join_inner(&book.path, rel)),
        bytes: Some(entry.size()),
        modified_ms: None,
        modified_text,
    }
}

pub struct Loaded {
    pub root: PathBuf,
    pub title: String,
    pub pages: Vec<PageDto>,
    pub backend: Arc<Backend>,
    pub extracting: bool,
    pub initial_index: Option<usize>,
}

pub fn open(
    app: AppHandle,
    path: &Path,
    read_subfolder: bool,
    locale: &str,
    session_id: u64,
    password: Option<&str>,
) -> Result<Loaded, String> {
    if !path.exists() {
        return Err(tr(locale, "找不到该路径", "Path not found"));
    }
    if path.is_dir() {
        return open_folder(path, path, read_subfolder, None, locale);
    }
    let ext = names::extension_of(path);
    if names::is_image_ext(&ext) {
        let Some(parent) = path.parent() else {
            return Err(tr(locale, "找不到该路径", "Path not found"));
        };
        let target = path
            .strip_prefix(parent)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        return open_folder(parent, parent, false, Some(target), locale);
    }
    // Many "cbz"/"cbr" dumps are mislabeled; prefer file magic over extension.
    let kind = sniff_archive_kind(path);
    match ext.as_str() {
        "zip" | "cbz" => match kind {
            ArchiveKind::Rar => open_rar(app, path, locale, session_id),
            ArchiveKind::SevenZ => open_7z(app, path, locale, session_id),
            _ => open_zip(path, locale),
        },
        "rar" | "cbr" => match kind {
            ArchiveKind::Zip => open_zip(path, locale),
            ArchiveKind::SevenZ => open_7z(app, path, locale, session_id),
            _ => open_rar(app, path, locale, session_id),
        },
        "7z" => match kind {
            ArchiveKind::Zip => open_zip(path, locale),
            ArchiveKind::Rar => open_rar(app, path, locale, session_id),
            _ => open_7z(app, path, locale, session_id),
        },
        "epub" => open_epub(path, locale),
        "pdf" => open_pdf(app, path, locale, session_id, password),
        "mobi" | "azw3" | "azw" => open_mobi(app, path, locale, session_id),
        _ => Err(tr(locale, "不支持该文件", "Unsupported file")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Rar,
    SevenZ,
    Pdf,
    Unknown,
}

fn sniff_archive_kind(path: &Path) -> ArchiveKind {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return ArchiveKind::Unknown;
    };
    let mut buf = [0u8; 8];
    let Ok(n) = file.read(&mut buf) else {
        return ArchiveKind::Unknown;
    };
    if n >= 2 && buf[0] == b'P' && buf[1] == b'K' {
        return ArchiveKind::Zip;
    }
    // RAR4: Rar!\x1a\x07\x00 ; RAR5: Rar!\x1a\x07\x01
    if n >= 6 && &buf[..6] == b"Rar!\x1a\x07" {
        return ArchiveKind::Rar;
    }
    if n >= 6 && &buf[..6] == b"7z\xbc\xaf\x27\x1c" {
        return ArchiveKind::SevenZ;
    }
    if n >= 5 && &buf[..5] == b"%PDF-" {
        return ArchiveKind::Pdf;
    }
    ArchiveKind::Unknown
}

fn pages_of(rels: &[String], sizes: &[(u32, u32)]) -> Vec<PageDto> {
    rels.iter()
        .enumerate()
        .map(|(index, rel)| {
            let (width, height) = sizes.get(index).copied().unwrap_or((0, 0));
            PageDto {
                name: file_title(rel),
                rel_path: rel.clone(),
                width,
                height,
            }
        })
        .collect()
}

fn no_images(locale: &str) -> String {
    tr(locale, "没有找到图片", "No images found")
}

fn open_folder(
    root: &Path,
    title_path: &Path,
    recursive: bool,
    focus: Option<String>,
    locale: &str,
) -> Result<Loaded, String> {
    let total = Instant::now();
    let list_started = Instant::now();
    let found = folder::collect(root, recursive)?;
    let list_ms = list_started.elapsed().as_secs_f64() * 1000.0;
    if found.is_empty() {
        return Err(no_images(locale));
    }
    let initial_index = focus.as_ref().and_then(|target| {
        found.iter().position(|(rel, _)| rel == target)
    });
    if focus.is_some() && initial_index.is_none() {
        return Err(no_images(locale));
    }
    let count = found.len();
    let rels: Vec<String> = found.iter().map(|(rel, _)| rel.clone()).collect();
    let pages = pages_of(&rels, &[]);
    let files = found.into_iter().map(|(_, path)| path).collect();
    crate::app_log::info(
        "archive",
        format!(
            "[open] folder pages={count} list={list_ms:.1}ms measure=deferred total={:.1}ms path={}",
            total.elapsed().as_secs_f64() * 1000.0,
            root.display()
        ),
    );
    Ok(Loaded {
        root: root.to_path_buf(),
        title: names::title_of(title_path),
        pages,
        backend: Arc::new(Backend::Files(files)),
        extracting: false,
        initial_index,
    })
}

fn open_zip(path: &Path, locale: &str) -> Result<Loaded, String> {
    let total = Instant::now();
    let list_started = Instant::now();
    let (rels, indices) = cbz::list(path)?;
    let list_ms = list_started.elapsed().as_secs_f64() * 1000.0;
    if rels.is_empty() {
        return Err(no_images(locale));
    }
    let count = rels.len();
    crate::app_log::info(
        "archive",
        format!(
            "[open] zip pages={count} list={list_ms:.1}ms measure=deferred total={:.1}ms path={}",
            total.elapsed().as_secs_f64() * 1000.0,
            path.display()
        ),
    );
    Ok(Loaded {
        root: path.to_path_buf(),
        title: names::title_of(path),
        pages: pages_of(&rels, &[]),
        backend: Arc::new(Backend::Zip(ZipBook {
            path: path.to_path_buf(),
            indices,
        })),
        extracting: false,
        initial_index: None,
    })
}

fn open_epub(path: &Path, locale: &str) -> Result<Loaded, String> {
    let total = Instant::now();
    let list_started = Instant::now();
    let (rels, book) = epub::open_book(path)?;
    let list_ms = list_started.elapsed().as_secs_f64() * 1000.0;
    if rels.is_empty() {
        return Err(no_images(locale));
    }
    let count = rels.len();
    crate::app_log::info(
        "archive",
        format!(
            "[open] epub pages={count} list={list_ms:.1}ms measure=deferred total={:.1}ms path={}",
            total.elapsed().as_secs_f64() * 1000.0,
            path.display()
        ),
    );
    Ok(Loaded {
        root: path.to_path_buf(),
        title: names::title_of(path),
        pages: pages_of(&rels, &[]),
        backend: Arc::new(Backend::Zip(book)),
        extracting: false,
        initial_index: None,
    })
}

fn open_pdf(
    app: AppHandle,
    path: &Path,
    locale: &str,
    session_id: u64,
    password: Option<&str>,
) -> Result<Loaded, String> {
    let password = password
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    match pdf::plan_open(&app, path, locale, password.as_deref())? {
        pdf::PdfPlan::Pdfium { page_count } => {
            let rels = pdf::list_rels_for_pdfium(page_count);
            let archive_path = path.to_path_buf();
            start_cache(app, path, rels, locale, session_id, move |job| {
                pdf::render_pages(archive_path, page_count, password, job);
            })
        }
        pdf::PdfPlan::Embedded { images } => {
            let rels = pdf::list_rels_for_embedded(&images);
            start_cache(app, path, rels, locale, session_id, move |job| {
                pdf::write_embedded(images, job);
            })
        }
    }
}

fn open_mobi(app: AppHandle, path: &Path, locale: &str, session_id: u64) -> Result<Loaded, String> {
    let listed = mobi::list(path, locale)?;
    if listed.can_random_read {
        return Ok(Loaded {
            root: path.to_path_buf(),
            title: names::title_of(path),
            pages: pages_of(&listed.rels, &[]),
            backend: Arc::new(Backend::Mobi(listed.book)),
            extracting: false,
            initial_index: None,
        });
    }
    let archive_path = path.to_path_buf();
    let book = listed.book;
    let rels = listed.rels;
    start_cache(app, path, rels, locale, session_id, move |job| {
        mobi::extract(archive_path, book, job);
    })
}

fn cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    crate::app_cache::subdir(app, crate::app_cache::DIR_COMIC)
}

fn start_cache(
    app: AppHandle,
    path: &Path,
    rels: Vec<String>,
    locale: &str,
    session_id: u64,
    spawn: impl FnOnce(ExtractJob) + Send + 'static,
) -> Result<Loaded, String> {
    if rels.is_empty() {
        return Err(no_images(locale));
    }
    let total = Instant::now();
    let root = cache_root(&app)?;
    let key = cache::cache_key(path);
    let count = rels.len();
    if let Some(files) = cache::try_reuse(&root, &key, rels.len()) {
        crate::app_log::info(
            "archive",
            format!(
                "[open] archive-cache-hit pages={count} measure=deferred total={:.1}ms path={}",
                total.elapsed().as_secs_f64() * 1000.0,
                path.display()
            ),
        );
        return Ok(Loaded {
            root: path.to_path_buf(),
            title: names::title_of(path),
            pages: pages_of(&rels, &[]),
            backend: Arc::new(Backend::Files(files)),
            extracting: false,
            initial_index: None,
        });
    }
    let cache = Arc::new(CacheBackend::new(rels.len()));
    let job = ExtractJob::new(cache.clone(), app, session_id, rels.len(), &root, &key)?;
    let owner = crate::app_log::current_owner();
    std::thread::spawn(move || {
        if let Some(owner) = owner {
            crate::app_log::scope(owner, || spawn(job));
        } else {
            spawn(job);
        }
    });
    crate::app_log::info(
        "archive",
        format!(
            "[open] archive-extract-start pages={count} total={:.1}ms path={}",
            total.elapsed().as_secs_f64() * 1000.0,
            path.display()
        ),
    );
    Ok(Loaded {
        root: path.to_path_buf(),
        title: names::title_of(path),
        pages: pages_of(&rels, &[]),
        backend: Arc::new(Backend::Cache(cache)),
        extracting: true,
        initial_index: None,
    })
}

fn open_7z(app: AppHandle, path: &Path, locale: &str, session_id: u64) -> Result<Loaded, String> {
    let (solid, rels, originals) = sevenz::list(path)?;
    if !solid {
        if rels.is_empty() {
            return Err(no_images(locale));
        }
        return Ok(Loaded {
            root: path.to_path_buf(),
            title: names::title_of(path),
            pages: pages_of(&rels, &[]),
            backend: Arc::new(Backend::Seven(SevenBook {
                path: path.to_path_buf(),
                names: originals,
            })),
            extracting: false,
            initial_index: None,
        });
    }
    let archive_path = path.to_path_buf();
    start_cache(app, path, rels, locale, session_id, move |job| {
        sevenz::extract(archive_path, originals, job);
    })
}

fn open_rar(app: AppHandle, path: &Path, locale: &str, session_id: u64) -> Result<Loaded, String> {
    let rels = rar::list(path)?;
    let archive_path = path.to_path_buf();
    let names = rels.clone();
    start_cache(app, path, rels, locale, session_id, move |job| {
        rar::extract(archive_path, names, job);
    })
}

pub fn start_measure(app: AppHandle, session_id: u64) -> Result<(), String> {
    let state = app.state::<crate::state::AppState>();
    let previous = state.measuring.swap(session_id, std::sync::atomic::Ordering::SeqCst);
    if previous == session_id {
        return Ok(());
    }
    let backend = {
        let guard = crate::state::lock(&state.session);
        let session = guard.as_ref().ok_or_else(|| "no comic".to_string())?;
        if session.id != session_id {
            state
                .measuring
                .compare_exchange(
                    session_id,
                    previous,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .ok();
            return Err("stale comic".to_string());
        }
        session.backend.clone()
    };
    let total = backend.page_count();
    let probe_app = app.clone();
    let owner = crate::app_log::current_owner();
    std::thread::spawn(move || {
        let run = || {
        let started = Instant::now();
        let alive = || {
            let state = probe_app.state::<crate::state::AppState>();
            let guard = crate::state::lock(&state.session);
            guard
                .as_ref()
                .map(|session| session.id == session_id)
                .unwrap_or(false)
                && state.measuring.load(std::sync::atomic::Ordering::SeqCst) == session_id
        };
        let mut ticker = measure::ProgressTicker::new(probe_app.clone(), session_id, total);
        let pages = match &*backend {
            Backend::Zip(book) => {
                book.probe_all(session_id, &alive, &mut |done| ticker.update(done))
            }
            Backend::Files(paths) => {
                let mut pages = Vec::with_capacity(paths.len());
                for (index, path) in paths.iter().enumerate() {
                    if !alive() {
                        break;
                    }
                    if let Some(size) = measure::file(path) {
                        pages.push(measure::metric(session_id, index, size, None));
                    }
                    ticker.update(index + 1);
                }
                pages
            }
            Backend::Seven(book) => sevenz::probe(
                book.path.clone(),
                book.names.clone(),
                session_id,
                &alive,
                &mut |done| ticker.update(done),
            ),
            Backend::Cache(cache) => {
                let mut pages = Vec::with_capacity(total);
                for index in 0..total {
                    if !alive() {
                        break;
                    }
                    if let Ok(path) = cache.wait(index) {
                        if let Some(size) = measure::file(&path) {
                            pages.push(measure::metric(session_id, index, size, None));
                        }
                    }
                    ticker.update(index + 1);
                }
                pages
            }
            Backend::Mobi(book) => {
                let mut pages = Vec::with_capacity(total);
                for index in 0..total {
                    if !alive() {
                        break;
                    }
                    if let Ok(bytes) = book.read(index) {
                        if let Some(size) = measure::reader(std::io::Cursor::new(bytes)) {
                            pages.push(measure::metric(session_id, index, size, None));
                        }
                    }
                    ticker.update(index + 1);
                }
                pages
            }
        };
        if alive() {
            measure::emit_batch(&probe_app, session_id, pages);
            ticker.finish();
            crate::app_log::info(
                "archive",
                format!(
                    "[measure] session={session_id} pages={total} elapsed={:.1}ms",
                    started.elapsed().as_secs_f64() * 1000.0
                ),
            );
        }
        let state = probe_app.state::<crate::state::AppState>();
        let _ = state.measuring.compare_exchange(
            session_id,
            0,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        );
        };
        if let Some(owner) = owner {
            crate::app_log::scope(owner, run);
        } else {
            run();
        }
    });
    Ok(())
}
