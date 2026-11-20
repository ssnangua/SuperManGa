//! Comic Text Detector ONNX — Saber `CTDBackend._detect_raw`.
//!
//! - letterbox 1024², pad bottom/right
//! - BGR / 255
//! - heads: blk / seg / det; text lines from `det` channel 0
//! - crop letterbox pad from lines_map, then SegDetectorRepresenter → original size
//! - box_threshold = 0.7 (Saber CTDBackend default)

use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr::image_util::{decode_rgba, letterbox_rgb, nchw_bgr_plain};
use crate::ocr_assets;

use super::super::geom::DetBox;
use super::super::seg_detector::boxes_from_bitmap;

pub const PACK: &str = "detector-ctd";
const FILE: &str = "comictextdetector.pt.onnx";
const INPUT_SIDE: u32 = 1024;
/// Saber `DEFAULT_TEXT_THRESHOLD` / SegDetectorRepresenter thresh.
const TEXT_THRESHOLD: f32 = 0.3;
/// Saber SegDetectorRepresenter default.
const UNCLIP_RATIO: f64 = 1.5;
/// Saber `DEFAULT_BOX_THRESHOLD`.
const BOX_THRESHOLD: f32 = 0.7;

pub fn is_available(app: &AppHandle) -> bool {
    ocr_assets::is_pack_installed(app, PACK)
}

pub fn detect(app: &AppHandle, bytes: &[u8]) -> Result<Vec<DetBox>, String> {
    if !is_available(app) {
        return Ok(Vec::new());
    }
    let img = decode_rgba(bytes)?;
    let (iw, ih) = img.dimensions();
    let (canvas, _scale, pad_w, pad_h, _, _) = letterbox_rgb(&img, INPUT_SIDE);

    let (shape, data) = nchw_bgr_plain(&canvas);
    let onnx = ocr_assets::pack_file(app, PACK, FILE)?;
    let mut session = open_session(app, &onnx)?;
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "detector has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("detector failed: {err}"))?;

    let (mh, mw, map) = extract_lines_map(&outputs)?;
    // Saber: crop pad from bottom/right before SegDetectorRepresenter.
    let pad_h_map = ((pad_h as usize) * mh / INPUT_SIDE as usize).min(mh.saturating_sub(1));
    let pad_w_map = ((pad_w as usize) * mw / INPUT_SIDE as usize).min(mw.saturating_sub(1));
    let uh = (mh - pad_h_map).max(1);
    let uw = (mw - pad_w_map).max(1);
    let cropped = crop_map(&map, mh, mw, uh, uw);

    // Saber: seg_rep(lines_map, height=im_h, width=im_w) → boxes in original pixels → we normalize.
    let boxes = boxes_from_bitmap(&cropped, uh, uw, TEXT_THRESHOLD, UNCLIP_RATIO, iw, ih);
    Ok(boxes
        .into_iter()
        .filter(|b| b.conf > BOX_THRESHOLD)
        .filter(|b| b.w >= 0.002 && b.h >= 0.002)
        .collect())
}

fn crop_map(map: &[f32], mh: usize, mw: usize, uh: usize, uw: usize) -> Vec<f32> {
    let uh = uh.min(mh);
    let uw = uw.min(mw);
    let mut out = Vec::with_capacity(uh * uw);
    for y in 0..uh {
        let start = y * mw;
        out.extend_from_slice(&map[start..start + uw]);
    }
    out
}

fn extract_lines_map(
    outputs: &ort::session::SessionOutputs<'_>,
) -> Result<(usize, usize, Vec<f32>), String> {
    let mut best: Option<(usize, usize, usize, Vec<f32>)> = None;
    for (name, output) in outputs.iter() {
        let Ok(array) = output.try_extract_array::<f32>() else {
            continue;
        };
        let shape = array.shape().to_vec();
        let flat: Vec<f32> = array.iter().copied().collect();
        let name_l = name.to_ascii_lowercase();
        let name_boost = if name_l.contains("det") && !name_l.contains("blk") {
            10
        } else if name_l.contains("seg") || name_l.contains("mask") {
            1
        } else {
            0
        };
        let parsed = match shape.as_slice() {
            [_, 2, h, w] if *h >= 64 && *w >= 64 => {
                Some((2 + name_boost, *h, *w, flat[..h * w].to_vec()))
            }
            [_, 1, h, w] if *h >= 64 && *w >= 64 => {
                Some((1 + name_boost, *h, *w, flat[..h * w].to_vec()))
            }
            [_, h, w] if *h >= 64 && *w >= 64 => Some((name_boost, *h, *w, flat)),
            [h, w] if *h >= 64 && *w >= 64 => Some((name_boost, *h, *w, flat)),
            _ => None,
        };
        if let Some((rank, h, w, map)) = parsed {
            let replace = match &best {
                None => true,
                Some((best_rank, ..)) => rank > *best_rank,
            };
            if replace {
                best = Some((rank, h, w, map));
            }
        }
    }
    best
        .map(|(_, h, w, map)| (h, w, map))
        .ok_or_else(|| "CTD produced no usable lines_map / mask output".into())
}
