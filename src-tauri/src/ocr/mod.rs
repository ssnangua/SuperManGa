#[cfg(windows)]
mod win_ocr;
mod detector;
mod easyocr;
mod image_util;
mod manga_ocr;
mod mit48_ocr;
mod paddle_ocr;
mod rapid_ocr;

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, WebviewWindow};

/// Bumped by [`cancel_ocr_translate`] so in-flight page jobs exit early.
static OCR_EPOCH: AtomicU64 = AtomicU64::new(0);

pub const OCR_CANCELLED: &str = "OCR_CANCELLED";

fn ocr_epoch() -> u64 {
    OCR_EPOCH.load(Ordering::Relaxed)
}

fn ensure_ocr_active(epoch: u64) -> Result<(), String> {
    if ocr_epoch() != epoch {
        return Err(OCR_CANCELLED.into());
    }
    Ok(())
}

/// Cancel any in-flight [`ocr_translate_page`] job (page turn / mode off).
#[tauri::command]
pub fn cancel_ocr_translate(window: WebviewWindow) {
    OCR_EPOCH.fetch_add(1, Ordering::Relaxed);
    crate::app_log::info_for(window.label(), "ocr", "ocr cancel requested");
}

use crate::neural_sr::ort_env;
use crate::ocr_assets;
use crate::state::{lock, AppState};
use crate::translation::{translate_regions_batch, TranslationSettings};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRegion {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
    /// Ink / text color (#rrggbb), optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg_color: Option<String>,
    /// Bubble/paper fill for masks (#rrggbb), optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_color: Option<String>,
    /// Glyph stroke / outline (#rrggbb), from 48px color head when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    /// Saber auto direction from textline long-edge vote (`true` = vertical-rl).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPageResult {
    pub regions: Vec<OcrRegion>,
    pub cached: bool,
    #[serde(default)]
    pub engine: String,
    /// Primary text detector id (`off` / `ctd` / `default` / …).
    #[serde(default)]
    pub text_detector: String,
    #[serde(default)]
    pub aux_yolo: bool,
    #[serde(default)]
    pub saber_yolo_refine: bool,
    #[serde(default)]
    pub source_lang: String,
    #[serde(default)]
    pub target_lang: String,
    #[serde(default)]
    pub translation_provider: String,
    /// Page image path when available (file path or archive-relative hint).
    #[serde(default)]
    pub image_path: String,
    /// Stable translation error code when OCR succeeded but translate failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrTranslateArgs {
    pub session_id: u64,
    pub index: usize,
    pub source_lang: String,
    #[serde(default = "default_engine")]
    pub ocr_engine: String,
    pub settings: TranslationSettings,
    /// Skip disk OCR cache and re-run detection / recognition.
    #[serde(default)]
    pub force_refresh: bool,
}

fn default_engine() -> String {
    "windows".into()
}

fn cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    crate::app_cache::subdir(app, crate::app_cache::DIR_OCR_CACHE)
}

/// Content identity for a page image (path/name/index independent).
fn image_content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Detection + recognition only (no translation provider / target language).
fn ocr_cache_key(
    image_hash: &str,
    source_lang: &str,
    ocr_engine: &str,
    settings: &TranslationSettings,
) -> String {
    let mut hasher = DefaultHasher::new();
    image_hash.hash(&mut hasher);
    source_lang.hash(&mut hasher);
    ocr_engine.hash(&mut hasher);
    settings.text_detector.hash(&mut hasher);
    settings.enable_aux_yolo.hash(&mut hasher);
    settings.enable_saber_yolo_refine.hash(&mut hasher);
    // Bump when OCR region geometry / overlay / detector preprocess changes.
    "ocr-only-v42".hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Full page result including translations.
fn full_cache_key(
    image_hash: &str,
    source_lang: &str,
    ocr_engine: &str,
    settings: &TranslationSettings,
) -> String {
    let mut hasher = DefaultHasher::new();
    image_hash.hash(&mut hasher);
    source_lang.hash(&mut hasher);
    ocr_engine.hash(&mut hasher);
    settings.provider.hash(&mut hasher);
    settings.target_lang.hash(&mut hasher);
    settings.ai_model.hash(&mut hasher);
    settings.ai_base_url.hash(&mut hasher);
    settings.text_detector.hash(&mut hasher);
    settings.enable_aux_yolo.hash(&mut hasher);
    settings.enable_saber_yolo_refine.hash(&mut hasher);
    "ocr-regions-v42".hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn strip_translations(regions: &mut [OcrRegion]) {
    for region in regions.iter_mut() {
        region.translation = None;
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OcrOnlyCache {
    regions: Vec<OcrRegion>,
    engine: String,
    #[serde(default)]
    text_detector: String,
    #[serde(default)]
    aux_yolo: bool,
    #[serde(default)]
    saber_yolo_refine: bool,
    #[serde(default)]
    source_lang: String,
}

fn pack_for_engine(engine: &str) -> Option<&'static str> {
    match engine {
        "manga-ocr" => Some("ocr-manga-ocr"),
        "paddle-ocr" => Some("ocr-paddle"),
        "mit-48px" => Some("ocr-mit48px"),
        "easyocr" => Some("ocr-easyocr"),
        _ => None,
    }
}

fn require_models(app: &AppHandle, ocr_engine: &str) -> Result<(), String> {
    if let Some(pack) = pack_for_engine(ocr_engine) {
        if !ocr_assets::is_pack_installed(app, pack) {
            return Err(format!("OCR_MODELS_REQUIRED:{pack}"));
        }
        // Local ONNX engines need the shared ORT runtime (same as neural SR).
        ort_env::ensure_ort(app)?;
    }
    Ok(())
}

/// `(bytes, image_path)`
fn read_page_bytes(
    app: &AppHandle,
    session_id: u64,
    index: usize,
) -> Result<(Vec<u8>, String), String> {
    let state = app.state::<AppState>();
    let guard = lock(&state.session);
    let session = guard.as_ref().ok_or("no comic")?;
    if session.id != session_id {
        return Err("stale comic".into());
    }
    let rel = session
        .pages
        .get(index)
        .map(|p| p.rel_path.clone())
        .unwrap_or_default();
    let image_path = session
        .backend
        .file_stat(index, &rel)
        .path
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| rel.clone());
    let bytes = session.backend.read(index)?;
    Ok((bytes, image_path))
}

fn detector_pipeline_label(settings: &TranslationSettings) -> String {
    let primary = settings.text_detector.trim();
    let primary = if primary.is_empty() {
        "default"
    } else {
        primary
    };
    let mut parts = vec![primary.to_string()];
    if settings.enable_aux_yolo && primary != "off" && primary != "yolo" {
        parts.push("aux-yolo".into());
    }
    if settings.enable_saber_yolo_refine && primary != "off" && primary != "saber_yolo" {
        parts.push("saber-yolo-refine".into());
    }
    parts.join("+")
}

/// Saber-style: detect TextBlocks once, then engines only recognize bubbles/lines.
fn detect_blocks_once(
    app: &AppHandle,
    bytes: &[u8],
    settings: &TranslationSettings,
) -> Result<Vec<detector::TextBlock>, String> {
    let id = settings.text_detector.trim().to_lowercase();
    if id.is_empty() || id == "off" || id == "none" || id == "disabled" {
        return Ok(Vec::new());
    }
    if !detector::is_available(app, &settings.text_detector) {
        return Ok(Vec::new());
    }
    ort_env::ensure_ort(app)?;
    detector::detect_text_blocks(app, bytes, settings)
}

fn ocr_page_bytes(
    app: &AppHandle,
    bytes: &[u8],
    source_lang: &str,
    ocr_engine: &str,
    settings: &TranslationSettings,
) -> Result<(Vec<OcrRegion>, String), String> {
    let engine = ocr_engine.trim().to_lowercase();
    let blocks = detect_blocks_once(app, bytes, settings)?;
    let has_blocks = !blocks.is_empty();

    let (mut regions, label) = match engine.as_str() {
        "windows" => {
            #[cfg(windows)]
            {
                let (regions, label) = if has_blocks {
                    win_ocr::recognize_on_blocks(app, bytes, source_lang, &blocks)?
                } else {
                    win_ocr::recognize(bytes, source_lang)?
                };
                Ok((regions, label))
            }
            #[cfg(not(windows))]
            {
                Err("系统 OCR 仅在 Windows 上可用；请改用 Manga OCR 等本地引擎".into())
            }
        }
        "manga-ocr" => {
            let regions = if has_blocks {
                manga_ocr::recognize_on_blocks(app, bytes, &blocks)?
            } else {
                manga_ocr::recognize_fallback(app, bytes)?
            };
            Ok((
                regions,
                if has_blocks {
                    "manga-ocr+detector".into()
                } else {
                    "manga-ocr".into()
                },
            ))
        }
        "paddle-ocr" => {
            if has_blocks {
                let regions =
                    rapid_ocr::recognize_on_text_blocks(app, bytes, "paddle-ocr", &blocks)?;
                Ok((regions, "paddle-ocr+detector".into()))
            } else {
                let regions = rapid_ocr::recognize_pack(app, bytes, "paddle-ocr")?;
                Ok((regions, "paddle-ocr".into()))
            }
        }
        "mit-48px" => {
            let regions = if has_blocks {
                mit48_ocr::recognize_on_blocks(app, bytes, &blocks)?
            } else {
                mit48_ocr::recognize_fallback(app, bytes)?
            };
            Ok((
                regions,
                if has_blocks {
                    "mit-48px+detector".into()
                } else {
                    "mit-48px".into()
                },
            ))
        }
        "easyocr" => {
            if has_blocks {
                let regions = easyocr::recognize_on_blocks(app, bytes, source_lang, &blocks)?;
                Ok((regions, "easyocr+detector".into()))
            } else {
                easyocr::recognize_with_craft(app, bytes, source_lang)
            }
        }
        other => Err(format!("未知 OCR 引擎: {other}")),
    }?;

    // Saber auto text direction from textline long-edge vote (not AABB h>w).
    apply_block_directions(&blocks, &mut regions);

    // Saber-style: when mit-48px is installed, color from warped textlines
    // (even if text came from manga-ocr / easyocr / …). Skip for mit-48px itself
    // — that path already filled colors in the same forward pass.
    if engine != "mit-48px" {
        if let Ok(img) = image_util::decode_rgba(bytes) {
            mit48_ocr::apply_model_colors(app, &img, &blocks, &mut regions);
        }
    }

    Ok((regions, label))
}

fn apply_block_directions(blocks: &[detector::TextBlock], regions: &mut [OcrRegion]) {
    if blocks.is_empty() {
        for region in regions.iter_mut() {
            if region.vertical.is_none() {
                region.vertical = Some(region.h > region.w);
            }
        }
        return;
    }
    for region in regions.iter_mut() {
        let bi = parse_region_block_index(&region.id).filter(|i| *i < blocks.len());
        let vertical = if let Some(i) = bi {
            blocks[i].is_vertical()
        } else {
            // IoU match fallback.
            let mut best = None;
            let mut best_iou = 0.05f64;
            for (i, block) in blocks.iter().enumerate() {
                let b = block.aabb().clamp01();
                let iou = aabb_iou_norm(
                    region.x, region.y, region.w, region.h, b.x, b.y, b.w, b.h,
                );
                if iou > best_iou {
                    best_iou = iou;
                    best = Some(i);
                }
            }
            best.map(|i| blocks[i].is_vertical())
                .unwrap_or(region.h > region.w)
        };
        region.vertical = Some(vertical);
    }
}

fn parse_region_block_index(id: &str) -> Option<usize> {
    id.strip_prefix('r')?.parse().ok()
}

fn aabb_iou_norm(ax: f64, ay: f64, aw: f64, ah: f64, bx: f64, by: f64, bw: f64, bh: f64) -> f64 {
    let x0 = ax.max(bx);
    let y0 = ay.max(by);
    let x1 = (ax + aw).min(bx + bw);
    let y1 = (ay + ah).min(by + bh);
    let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    let uni = aw * ah + bw * bh - inter;
    if uni <= 1e-12 {
        0.0
    } else {
        inter / uni
    }
}

#[tauri::command]
pub async fn ocr_translate_page(
    window: WebviewWindow,
    app: AppHandle,
    args: OcrTranslateArgs,
) -> Result<OcrPageResult, String> {
    let owner = window.label().to_string();
    let app2 = app.clone();
    let epoch = ocr_epoch();
    tauri::async_runtime::spawn_blocking(move || {
        crate::app_log::scope(owner, || {
        ensure_ocr_active(epoch)?;
        let engine = if args.ocr_engine.trim().is_empty() {
            args.settings.ocr_engine.clone()
        } else {
            args.ocr_engine.clone()
        };
        require_models(&app2, &engine)?;
        ensure_ocr_active(epoch)?;
        let (bytes, image_path) = read_page_bytes(&app2, args.session_id, args.index)?;
        ensure_ocr_active(epoch)?;
        let image_hash = image_content_hash(&bytes);
        let root = cache_root(&app2)?;
        let full_key = full_cache_key(&image_hash, &args.source_lang, &engine, &args.settings);
        let ocr_key = ocr_cache_key(&image_hash, &args.source_lang, &engine, &args.settings);
        let full_path = root.join(format!("{full_key}.json"));
        let ocr_path = root.join(format!("{ocr_key}.ocr.json"));

        // Exact settings match (OCR + translate) → return immediately.
        if !args.force_refresh && full_path.is_file() {
            if let Ok(raw) = fs::read_to_string(&full_path) {
                if let Ok(mut cached) = serde_json::from_str::<OcrPageResult>(&raw) {
                    ensure_ocr_active(epoch)?;
                    cached.cached = true;
                    return Ok(cached);
                }
            }
        }

        // Reuse detection+OCR when only translation settings changed.
        let mut ocr_from_cache = false;
        let (mut regions, used_engine, ocr_meta) = if !args.force_refresh && ocr_path.is_file() {
            match fs::read_to_string(&ocr_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<OcrOnlyCache>(&raw).ok())
            {
                Some(mut cached) => {
                    ensure_ocr_active(epoch)?;
                    strip_translations(&mut cached.regions);
                    ocr_from_cache = true;
                    let engine_label = cached.engine.clone();
                    let meta = (
                        cached.text_detector.clone(),
                        cached.aux_yolo,
                        cached.saber_yolo_refine,
                        cached.source_lang.clone(),
                    );
                    crate::app_log::info(
                        "ocr",
                        format!(
                            "ocr cache hit regions={} image={}",
                            cached.regions.len(),
                            image_path
                        ),
                    );
                    (cached.regions, engine_label, meta)
                }
                None => {
                    ensure_ocr_active(epoch)?;
                    let (regions, used_engine) =
                        ocr_page_bytes(&app2, &bytes, &args.source_lang, &engine, &args.settings)?;
                    (
                        regions,
                        used_engine,
                        (
                            args.settings.text_detector.clone(),
                            args.settings.enable_aux_yolo,
                            args.settings.enable_saber_yolo_refine,
                            args.source_lang.clone(),
                        ),
                    )
                }
            }
        } else {
            ensure_ocr_active(epoch)?;
            let (regions, used_engine) =
                ocr_page_bytes(&app2, &bytes, &args.source_lang, &engine, &args.settings)?;
            (
                regions,
                used_engine,
                (
                    args.settings.text_detector.clone(),
                    args.settings.enable_aux_yolo,
                    args.settings.enable_saber_yolo_refine,
                    args.source_lang.clone(),
                ),
            )
        };

        // Persist OCR-only (no translations) for later provider/target switches.
        if !ocr_from_cache {
            let mut ocr_regions = regions.clone();
            strip_translations(&mut ocr_regions);
            let ocr_cache = OcrOnlyCache {
                regions: ocr_regions,
                engine: used_engine.clone(),
                text_detector: ocr_meta.0.clone(),
                aux_yolo: ocr_meta.1,
                saber_yolo_refine: ocr_meta.2,
                source_lang: if ocr_meta.3.is_empty() {
                    args.source_lang.clone()
                } else {
                    ocr_meta.3.clone()
                },
            };
            if let Ok(raw) = serde_json::to_string(&ocr_cache) {
                let _ = fs::write(&ocr_path, raw);
            }
        }

        ensure_ocr_active(epoch)?;
        let mut translation_error: Option<String> = None;
        if !regions.is_empty() {
            strip_translations(&mut regions);
            let lines: Vec<(String, String)> = regions
                .iter()
                .map(|r| (r.id.clone(), r.text.clone()))
                .collect();
            let cancel = || ensure_ocr_active(epoch);
            match translate_regions_batch(
                &lines,
                &args.settings,
                Some(args.source_lang.as_str()),
                Some(&cancel),
            ) {
                Ok(translations) => {
                    for (region, translation) in regions.iter_mut().zip(translations) {
                        if !translation.trim().is_empty() {
                            region.translation = Some(translation);
                        }
                    }
                }
                Err(err) if err == OCR_CANCELLED => return Err(err),
                Err(err) => {
                    crate::app_log::error("ocr", format!("ocr translate failed: {err}"));
                    translation_error = Some(err);
                }
            }
        }

        ensure_ocr_active(epoch)?;
        let result = OcrPageResult {
            regions,
            cached: ocr_from_cache,
            engine: used_engine,
            text_detector: args.settings.text_detector.clone(),
            aux_yolo: args.settings.enable_aux_yolo,
            saber_yolo_refine: args.settings.enable_saber_yolo_refine,
            source_lang: args.source_lang.clone(),
            target_lang: args.settings.target_lang.clone(),
            translation_provider: args.settings.provider.clone(),
            image_path,
            translation_error: translation_error.clone(),
        };
        crate::app_log::info(
            "ocr",
            format!(
                "ocr done engine={} detector={} regions={} ocrCached={} image={}",
                result.engine,
                detector_pipeline_label(&args.settings),
                result.regions.len(),
                ocr_from_cache,
                result.image_path
            ),
        );
        // Skip full cache when translate failed so the next run can retry translation
        // (OCR-only cache above still lets us skip detection/recognition).
        if translation_error.is_none() {
            if let Ok(raw) = serde_json::to_string(&result) {
                let _ = fs::write(full_path, raw);
            }
        }
        Ok(result)
        })
    })
    .await
    .map_err(|err| err.to_string())?
}
