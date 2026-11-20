//! EasyOCR g2 ONNX: CRAFT detection + CRNN recognition (zh_sim / ja / ko / latin).

use image::RgbaImage;
use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr_assets;
use super::detector::{self, TextBlock};
use super::image_util::{
    apply_sampled_colors, crop_norm, decode_rgba, nchw_rgb_norm, region, to_rgb_resized,
};
use super::OcrRegion;

const PACK: &str = "ocr-easyocr";
const CRAFT_FILE: &str = "craft.onnx";
const REC_H: u32 = 64;
const TEXT_THRESHOLD: f32 = 0.7;
const LINK_THRESHOLD: f32 = 0.4;
const LOW_TEXT: f32 = 0.4;
const MIN_AREA: usize = 10;

#[derive(Clone, Copy)]
enum RecScript {
    ZhSim,
    Japanese,
    Korean,
    Latin,
}

impl RecScript {
    fn from_source_lang(source_lang: &str) -> Self {
        let s = source_lang.trim().to_lowercase();
        if s.is_empty() || s == "auto" {
            // Manga-first default.
            return Self::Japanese;
        }
        let base = s.split(['-', '_']).next().unwrap_or(s.as_str());
        match base {
            "ja" | "jp" | "jpn" => Self::Japanese,
            "ko" | "kr" | "kor" => Self::Korean,
            "zh" | "cn" | "chi" => Self::ZhSim,
            _ => Self::Latin,
        }
    }

    fn rec_file(self) -> &'static str {
        match self {
            Self::ZhSim => "rec_zh_sim.onnx",
            Self::Japanese => "rec_japanese.onnx",
            Self::Korean => "rec_korean.onnx",
            Self::Latin => "rec_latin.onnx",
        }
    }

    fn keys(self) -> Vec<String> {
        let raw = match self {
            Self::ZhSim => include_str!("easyocr_dict/keys_zh_sim.txt"),
            Self::Japanese => include_str!("easyocr_dict/keys_japanese.txt"),
            Self::Korean => include_str!("easyocr_dict/keys_korean.txt"),
            Self::Latin => include_str!("easyocr_dict/keys_latin.txt"),
        };
        // CTC blank at index 0 (EasyOCR CTCLabelConverter).
        let mut keys = Vec::with_capacity(raw.lines().count() + 1);
        keys.push("blank".into());
        for line in raw.lines() {
            let t = line.trim_end_matches('\r');
            // Preserve single-space character lines.
            keys.push(t.to_string());
        }
        keys
    }
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

/// CRAFT-style canvas: long side capped, pad to 32, keep scale for box mapping.
fn craft_input(img: &RgbaImage, limit_side: u32) -> (RgbaImage, f32, u32, u32) {
    let (w, h) = img.dimensions();
    let max_side = w.max(h) as f32;
    let scale = if max_side > limit_side as f32 {
        limit_side as f32 / max_side
    } else {
        1.0
    };
    let nw = ((w as f32 * scale) as u32).max(1);
    let nh = ((h as f32 * scale) as u32).max(1);
    let pw = ((nw + 31) / 32) * 32;
    let ph = ((nh + 31) / 32) * 32;
    let resized = to_rgb_resized(img, nw, nh);
    let mut canvas = RgbaImage::from_pixel(pw, ph, image::Rgba([0, 0, 0, 255]));
    for y in 0..nh {
        for x in 0..nw {
            canvas.put_pixel(x, y, *resized.get_pixel(x, y));
        }
    }
    (canvas, scale, w, h)
}

/// Axis-aligned boxes from CRAFT region/affinity maps (map coords → original image norm).
fn craft_boxes(
    region_map: &[f32],
    affinity_map: &[f32],
    mh: usize,
    mw: usize,
    input_w: u32,
    input_h: u32,
    scale: f32,
    orig_w: u32,
    orig_h: u32,
) -> Vec<(f64, f64, f64, f64)> {
    let mut mask: Vec<bool> = (0..mh * mw)
        .map(|i| region_map[i] >= LOW_TEXT || affinity_map[i] >= LINK_THRESHOLD)
        .collect();
    let mut boxes = Vec::new();
    let mut stack = Vec::new();

    for start in 0..mh * mw {
        if !mask[start] {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (mw, mh, 0usize, 0usize);
        let (mut peak, mut area) = (0f32, 0usize);
        stack.push(start);
        mask[start] = false;
        while let Some(i) = stack.pop() {
            let x = i % mw;
            let y = i / mw;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
            peak = peak.max(region_map[i]);
            area += 1;
            if x > 0 && mask[i - 1] {
                mask[i - 1] = false;
                stack.push(i - 1);
            }
            if x + 1 < mw && mask[i + 1] {
                mask[i + 1] = false;
                stack.push(i + 1);
            }
            if y > 0 && mask[i - mw] {
                mask[i - mw] = false;
                stack.push(i - mw);
            }
            if y + 1 < mh && mask[i + mw] {
                mask[i + mw] = false;
                stack.push(i + mw);
            }
        }
        if area < MIN_AREA || peak < TEXT_THRESHOLD {
            continue;
        }
        // Map is typically 1/2 of CRAFT input; pad a little in map space.
        let pad = 2usize;
        let mx0 = x0.saturating_sub(pad) as f32;
        let my0 = y0.saturating_sub(pad) as f32;
        let mx1 = (x1 + pad).min(mw) as f32;
        let my1 = (y1 + pad).min(mh) as f32;
        let map_to_in_x = input_w as f32 / mw as f32;
        let map_to_in_y = input_h as f32 / mh as f32;
        let ix0 = mx0 * map_to_in_x / scale;
        let iy0 = my0 * map_to_in_y / scale;
        let ix1 = mx1 * map_to_in_x / scale;
        let iy1 = my1 * map_to_in_y / scale;
        let bw = (ix1 - ix0).max(1.0);
        let bh = (iy1 - iy0).max(1.0);
        if bw < 4.0 || bh < 4.0 {
            continue;
        }
        boxes.push((
            (ix0 / orig_w as f32) as f64,
            (iy0 / orig_h as f32) as f64,
            (bw / orig_w as f32) as f64,
            (bh / orig_h as f32) as f64,
        ));
    }
    boxes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    boxes
}

fn to_gray_nchw(img: &RgbaImage) -> (Vec<usize>, Vec<f32>) {
    let (w, h) = img.dimensions();
    let mut data = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            let g =
                (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])) / 255.0;
            // EasyOCR NormalizePAD: (x - 0.5) / 0.5
            data.push((g - 0.5) / 0.5);
        }
    }
    (vec![1, 1, h as usize, w as usize], data)
}

fn recognize_crop(
    app: &AppHandle,
    crop: &RgbaImage,
    rec_file: &str,
    keys: &[String],
) -> Result<String, String> {
    let rec_path = ocr_assets::pack_file(app, PACK, rec_file)?;
    let mut session = open_session(app, &rec_path)?;
    let (cw, ch) = crop.dimensions();
    let tw = ((cw as f32 / ch.max(1) as f32) * REC_H as f32)
        .round()
        .clamp(16.0, 512.0) as u32;
    let resized = to_rgb_resized(crop, tw, REC_H);
    let (shape, data) = to_gray_nchw(&resized);
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "easyocr rec has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("easyocr rec failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "easyocr rec has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape();
    let flat: Vec<f32> = array.iter().copied().collect();
    let (seq, vocab, flat) = if shape.len() == 3 {
        let a = shape[1];
        let b = shape[2];
        if b > a {
            (a, b, flat)
        } else {
            let mut out = vec![0f32; a * b];
            for t in 0..b {
                for c in 0..a {
                    out[t * a + c] = flat[c * b + t];
                }
            }
            (b, a, out)
        }
    } else if shape.len() == 2 {
        (shape[0], shape[1], flat)
    } else {
        return Err(format!("unexpected easyocr rec shape: {shape:?}"));
    };
    Ok(ctc_greedy(&flat, seq, vocab, keys))
}

/// Consume Saber TextBlocks (line crops); no CRAFT when blocks are provided.
pub fn recognize_on_blocks(
    app: &AppHandle,
    bytes: &[u8],
    source_lang: &str,
    blocks: &[TextBlock],
) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    let script = RecScript::from_source_lang(source_lang);
    let keys = script.keys();
    let rec_file = script.rec_file();
    let img = decode_rgba(bytes)?;
    let mut regions = detector::regions_from_line_ocr(blocks, |line| {
        let draft = region("tmp", line.x, line.y, line.w, line.h, String::new());
        let crop = crop_norm(&img, &draft);
        recognize_crop(app, &crop, rec_file, &keys).unwrap_or_default()
    });
    apply_sampled_colors(&img, &mut regions);
    Ok(regions)
}

/// Built-in CRAFT detection fallback when no Saber detector blocks exist.
pub fn recognize_with_craft(
    app: &AppHandle,
    bytes: &[u8],
    source_lang: &str,
) -> Result<(Vec<OcrRegion>, String), String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    let script = RecScript::from_source_lang(source_lang);
    let keys = script.keys();
    let rec_file = script.rec_file();

    let img = decode_rgba(bytes)?;

    let (det_img, scale, ow, oh) = craft_input(&img, 1280);
    let (shape, data) = nchw_rgb_norm(&det_img, [0.485, 0.456, 0.406], [0.229, 0.224, 0.225]);

    let craft_path = ocr_assets::pack_file(app, PACK, CRAFT_FILE)?;
    let mut det_session = open_session(app, &craft_path)?;
    let input_name = det_session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "easyocr craft has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = det_session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("easyocr craft failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "easyocr craft has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape().to_vec();
    let flat: Vec<f32> = array.iter().copied().collect();

    // Accept [1,H,W,2], [1,2,H,W], or [H,W,2].
    let (mh, mw, region_map, affinity_map) = match shape.as_slice() {
        [_, h, w, 2] => {
            let mut region_m = vec![0f32; h * w];
            let mut affinity = vec![0f32; h * w];
            for y in 0..*h {
                for x in 0..*w {
                    let i = y * w + x;
                    region_m[i] = flat[i * 2];
                    affinity[i] = flat[i * 2 + 1];
                }
            }
            (*h, *w, region_m, affinity)
        }
        [_, 2, h, w] => {
            let hw = h * w;
            let region_m = flat[..hw].to_vec();
            let affinity = flat[hw..hw * 2].to_vec();
            (*h, *w, region_m, affinity)
        }
        [h, w, 2] => {
            let mut region_m = vec![0f32; h * w];
            let mut affinity = vec![0f32; h * w];
            for y in 0..*h {
                for x in 0..*w {
                    let i = y * w + x;
                    region_m[i] = flat[i * 2];
                    affinity[i] = flat[i * 2 + 1];
                }
            }
            (*h, *w, region_m, affinity)
        }
        _ => return Err(format!("unexpected easyocr craft shape: {shape:?}")),
    };

    let boxes = craft_boxes(
        &region_map,
        &affinity_map,
        mh,
        mw,
        det_img.width(),
        det_img.height(),
        scale,
        ow,
        oh,
    );

    let mut regions = Vec::new();
    for (i, (x, y, w, h)) in boxes.into_iter().take(96).enumerate() {
        let draft = region(format!("r{i}"), x, y, w, h, String::new());
        let crop = crop_norm(&img, &draft);
        let text = recognize_crop(app, &crop, rec_file, &keys).unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        regions.push(region(format!("r{i}"), x, y, w, h, text));
    }

    if regions.is_empty() {
        let text = recognize_crop(app, &img, rec_file, &keys)?;
        if !text.trim().is_empty() {
            regions.push(region("r0", 0.05, 0.05, 0.9, 0.2, text));
        }
    }

    Ok((regions, "easyocr".into()))
}
