//! MIT 48px CTC OCR (manga-image-translator family).
//!
//! ONNX exports `logits` + `colors` (T×6 = fgRGB + bgRGB in 0..1).
//! Color aggregation follows MIT CTC averaging (Saber `color_extractor` effect).

use image::RgbaImage;
use ort::session::{Session, SessionOutputs};
use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr_assets;

use super::detector::TextBlock;
use super::image_util::{
    apply_sampled_colors, crop_line_48, decode_rgba, hex_rgb, nchw_rgb_norm, region, to_rgb_resized,
};
use super::OcrRegion;

const PACK: &str = "ocr-mit48px";

#[derive(Clone, Copy, Debug, Default)]
struct Rgb8 {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Clone, Debug, Default)]
struct CropResult {
    text: String,
    fg: Option<Rgb8>,
    /// MIT "bg" = glyph stroke / outline color.
    stroke: Option<Rgb8>,
}

fn load_alphabet(path: &std::path::Path) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    // MIT alphabets are one token per line (`<PAD>` is blank at index 0).
    let line_keys: Vec<String> = raw
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();
    if line_keys.len() >= 1000 {
        return Ok(line_keys);
    }
    let mut keys = Vec::new();
    keys.push("".into());
    for ch in raw.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        keys.push(ch.to_string());
    }
    Ok(keys)
}

fn alphabet_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let v5 = ocr_assets::pack_file(app, PACK, "alphabet-all-v5.txt");
    if v5.as_ref().is_ok_and(|p| p.is_file()) {
        return v5;
    }
    ocr_assets::pack_file(app, PACK, "alphabet-all-v7.txt")
}

fn clamp01_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn average_rgb(samples: &[(f32, f32, f32)]) -> Option<Rgb8> {
    if samples.is_empty() {
        return None;
    }
    let n = samples.len() as f32;
    let (mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32);
    for (rr, gg, bb) in samples {
        r += rr;
        g += gg;
        b += bb;
    }
    Some(Rgb8 {
        r: clamp01_u8(r / n),
        g: clamp01_u8(g / n),
        b: clamp01_u8(b / n),
    })
}

fn average_rgb8(samples: &[Rgb8]) -> Option<Rgb8> {
    if samples.is_empty() {
        return None;
    }
    let n = samples.len() as u32;
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    for s in samples {
        r += u32::from(s.r);
        g += u32::from(s.g);
        b += u32::from(s.b);
    }
    Some(Rgb8 {
        r: (r / n) as u8,
        g: (g / n) as u8,
        b: (b / n) as u8,
    })
}

fn lum(c: Rgb8) -> f32 {
    0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b)
}

fn chroma(c: Rgb8) -> u8 {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    max.saturating_sub(min)
}

/// Mean channel delta — close enough to MIT `color_difference` for grey manga ink.
fn color_diff(a: Rgb8, b: Rgb8) -> f32 {
    let dr = (i16::from(a.r) - i16::from(b.r)).unsigned_abs() as f32;
    let dg = (i16::from(a.g) - i16::from(b.g)).unsigned_abs() as f32;
    let db = (i16::from(a.b) - i16::from(b.b)).unsigned_abs() as f32;
    (dr + dg + db) / 3.0
}

fn black() -> Rgb8 {
    Rgb8 { r: 0, g: 0, b: 0 }
}

fn white() -> Rgb8 {
    Rgb8 {
        r: 255,
        g: 255,
        b: 255,
    }
}

/// Kill JPEG/scan cast, then snap to Saber-like poles (`#000` / `#FFF`).
/// Only keep vividly saturated colors (true colored SFX / UI text).
fn snap_render_color(c: Rgb8) -> Rgb8 {
    // Weak chroma (incl. greenish/yellowish greys) → luminance grey first.
    if chroma(c) < 50 {
        let y = lum(c).round().clamp(0.0, 255.0) as u8;
        if y <= 150 {
            return black();
        }
        return white();
    }
    c
}

/// MIT `TextBlock.get_font_colors` + `stroke_width`: force contrast, drop useless stroke.
/// Saber factory default is fixed `#000` + `#FFF` stroke; auto colors still need this cleanup.
fn finalize_ink_stroke(fg: Rgb8, stroke: Option<Rgb8>) -> (Rgb8, Option<Rgb8>) {
    let fg = snap_render_color(fg);
    let mut stroke = stroke
        .map(snap_render_color)
        .unwrap_or_else(|| if lum(fg) <= 127.0 { white() } else { black() });
    // adjust_bg_color: if too close, flip stroke to opposite pole.
    if color_diff(fg, stroke) < 30.0 {
        stroke = if lum(fg) <= 127.0 { white() } else { black() };
    }
    // stroke_width → 0 when still too similar.
    if color_diff(fg, stroke) <= 15.0 {
        (fg, None)
    } else {
        (fg, Some(stroke))
    }
}

/// CTC greedy decode + MIT color average on emitted (non-blank) steps.
fn ctc_decode_with_colors(
    logits: &[f32],
    colors: Option<&[f32]>,
    seq: usize,
    vocab: usize,
    keys: &[String],
) -> CropResult {
    let mut last = usize::MAX;
    let mut out = String::new();
    let mut fg_acc: Vec<(f32, f32, f32)> = Vec::new();
    let mut bg_acc: Vec<(f32, f32, f32)> = Vec::new();
    for t in 0..seq {
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
            let ch = keys.get(best_i).map(|s| s.as_str()).unwrap_or("");
            if ch != "<PAD>" && ch != "<S>" && ch != "</S>" && !ch.is_empty() {
                if ch == " " {
                    out.push(' ');
                } else {
                    out.push_str(ch);
                    if let Some(cols) = colors {
                        let base = t * 6;
                        if base + 6 <= cols.len() {
                            fg_acc.push((cols[base], cols[base + 1], cols[base + 2]));
                            bg_acc.push((cols[base + 3], cols[base + 4], cols[base + 5]));
                        }
                    }
                }
            }
        }
        last = best_i;
    }
    CropResult {
        text: out,
        fg: average_rgb(&fg_acc),
        stroke: average_rgb(&bg_acc),
    }
}

fn extract_logits_colors(
    outputs: &SessionOutputs<'_>,
) -> Result<(Vec<f32>, usize, usize, Option<Vec<f32>>), String> {
    let mut logits_flat: Option<Vec<f32>> = None;
    let mut logits_shape: Option<Vec<usize>> = None;
    let mut colors_flat: Option<Vec<f32>> = None;
    let mut colors_shape: Option<Vec<usize>> = None;

    for (name, value) in outputs.iter() {
        let array = value
            .try_extract_array::<f32>()
            .map_err(|err| err.to_string())?;
        let shape = array.shape().to_vec();
        let flat: Vec<f32> = array.iter().copied().collect();
        let lname = name.to_ascii_lowercase();
        if lname.contains("color") {
            colors_flat = Some(flat);
            colors_shape = Some(shape);
        } else if logits_flat.is_none() {
            logits_flat = Some(flat);
            logits_shape = Some(shape);
        }
    }

    let flat = logits_flat.ok_or_else(|| "mit48 has no logits".to_string())?;
    let shape = logits_shape.unwrap();
    let (seq, vocab, flat) = match shape.as_slice() {
        [_, a, b] if *b > *a => (*a, *b, flat),
        [_, a, b] => {
            let mut out = vec![0f32; a * b];
            for t in 0..*b {
                for c in 0..*a {
                    out[t * a + c] = flat[c * b + t];
                }
            }
            (*b, *a, out)
        }
        _ => return Err(format!("unexpected mit48 logits shape: {shape:?}")),
    };

    let colors = match (colors_flat, colors_shape) {
        (Some(cflat), Some(cshape)) => match cshape.as_slice() {
            [_, s, 6] if *s == seq => Some(cflat),
            [_, 6, s] if *s == seq => {
                let mut out = vec![0f32; seq * 6];
                for t in 0..seq {
                    for k in 0..6 {
                        out[t * 6 + k] = cflat[k * seq + t];
                    }
                }
                Some(out)
            }
            _ => {
                eprintln!("[mit48] ignore colors shape {cshape:?}");
                None
            }
        },
        _ => None,
    };

    Ok((flat, seq, vocab, colors))
}

fn infer_crop(
    session: &mut Session,
    crop: &RgbaImage,
    keys: &[String],
) -> Result<CropResult, String> {
    let target_h = 48u32;
    let (cw, ch) = crop.dimensions();
    let tw = ((cw as f32 / ch.max(1) as f32) * target_h as f32)
        .round()
        .clamp(48.0, 2048.0) as u32;
    let resized = to_rgb_resized(crop, tw, target_h);
    let (shape, data) = nchw_rgb_norm(&resized, [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "mit48 model has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("mit48 failed: {err}"))?;
    let (logits, seq, vocab, colors) = extract_logits_colors(&outputs)?;
    Ok(ctc_decode_with_colors(
        &logits,
        colors.as_deref(),
        seq,
        vocab,
        keys,
    ))
}

fn apply_crop_colors(region: &mut OcrRegion, crop: &CropResult) {
    let Some(fg) = crop.fg else {
        return;
    };
    let (fg, stroke) = finalize_ink_stroke(fg, crop.stroke);
    region.fg_color = Some(hex_rgb(fg.r, fg.g, fg.b));
    region.stroke_color = stroke.map(|s| hex_rgb(s.r, s.g, s.b));
}

fn session_has_colors(session: &Session) -> bool {
    session
        .outputs()
        .iter()
        .any(|o| o.name().to_ascii_lowercase().contains("color"))
}

/// Saber `extract_colors_for_bubbles`: color each textline (warped 48px), average per bubble.
/// Keeps sampled `bg_color` (paper fill). No-op if pack missing / no color head.
pub fn apply_model_colors(
    app: &AppHandle,
    img: &RgbaImage,
    blocks: &[TextBlock],
    regions: &mut [OcrRegion],
) {
    if regions.is_empty() || !ocr_assets::is_pack_installed(app, PACK) {
        return;
    }
    let Ok(onnx) = ocr_assets::pack_file(app, PACK, "ocr-48px-ctc.onnx") else {
        return;
    };
    let Ok(alphabet) = alphabet_path(app) else {
        return;
    };
    let Ok(keys) = load_alphabet(&alphabet) else {
        return;
    };
    let Ok(mut session) = open_session(app, &onnx) else {
        return;
    };
    if !session_has_colors(&session) {
        return;
    }

    // Precompute per-block colors from lines (Saber aggregation).
    let mut block_colors: Vec<Option<(Rgb8, Option<Rgb8>)>> = vec![None; blocks.len()];
    for (bi, block) in blocks.iter().enumerate() {
        let mut fgs = Vec::new();
        let mut strokes = Vec::new();
        for line in &block.lines {
            let crop_img = crop_line_48(img, line);
            let Ok(crop) = infer_crop(&mut session, &crop_img, &keys) else {
                continue;
            };
            if let Some(fg) = crop.fg {
                fgs.push(fg);
            }
            if let Some(stroke) = crop.stroke {
                strokes.push(stroke);
            }
        }
        if let Some(fg) = average_rgb8(&fgs) {
            block_colors[bi] = Some(finalize_ink_stroke(fg, average_rgb8(&strokes)));
        }
    }

    for region in regions.iter_mut() {
        let bi = region_block_index(region, blocks);
        if let Some(i) = bi {
            if let Some((fg, stroke)) = block_colors.get(i).and_then(|c| c.as_ref()) {
                region.fg_color = Some(hex_rgb(fg.r, fg.g, fg.b));
                region.stroke_color = stroke.map(|s| hex_rgb(s.r, s.g, s.b));
                continue;
            }
        }
        // No matching block: fall back to warped-ish AABB via region box as a DetBox.
        let line = super::detector::DetBox::from_xyxy(
            region.x,
            region.y,
            region.x + region.w,
            region.y + region.h,
            1.0,
        );
        let crop_img = crop_line_48(img, &line);
        if let Ok(crop) = infer_crop(&mut session, &crop_img, &keys) {
            apply_crop_colors(region, &crop);
        }
    }
}

fn region_block_index(region: &OcrRegion, blocks: &[TextBlock]) -> Option<usize> {
    // regions_from_* use id `r{block_index}` even when empty blocks are skipped.
    if let Some(rest) = region.id.strip_prefix('r') {
        if let Ok(i) = rest.parse::<usize>() {
            if i < blocks.len() {
                return Some(i);
            }
        }
    }
    // Fallback: largest IoU with block AABB.
    let mut best = None;
    let mut best_iou = 0.05f64;
    for (i, block) in blocks.iter().enumerate() {
        let b = block.aabb().clamp01();
        let iou = aabb_iou(
            region.x,
            region.y,
            region.w,
            region.h,
            b.x,
            b.y,
            b.w,
            b.h,
        );
        if iou > best_iou {
            best_iou = iou;
            best = Some(i);
        }
    }
    best
}

fn aabb_iou(ax: f64, ay: f64, aw: f64, ah: f64, bx: f64, by: f64, bw: f64, bh: f64) -> f64 {
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

/// Saber textline-aware path: CTC per line, one overlay region per bubble.
pub fn recognize_on_blocks(
    app: &AppHandle,
    bytes: &[u8],
    blocks: &[TextBlock],
) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    let alphabet = alphabet_path(app)?;
    let keys = load_alphabet(&alphabet)?;
    let onnx = ocr_assets::pack_file(app, PACK, "ocr-48px-ctc.onnx")?;
    let mut session = open_session(app, &onnx)?;
    let img = decode_rgba(bytes)?;

    let mut out = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        let mut parts: Vec<String> = Vec::new();
        let mut fgs = Vec::new();
        let mut strokes = Vec::new();
        for line in &block.lines {
            let crop_img = crop_line_48(&img, line);
            let crop = infer_crop(&mut session, &crop_img, &keys).unwrap_or_default();
            let t = crop.text.trim();
            if !t.is_empty() {
                parts.push(t.to_string());
            }
            if let Some(fg) = crop.fg {
                fgs.push(fg);
            }
            if let Some(stroke) = crop.stroke {
                strokes.push(stroke);
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
        let mut region = region(format!("r{i}"), b.x, b.y, b.w, b.h, parts.join(" "));
        let (fg_s, bg_s) = super::image_util::sample_region_colors(&img, &region);
        region.fg_color = fg_s;
        region.bg_color = bg_s;
        if let Some(fg) = average_rgb8(&fgs) {
            let (fg, stroke) = finalize_ink_stroke(fg, average_rgb8(&strokes));
            region.fg_color = Some(hex_rgb(fg.r, fg.g, fg.b));
            region.stroke_color = stroke.map(|s| hex_rgb(s.r, s.g, s.b));
        }
        out.push(region);
    }
    Ok(out)
}

/// Minimal fallback when no Saber detector blocks are available.
pub fn recognize_fallback(app: &AppHandle, bytes: &[u8]) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    let alphabet = alphabet_path(app)?;
    let keys = load_alphabet(&alphabet)?;
    let onnx = ocr_assets::pack_file(app, PACK, "ocr-48px-ctc.onnx")?;
    let mut session = open_session(app, &onnx)?;
    let img = decode_rgba(bytes)?;
    let crop = infer_crop(&mut session, &img, &keys)?;
    if crop.text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut regions = vec![region("r0", 0.08, 0.05, 0.84, 0.9, crop.text.clone())];
    apply_sampled_colors(&img, &mut regions);
    apply_crop_colors(&mut regions[0], &crop);
    Ok(regions)
}
