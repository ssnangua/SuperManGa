//! Shared RapidOCR-style DB detection + CTC recognition (PaddleOCR pack).

use image::{DynamicImage, RgbaImage};
use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr_assets;

use super::image_util::{
    crop_norm, crop_norm_padded, decode_rgba, nchw_rgb_norm, paddle_det_input, region,
    to_rgb_resized,
};
use super::OcrRegion;

fn load_keys(path: &std::path::Path) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut keys = Vec::new();
    // CTC blank at index 0 (PP-OCR / RapidOCR convention).
    keys.push("blank".into());
    for line in raw.lines() {
        let t = line.trim_end_matches('\r');
        // Keep single-space lines; they are a real class in PP-OCRv6.
        if t.is_empty() {
            continue;
        }
        keys.push(t.to_string());
    }
    // PP-OCRv6 rec often reserves a final space class after the dict.
    if !keys.iter().any(|k| k == " ") {
        keys.push(" ".into());
    }
    Ok(keys)
}

fn ctc_greedy(logits: &[f32], seq_len: usize, vocab: usize, keys: &[String]) -> String {
    let mut last = usize::MAX;
    let mut out = String::new();
    for t in 0..seq_len {
        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for c in 0..vocab {
            let v = logits[t * vocab + c];
            if v > best_v {
                best_v = v;
                best_i = c;
            }
        }
        if best_i != 0 && best_i != last {
            if let Some(ch) = keys.get(best_i) {
                if ch != "blank" {
                    out.push_str(ch);
                }
            }
        }
        last = best_i;
    }
    out
}

fn db_boxes_from_map(
    map: &[f32],
    mh: usize,
    mw: usize,
    scale_x: f32,
    scale_y: f32,
    orig_w: u32,
    orig_h: u32,
    thresh: f32,
) -> Vec<(f64, f64, f64, f64)> {
    // Simple threshold + connected components via scanline boxes.
    let mut visited = vec![false; mh * mw];
    let mut boxes = Vec::new();
    let dirs = [(0isize, 1), (1, 0), (0, -1), (-1, 0)];
    for y in 0..mh {
        for x in 0..mw {
            let i = y * mw + x;
            if visited[i] || map[i] < thresh {
                continue;
            }
            let mut stack = vec![(x, y)];
            visited[i] = true;
            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut count = 0usize;
            while let Some((cx, cy)) = stack.pop() {
                count += 1;
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);
                for (dx, dy) in dirs {
                    let nx = cx as isize + dx;
                    let ny = cy as isize + dy;
                    if nx < 0 || ny < 0 || nx >= mw as isize || ny >= mh as isize {
                        continue;
                    }
                    let ni = ny as usize * mw + nx as usize;
                    if visited[ni] || map[ni] < thresh {
                        continue;
                    }
                    visited[ni] = true;
                    stack.push((nx as usize, ny as usize));
                }
            }
            if count < 20 {
                continue;
            }
            // Map is usually 1/4 of network input; paddle_det pads to 32.
            let pad = 2usize;
            let x0 = min_x.saturating_sub(pad) as f32 / scale_x;
            let y0 = min_y.saturating_sub(pad) as f32 / scale_y;
            let x1 = ((max_x + pad).min(mw - 1)) as f32 / scale_x;
            let y1 = ((max_y + pad).min(mh - 1)) as f32 / scale_y;
            let bw = (x1 - x0).max(1.0);
            let bh = (y1 - y0).max(1.0);
            if bw < 4.0 || bh < 4.0 {
                continue;
            }
            boxes.push((
                (x0 / orig_w as f32) as f64,
                (y0 / orig_h as f32) as f64,
                (bw / orig_w as f32) as f64,
                (bh / orig_h as f32) as f64,
            ));
        }
    }
    boxes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    boxes
}

fn recognize_crop(
    app: &AppHandle,
    pack: &str,
    crop: &RgbaImage,
    keys: &[String],
) -> Result<String, String> {
    let (cw, ch) = crop.dimensions();
    // Multi-line bubble crops are often still wider than tall (e.g. 0.37×0.08);
    // any crop much taller than one Latin line must be sliced or CTC returns "I"/"RTWIU".
    let looks_multiline = ch > 72 || ((ch as f32) / (cw.max(1) as f32) > 0.22 && ch > 56);
    if looks_multiline {
        let strip_h = ((cw as f32) * 0.16).clamp(40.0, 64.0) as u32;
        let step = strip_h.saturating_sub(6).max(28);
        let mut parts = Vec::new();
        let mut y = 0u32;
        while y < ch {
            let h = strip_h.min(ch - y);
            if h < 14 {
                break;
            }
            let band = DynamicImage::ImageRgba8(crop.clone())
                .crop_imm(0, y, cw, h)
                .to_rgba8();
            let t = recognize_crop_flat(app, pack, &band, keys).unwrap_or_default();
            let t = t.trim().to_string();
            if !t.is_empty() {
                parts.push(t);
            }
            if y + h >= ch {
                break;
            }
            y = y.saturating_add(step);
        }
        if !parts.is_empty() {
            return Ok(parts.join(" "));
        }
    }
    recognize_crop_flat(app, pack, crop, keys)
}

fn recognize_crop_flat(
    app: &AppHandle,
    pack: &str,
    crop: &RgbaImage,
    keys: &[String],
) -> Result<String, String> {
    let rec_path = ocr_assets::pack_file(app, pack, "rec.onnx")?;
    let mut session = open_session(app, &rec_path)?;
    let target_h = 48u32;
    let (cw, ch) = crop.dimensions();
    // PP-OCR max_wh_ratio ~25 → allow long Latin lines (was 320, crushed bubbles).
    let tw = ((cw as f32 / ch.max(1) as f32) * target_h as f32)
        .round()
        .clamp(16.0, 1200.0) as u32;
    let resized = to_rgb_resized(crop, tw, target_h);
    let (shape, data) = nchw_rgb_norm(&resized, [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "rec model has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("rec failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "rec has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape();
    // Expect [1, T, C] or [1, C, T]
    let (seq, vocab, flat) = if shape.len() == 3 {
        let a = shape[1];
        let b = shape[2];
        let flat: Vec<f32> = array.iter().copied().collect();
        if b > a {
            (a, b, flat)
        } else {
            // transpose C,T -> T,C
            let mut out = vec![0f32; a * b];
            for t in 0..b {
                for c in 0..a {
                    out[t * a + c] = flat[c * b + t];
                }
            }
            (b, a, out)
        }
    } else {
        return Err(format!("unexpected rec output shape: {shape:?}"));
    };
    Ok(ctc_greedy(&flat, seq, vocab, keys))
}

/// Recognize pre-detected regions with the pack's CTC rec model only.
#[allow(dead_code)]
pub fn recognize_on_regions(
    app: &AppHandle,
    bytes: &[u8],
    pack: &str,
    boxes: &[OcrRegion],
) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, pack) {
        return Err(format!("OCR_MODELS_REQUIRED:{pack}"));
    }
    let img = decode_rgba(bytes)?;
    let keys_path = ocr_assets::pack_file(app, pack, "keys.txt")?;
    let keys = load_keys(&keys_path)?;
    let mut regions = Vec::new();
    for (i, draft) in boxes.iter().take(128).enumerate() {
        // Slight pad so tight DBNet/unclip boxes keep glyph edges for CTC rec.
        let crop = crop_norm_padded(&img, draft, 0.12);
        let text = recognize_crop(app, pack, &crop, &keys).unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        let mut r = region(
            format!("r{i}"),
            draft.x,
            draft.y,
            draft.w,
            draft.h,
            text,
        );
        let (fg, bg) = super::image_util::sample_region_colors(&img, &r);
        r.fg_color = fg;
        r.bg_color = bg;
        regions.push(r);
    }
    Ok(regions)
}

/// Line-wise CTC on Saber-style text blocks, then one overlay region per bubble.
pub fn recognize_on_text_blocks(
    app: &AppHandle,
    bytes: &[u8],
    pack: &str,
    blocks: &[super::detector::TextBlock],
) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, pack) {
        return Err(format!("OCR_MODELS_REQUIRED:{pack}"));
    }
    let img = decode_rgba(bytes)?;
    let keys_path = ocr_assets::pack_file(app, pack, "keys.txt")?;
    let keys = load_keys(&keys_path)?;
    let mut regions = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        let mut parts: Vec<String> = Vec::new();
        for line in &block.lines {
            let draft = region("tmp", line.x, line.y, line.w, line.h, String::new());
            // Saber: expand textline polygon by 1px, then CTC on that line only.
            let crop = super::image_util::crop_norm_expand_px(&img, &draft, 1);
            let text = recognize_crop_flat(app, pack, &crop, &keys).unwrap_or_default();
            let t = text.trim();
            if !t.is_empty() {
                parts.push(t.to_string());
            }
        }
        let b = block.aabb().clamp01();
        // Saber joins per-line CTC with spaces — never re-OCR the bubble AABB.
        let text = parts.join(" ");
        if text.trim().is_empty() {
            continue;
        }
        crate::app_log::info(
            "ocr",
            format!(
                "block r{i} det_lines={} parts={} aabb={:.3}x{:.3} text={:?}",
                block.lines.len(),
                parts.len(),
                b.w,
                b.h,
                text
            ),
        );
        let mut r = region(format!("r{i}"), b.x, b.y, b.w, b.h, text);
        let (fg, bg) = super::image_util::sample_region_colors(&img, &r);
        r.fg_color = fg;
        r.bg_color = bg;
        regions.push(r);
    }
    Ok(regions)
}

pub fn recognize_pack(app: &AppHandle, bytes: &[u8], pack: &str) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, pack) {
        return Err(format!("OCR_MODELS_REQUIRED:{pack}"));
    }
    let img = decode_rgba(bytes)?;
    let (iw, ih) = img.dimensions();
    let det_path = ocr_assets::pack_file(app, pack, "det.onnx")?;
    let keys_path = ocr_assets::pack_file(app, pack, "keys.txt")?;
    let keys = load_keys(&keys_path)?;

    let (det_img, scale_x, scale_y, ow, oh) = paddle_det_input(&img, 960);
    let (shape, data) = nchw_rgb_norm(&det_img, [0.485, 0.456, 0.406], [0.229, 0.224, 0.225]);
    let mut det_session = open_session(app, &det_path)?;
    let input_name = det_session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "det model has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = det_session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("det failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "det has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape().to_vec();
    let flat: Vec<f32> = array.iter().copied().collect();
    // [1,1,H,W] or [1,H,W]
    let (mh, mw) = match shape.as_slice() {
        [_, _, h, w] => (*h, *w),
        [_, h, w] => (*h, *w),
        [h, w] => (*h, *w),
        _ => return Err(format!("unexpected det shape: {shape:?}")),
    };
    // Probability map is relative to network input; account for resize scale.
    let map_scale_x = (det_img.width() as f32 / mw as f32) / scale_x;
    let map_scale_y = (det_img.height() as f32 / mh as f32) / scale_y;
    let boxes = db_boxes_from_map(
        &flat,
        mh,
        mw,
        map_scale_x,
        map_scale_y,
        ow,
        oh,
        0.3,
    );

    let mut regions = Vec::new();
    for (i, (x, y, w, h)) in boxes.into_iter().take(64).enumerate() {
        let draft = region(format!("r{i}"), x, y, w, h, String::new());
        let crop = crop_norm(&img, &draft);
        let text = recognize_crop(app, pack, &crop, &keys).unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        regions.push(region(format!("r{i}"), x, y, w, h, text));
    }

    if regions.is_empty() {
        // Fallback: whole-page recognition strip.
        let text = recognize_crop(app, pack, &img, &keys)?;
        if !text.trim().is_empty() {
            regions.push(region("r0", 0.05, 0.05, 0.9, 0.2, text));
        }
    }
    let _ = (iw, ih);
    Ok(regions)
}
