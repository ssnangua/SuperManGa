//! YSGYolo (ultralytics detect) ONNX → normalized AABB boxes.

use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr_assets;
use crate::ocr::image_util::{decode_rgba, nchw_rgb_plain, to_rgb_resized};

use super::super::geom::DetBox;

pub const PACK: &str = "detector-yolo";
const FILE: &str = "ysgyolo.onnx";
const DETECT_SIZE: u32 = 640;
const DEFAULT_CONF: f32 = 0.3;

pub fn is_available(app: &AppHandle) -> bool {
    ocr_assets::is_pack_installed(app, PACK)
}

pub fn detect(app: &AppHandle, bytes: &[u8]) -> Result<Vec<DetBox>, String> {
    detect_with_conf(app, bytes, DEFAULT_CONF)
}

pub fn detect_with_conf(app: &AppHandle, bytes: &[u8], conf_thresh: f32) -> Result<Vec<DetBox>, String> {
    if !is_available(app) {
        return Ok(Vec::new());
    }
    let img = decode_rgba(bytes)?;
    let (iw, ih) = img.dimensions();
    let side = DETECT_SIZE;
    // Letterbox-style: resize longest side to DETECT_SIZE, pad.
    let scale = side as f32 / (iw.max(ih) as f32).max(1.0);
    let tw = ((iw as f32 * scale).round() as u32).max(1);
    let th = ((ih as f32 * scale).round() as u32).max(1);
    let resized = to_rgb_resized(&img, tw, th);
    let mut canvas = image::RgbaImage::from_pixel(side, side, image::Rgba([114, 114, 114, 255]));
    let ox = (side - tw) / 2;
    let oy = (side - th) / 2;
    for y in 0..th {
        for x in 0..tw {
            canvas.put_pixel(x + ox, y + oy, *resized.get_pixel(x, y));
        }
    }
    let (shape, data) = nchw_rgb_plain(&canvas);
    let onnx = ocr_assets::pack_file(app, PACK, FILE)?;
    let mut session = open_session(app, &onnx)?;
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "yolo detector has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("yolo detector failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "yolo detector has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape().to_vec();
    let flat: Vec<f32> = array.iter().copied().collect();
    let raw = parse_yolo_detect(&flat, &shape, conf_thresh)?;
    // Map from letterbox canvas coords back to original normalized.
    let mut out = Vec::new();
    for (cx, cy, bw, bh, conf) in raw {
        // cx,cy,bw,bh in pixel space of side×side canvas (center format) or xyxy
        let x0 = (cx - bw * 0.5 - ox as f32) / scale;
        let y0 = (cy - bh * 0.5 - oy as f32) / scale;
        let x1 = (cx + bw * 0.5 - ox as f32) / scale;
        let y1 = (cy + bh * 0.5 - oy as f32) / scale;
        let nx0 = (x0 as f64 / iw as f64).clamp(0.0, 1.0);
        let ny0 = (y0 as f64 / ih as f64).clamp(0.0, 1.0);
        let nx1 = (x1 as f64 / iw as f64).clamp(0.0, 1.0);
        let ny1 = (y1 as f64 / ih as f64).clamp(0.0, 1.0);
        if nx1 - nx0 < 0.004 || ny1 - ny0 < 0.003 {
            continue;
        }
        out.push(DetBox::from_xyxy(nx0, ny0, nx1, ny1, conf));
    }
    Ok(nms(&out, 0.5))
}

/// Parse ultralytics detect ONNX: [1, C, N] or [1, N, C] with C = 4+nc (xywh + class scores).
fn parse_yolo_detect(
    flat: &[f32],
    shape: &[usize],
    conf_thresh: f32,
) -> Result<Vec<(f32, f32, f32, f32, f32)>, String> {
    let (n, c, transposed) = match shape {
        [1, c, n] if *c >= 5 => (*n, *c, true),
        [1, n, c] if *c >= 5 => (*n, *c, false),
        [c, n] if *c >= 5 => (*n, *c, true),
        [n, c] if *c >= 5 => (*n, *c, false),
        _ => return Err(format!("unexpected yolo shape: {shape:?}")),
    };
    let mut out = Vec::new();
    for i in 0..n {
        let get = |k: usize| -> f32 {
            if transposed {
                flat[k * n + i]
            } else {
                flat[i * c + k]
            }
        };
        let cx = get(0);
        let cy = get(1);
        let bw = get(2);
        let bh = get(3);
        let mut best = 0.0f32;
        for k in 4..c {
            best = best.max(get(k));
        }
        // Some exports put objectness at index 4 then classes.
        if c == 5 {
            best = get(4);
        }
        if best < conf_thresh {
            continue;
        }
        out.push((cx, cy, bw, bh, best));
    }
    Ok(out)
}

fn nms(boxes: &[DetBox], iou_thresh: f64) -> Vec<DetBox> {
    let mut idxs: Vec<usize> = (0..boxes.len()).collect();
    idxs.sort_by(|&a, &b| {
        boxes[b]
            .conf
            .partial_cmp(&boxes[a].conf)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep = Vec::new();
    let mut suppressed = vec![false; boxes.len()];
    for &i in &idxs {
        if suppressed[i] {
            continue;
        }
        keep.push(boxes[i]);
        for &j in &idxs {
            if suppressed[j] || j == i {
                continue;
            }
            let inter = super::super::geom::intersection_area(&boxes[i], &boxes[j]);
            let uni = boxes[i].area() + boxes[j].area() - inter;
            if uni > 0.0 && inter / uni >= iou_thresh {
                suppressed[j] = true;
            }
        }
    }
    keep
}
