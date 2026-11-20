//! Multi-backend text region detection (CTD / DBNet / YSGYolo / SaberYOLO).

mod aux_yolo;
mod backends;
mod geom;
mod merge;
mod postprocess;
mod refinement;
mod seg_detector;
mod smart_sort;
mod textline_merge;

use tauri::AppHandle;

use crate::neural_sr::ort_env;
use crate::translation::TranslationSettings;

use super::image_util::{decode_rgba, region};
use super::OcrRegion;

use aux_yolo::merge_aux_yolo_boxes;
use postprocess::{postprocess_blocks, sort_blocks_reading_order};
use refinement::refine_blocks_with_references;
use textline_merge::merge_textlines;

pub use geom::DetBox;
pub use merge::TextBlock;
#[allow(unused_imports)]
pub use merge::merge_nearby_blocks;

/// Saber default: drop boxes smaller than 0.05% of the page area.
const MIN_TEXT_AREA_PERCENT: f64 = 0.05;

#[allow(dead_code)]
pub fn pack_for_detector(id: &str) -> Option<&'static str> {
    match id {
        "ctd" => Some(backends::ctd::PACK),
        "dbnet" | "default" => Some(backends::default_dbnet::PACK),
        "yolo" => Some(backends::yolo::PACK),
        "saber_yolo" => Some(backends::saber_yolo::PACK),
        _ => None,
    }
}

pub fn is_available(app: &AppHandle, detector_id: &str) -> bool {
    match normalize_id(detector_id) {
        "ctd" => backends::ctd::is_available(app),
        "dbnet" => backends::default_dbnet::is_available(app),
        "yolo" => backends::yolo::is_available(app),
        "saber_yolo" => backends::saber_yolo::is_available(app),
        _ => false,
    }
}

fn normalize_id(raw: &str) -> &'static str {
    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "off" | "none" | "disabled" => "off",
        "ctd" | "comictextdetector" | "text-detector" | "detector-ctd" => "ctd",
        "dbnet" | "default" | "detector-dbnet" | "detector-default" => "dbnet",
        "yolo" | "ysg" | "ysgyolo" => "yolo",
        "saber_yolo" | "saberyolo" | "saber-yolo" => "saber_yolo",
        "" => "dbnet",
        _ => "dbnet",
    }
}

fn requires_merge(id: &str) -> bool {
    matches!(id, "ctd" | "dbnet" | "yolo")
}

fn run_primary(app: &AppHandle, bytes: &[u8], id: &str) -> Result<Vec<DetBox>, String> {
    match id {
        "ctd" => backends::ctd::detect(app, bytes),
        "dbnet" => backends::default_dbnet::detect(app, bytes),
        "yolo" => backends::yolo::detect(app, bytes),
        "saber_yolo" => backends::saber_yolo::detect(app, bytes),
        _ => Ok(Vec::new()),
    }
}

fn filter_min_area(blocks: Vec<TextBlock>) -> Vec<TextBlock> {
    let min_frac = (MIN_TEXT_AREA_PERCENT / 100.0).max(0.0);
    if min_frac <= 0.0 {
        return blocks;
    }
    blocks
        .into_iter()
        .filter(|b| b.aabb().area() >= min_frac)
        .collect()
}

/// Detect text **blocks** (Saber TextBlock): merge + refine, keep line membership
/// so OCR can recognize line-by-line then emit one overlay region per bubble.
pub fn detect_text_blocks(
    app: &AppHandle,
    bytes: &[u8],
    settings: &TranslationSettings,
) -> Result<Vec<TextBlock>, String> {
    let id = normalize_id(&settings.text_detector);
    if id == "off" {
        return Ok(Vec::new());
    }
    if !is_available(app, id) {
        return Ok(Vec::new());
    }
    ort_env::ensure_ort(app)?;

    let img = decode_rgba(bytes)?;
    let image_size = Some(img.dimensions());

    let mut lines = run_primary(app, bytes, id)?;

    let enable_aux = settings.enable_aux_yolo && id != "yolo" && id != "off";
    if enable_aux && backends::yolo::is_available(app) {
        let aux = backends::yolo::detect_with_conf(app, bytes, 0.4)?;
        lines = merge_aux_yolo_boxes(&lines, &aux, None);
    }

    // Saber `apply_saber_yolo_refinement` checks `len(raw_lines) >= 2`.
    let raw_line_count = lines.len();

    // Saber BaseTextDetector: merge → postprocess → optional SaberYOLO refine.
    let mut blocks: Vec<TextBlock> = if requires_merge(id) {
        merge_textlines(&lines)
    } else {
        lines.into_iter().map(TextBlock::from_line).collect()
    };
    // Saber default right_to_left=True, sort_method='smart'.
    blocks = postprocess_blocks(blocks, true, image_size);

    let enable_refine = settings.enable_saber_yolo_refine && id != "saber_yolo" && id != "off";
    if enable_refine
        && !blocks.is_empty()
        && raw_line_count >= 2
        && backends::saber_yolo::is_available(app)
    {
        let refs = backends::saber_yolo::detect(app, bytes)?;
        crate::app_log::info(
            "ocr",
            format!(
                "saber_yolo refine refs={} blocks_in={} raw_lines={}",
                refs.len(),
                blocks.len(),
                raw_line_count
            ),
        );
        if refs.len() >= 2 {
            let (refined, diag) = refine_blocks_with_references(&blocks, &refs, Some(0.5));
            crate::app_log::info(
                "ocr",
                format!(
                    "saber_yolo refine done refs={} in={} splits={} out={} skips={:?}",
                    diag.refs, diag.blocks_in, diag.splits, diag.blocks_out, diag.skip_reasons
                ),
            );
            blocks = refined;
            // Saber re-sorts after refine via smart_sort.
            blocks = sort_blocks_reading_order(blocks, true, image_size);
        }
    }

    Ok(filter_min_area(blocks))
}

/// OCR whole bubble AABB (Saber MangaOCR path: one crop per TextBlock).
pub fn regions_from_block_ocr<F>(blocks: &[TextBlock], mut recognize_block: F) -> Vec<OcrRegion>
where
    F: FnMut(&DetBox) -> String,
{
    let mut out = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        let b = block.aabb().clamp01();
        let text = recognize_block(&b);
        let t = text.trim();
        if t.is_empty() {
            continue;
        }
        out.push(region(format!("r{i}"), b.x, b.y, b.w, b.h, t.to_string()));
    }
    out
}

/// Block-level draft regions (AABB only). Prefer [`detect_text_blocks`] + line OCR
/// when the recognizer supports per-line crops.
#[allow(dead_code)]
pub fn detect_regions(
    app: &AppHandle,
    bytes: &[u8],
    settings: &TranslationSettings,
) -> Result<Vec<OcrRegion>, String> {
    let blocks = detect_text_blocks(app, bytes, settings)?;
    Ok(blocks
        .into_iter()
        .enumerate()
        .filter_map(|(i, block)| {
            let b = block.aabb().clamp01();
            if b.area() < MIN_TEXT_AREA_PERCENT / 100.0 {
                return None;
            }
            Some(region(format!("d{i}"), b.x, b.y, b.w, b.h, String::new()))
        })
        .collect())
}

/// OCR each line in a block; join with spaces for translation (Saber/Ballons style).
pub fn regions_from_line_ocr<F>(blocks: &[TextBlock], mut recognize_line: F) -> Vec<OcrRegion>
where
    F: FnMut(&DetBox) -> String,
{
    let mut out = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        let mut parts: Vec<String> = Vec::new();
        for line in &block.lines {
            let text = recognize_line(line);
            let t = text.trim();
            if !t.is_empty() {
                parts.push(t.to_string());
            }
        }
        if parts.is_empty() {
            continue;
        }
        let b = block.aabb().clamp01();
        crate::app_log::info(
            "ocr",
            format!(
                "block r{i} lines={} kept={} aabb={:.3}x{:.3} text={:?}",
                block.lines.len(),
                parts.len(),
                b.w,
                b.h,
                parts.join(" ")
            ),
        );
        // Space-join so Baidu/Microsoft see one sentence, not broken newline segments.
        out.push(region(
            format!("r{i}"),
            b.x,
            b.y,
            b.w,
            b.h,
            parts.join(" "),
        ));
    }
    out
}
