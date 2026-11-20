//! SaberYOLO OBB ONNX — Saber `SaberYoloBackend` / ultralytics `predict` postprocess.
//!
//! Model emits **final text-block candidates** (not raw glyph lines). NMS must match
//! ultralytics OBB (`agnostic_nms`, conf/iou thresholds): letterbox `xywh` IoU.
//! Polygon-IoU NMS under-suppresses and over-fragments refine references.

use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr::image_util::{decode_rgba, nchw_rgb_plain, to_rgb_resized};
use crate::ocr_assets;

use super::super::geom::DetBox;

pub const PACK: &str = "detector-saber-yolo";
const FILE: &str = "saber_yolo.onnx";
const DETECT_SIZE: u32 = 640;
/// Saber `SABER_YOLO_CONF_THRESH`.
const DEFAULT_CONF: f32 = 0.2;
/// Saber `SABER_YOLO_IOU_THRESH` / ultralytics OBB NMS.
const DEFAULT_IOU: f64 = 0.5;

pub fn is_available(app: &AppHandle) -> bool {
    ocr_assets::is_pack_installed(app, PACK)
}

pub fn detect(app: &AppHandle, bytes: &[u8]) -> Result<Vec<DetBox>, String> {
    if !is_available(app) {
        return Ok(Vec::new());
    }
    let img = decode_rgba(bytes)?;
    let (iw, ih) = img.dimensions();
    let side = DETECT_SIZE;
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
        .ok_or_else(|| "saber_yolo has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("saber_yolo failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "saber_yolo has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape().to_vec();
    let flat: Vec<f32> = array.iter().copied().collect();
    // This ONNX export: [1, 4+nc+1, N] = cx,cy,w,h, scores…, angle (angle last).
    let raw = parse_obb(&flat, &shape, DEFAULT_CONF)?;
    // NMS in letterbox xywh (Saber ultralytics predict), then keep OBB quads.
    let kept_raw = nms_xywh(&raw, DEFAULT_IOU);
    let mut out = Vec::new();
    for (cx, cy, bw, bh, angle, conf) in kept_raw {
        let angle = if angle.abs() > std::f32::consts::PI {
            angle.to_radians()
        } else {
            angle
        };
        let corners = obb_corners(cx, cy, bw, bh, angle);
        let mut pts = [[0.0f64; 2]; 4];
        let mut ok = true;
        for (i, (px, py)) in corners.iter().enumerate() {
            let x = (*px - ox as f32) / scale;
            let y = (*py - oy as f32) / scale;
            pts[i] = [
                (x as f64 / iw as f64).clamp(0.0, 1.0),
                (y as f64 / ih as f64).clamp(0.0, 1.0),
            ];
            if !x.is_finite() || !y.is_finite() {
                ok = false;
            }
        }
        if !ok {
            continue;
        }
        let box_ = DetBox::from_quad(pts, conf);
        if box_.w < 0.004 || box_.h < 0.003 || box_.poly_area() <= 0.0 {
            continue;
        }
        out.push(box_);
    }
    // xywh IoU NMS keeps parent+child when IoU = child/parent < 0.5. Ultralytics
    // .pt rarely emits those parents; drop parents that enclose ≥2 smaller refs so
    // refine does not assign both columns to one oversized candidate.
    let before_parent = out.len();
    out = suppress_multi_child_parents(out);
    crate::app_log::info(
        "ocr",
        format!(
            "saber_yolo raw={} after_nms={} after_parent_filter={} conf>={}",
            raw.len(),
            before_parent,
            out.len(),
            DEFAULT_CONF
        ),
    );
    Ok(out)
}

/// Drop a box if it contains the centers of ≥2 substantially smaller boxes.
fn suppress_multi_child_parents(boxes: Vec<DetBox>) -> Vec<DetBox> {
    let n = boxes.len();
    if n < 3 {
        return boxes;
    }
    let mut drop = vec![false; n];
    for i in 0..n {
        let ia = boxes[i].poly_area();
        if ia <= 0.0 {
            continue;
        }
        let mut children = 0usize;
        for (j, b) in boxes.iter().enumerate() {
            if i == j {
                continue;
            }
            let ja = b.poly_area();
            if ja <= 0.0 || ja >= ia * 0.85 {
                continue;
            }
            let (cx, cy) = b.center();
            if boxes[i].contains_point(cx, cy) {
                children += 1;
                if children >= 2 {
                    drop[i] = true;
                    break;
                }
            }
        }
    }
    boxes
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop[*i])
        .map(|(_, b)| b)
        .collect()
}

/// Ultralytics-style agnostic NMS on letterbox `cx,cy,w,h` (angle ignored for IoU).
fn nms_xywh(
    boxes: &[(f32, f32, f32, f32, f32, f32)],
    iou_thresh: f64,
) -> Vec<(f32, f32, f32, f32, f32, f32)> {
    let mut idxs: Vec<usize> = (0..boxes.len()).collect();
    idxs.sort_by(|&a, &b| {
        boxes[b]
            .5
            .partial_cmp(&boxes[a].5)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep = Vec::new();
    let mut suppressed = vec![false; boxes.len()];
    for &i in &idxs {
        if suppressed[i] {
            continue;
        }
        keep.push(boxes[i]);
        let (acx, acy, aw, ah, _, _) = boxes[i];
        let ax1 = acx - aw * 0.5;
        let ay1 = acy - ah * 0.5;
        let ax2 = acx + aw * 0.5;
        let ay2 = acy + ah * 0.5;
        let aa = (aw * ah) as f64;
        for &j in &idxs {
            if suppressed[j] || j == i {
                continue;
            }
            let (bcx, bcy, bw, bh, _, _) = boxes[j];
            let bx1 = bcx - bw * 0.5;
            let by1 = bcy - bh * 0.5;
            let bx2 = bcx + bw * 0.5;
            let by2 = bcy + bh * 0.5;
            let ix1 = ax1.max(bx1);
            let iy1 = ay1.max(by1);
            let ix2 = ax2.min(bx2);
            let iy2 = ay2.min(by2);
            let inter = ((ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0)) as f64;
            let uni = aa + (bw * bh) as f64 - inter;
            if uni > 0.0 && inter / uni >= iou_thresh {
                suppressed[j] = true;
            }
        }
    }
    keep
}

fn obb_corners(cx: f32, cy: f32, w: f32, h: f32, angle: f32) -> [(f32, f32); 4] {
    let (s, c) = angle.sin_cos();
    let hw = w * 0.5;
    let hh = h * 0.5;
    [
        (cx + c * (-hw) - s * (-hh), cy + s * (-hw) + c * (-hh)),
        (cx + c * hw - s * (-hh), cy + s * hw + c * (-hh)),
        (cx + c * hw - s * hh, cy + s * hw + c * hh),
        (cx + c * (-hw) - s * hh, cy + s * (-hw) + c * hh),
    ]
}

fn parse_obb(
    flat: &[f32],
    shape: &[usize],
    conf_thresh: f32,
) -> Result<Vec<(f32, f32, f32, f32, f32, f32)>, String> {
    let (n, c, transposed) = match shape {
        [1, c, n] if *c >= 5 => (*n, *c, true),
        [1, n, c] if *c >= 5 => (*n, *c, false),
        [c, n] if *c >= 5 => (*n, *c, true),
        [n, c] if *c >= 5 => (*n, *c, false),
        _ => return Err(format!("unexpected saber_yolo shape: {shape:?}")),
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
        // Ultralytics OBB export used here: cx,cy,w,h, score(s)…, angle (last).
        // Scores are already probabilities — no sigmoid.
        let (angle, best) = if c >= 6 {
            let angle = get(c - 1);
            let mut best = 0.0f32;
            for k in 4..c - 1 {
                best = best.max(get(k));
            }
            (angle, best)
        } else {
            (0.0, get(4))
        };
        if best < conf_thresh {
            continue;
        }
        out.push((cx, cy, bw, bh, angle, best));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_obb_uses_score_then_angle_last() {
        // [1, 6, 2] transposed: cx,cy,w,h,score,angle
        let n = 2usize;
        let c = 6usize;
        let mut flat = vec![0.0f32; c * n];
        let mut set = |k: usize, i: usize, v: f32| {
            flat[k * n + i] = v;
        };
        // det0: high score, small angle — keep
        set(0, 0, 100.0);
        set(1, 0, 200.0);
        set(2, 0, 30.0);
        set(3, 0, 40.0);
        set(4, 0, 0.91);
        set(5, 0, 0.12);
        // det1: low score, large angle-looking value — drop (old bug kept these)
        set(0, 1, 10.0);
        set(1, 1, 20.0);
        set(2, 1, 5.0);
        set(3, 1, 6.0);
        set(4, 1, 0.05);
        set(5, 1, 0.85);
        let got = parse_obb(&flat, &[1, c, n], 0.2).unwrap();
        assert_eq!(got.len(), 1);
        let (cx, cy, bw, bh, angle, conf) = got[0];
        assert!((cx - 100.0).abs() < 1e-5);
        assert!((cy - 200.0).abs() < 1e-5);
        assert!((bw - 30.0).abs() < 1e-5);
        assert!((bh - 40.0).abs() < 1e-5);
        assert!((angle - 0.12).abs() < 1e-5);
        assert!((conf - 0.91).abs() < 1e-5);
    }
}
