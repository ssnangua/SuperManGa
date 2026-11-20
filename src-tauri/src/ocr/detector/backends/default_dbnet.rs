//! Saber `DefaultBackend` (DBNet ResNet34) ONNX.
//!
//! - longest side → 1536, pad bottom/right to multiple of 256
//! - RGB → [-1, 1]
//! - SegDetectorRepresenter thresh=0.5, unclip_ratio=2.2
//! - boxes in resized space × (1/ratio) → original pixels → normalize

use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr::image_util::{decode_rgba, nchw_rgb_norm, to_rgb_resized};
use crate::ocr_assets;

use super::super::geom::DetBox;
use super::super::seg_detector::boxes_from_bitmap;

pub const PACK: &str = "detector-dbnet";
const FILE: &str = "detect-20241225.onnx";
const DETECT_SIZE: u32 = 1536;
const PAD_ALIGN: u32 = 256;
const TEXT_THRESH: f32 = 0.5;
/// Saber DefaultBackend DEFAULT_UNCLIP_RATIO.
const UNCLIP_RATIO: f64 = 2.2;

pub fn is_available(app: &AppHandle) -> bool {
    ocr_assets::is_pack_installed(app, PACK)
}

fn align_pad(v: u32) -> u32 {
    ((v + PAD_ALIGN - 1) / PAD_ALIGN) * PAD_ALIGN
}

pub fn detect(app: &AppHandle, bytes: &[u8]) -> Result<Vec<DetBox>, String> {
    if !is_available(app) {
        return Ok(Vec::new());
    }
    let img = decode_rgba(bytes)?;
    let (iw, ih) = img.dimensions();

    // Saber resize_aspect_ratio: scale longest side to DETECT_SIZE, pad % 256.
    let scale = DETECT_SIZE as f32 / (iw.max(ih) as f32).max(1.0);
    let tw = ((iw as f32 * scale).round() as u32).max(1);
    let th = ((ih as f32 * scale).round() as u32).max(1);
    // Prefer Saber pad-to-%256; also pad to DETECT_SIZE² for fixed-shape ONNX.
    let pw = align_pad(tw).max(DETECT_SIZE);
    let ph = align_pad(th).max(DETECT_SIZE);
    let resized = to_rgb_resized(&img, tw, th);
    let mut canvas = image::RgbaImage::from_pixel(pw, ph, image::Rgba([0, 0, 0, 255]));
    for y in 0..th {
        for x in 0..tw {
            canvas.put_pixel(x, y, *resized.get_pixel(x, y));
        }
    }

    // Saber: pixel/127.5 - 1 → [-1, 1]
    let (shape, data) = nchw_rgb_norm(&canvas, [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
    let onnx = ocr_assets::pack_file(app, PACK, FILE)?;
    let mut session = open_session(app, &onnx)?;
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "default detector has no inputs".to_string())?;
    let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|err| format!("default detector failed: {err}"))?;
    let output = outputs
        .iter()
        .next()
        .ok_or_else(|| "default detector has no outputs".to_string())?
        .1;
    let array = output
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape().to_vec();
    let flat: Vec<f32> = array.iter().copied().collect();
    let (mh, mw, map) = match shape.as_slice() {
        [_, c, h, w] if *c >= 1 => {
            let map = flat[..h * w].to_vec();
            (*h, *w, map)
        }
        [_, h, w] => (*h, *w, flat),
        [h, w] => (*h, *w, flat),
        _ => return Err(format!("unexpected default detector shape: {shape:?}")),
    };

    // Saber: seg_rep(db, height=img_resized_h, width=img_resized_w) then * (1/ratio).
    // Run on full padded map → canvas pixel space, then / scale → original.
    let boxes = boxes_from_bitmap(&map, mh, mw, TEXT_THRESH, UNCLIP_RATIO, pw, ph);
    let mut out = Vec::new();
    for b in boxes {
        // Canvas-normalized → original pixels via content scale (top-left placement).
        let x0 = (b.x * pw as f64) / scale as f64;
        let y0 = (b.y * ph as f64) / scale as f64;
        let x1 = ((b.x + b.w) * pw as f64) / scale as f64;
        let y1 = ((b.y + b.h) * ph as f64) / scale as f64;
        let nx0 = (x0 / iw as f64).clamp(0.0, 1.0);
        let ny0 = (y0 / ih as f64).clamp(0.0, 1.0);
        let nx1 = (x1 / iw as f64).clamp(0.0, 1.0);
        let ny1 = (y1 / ih as f64).clamp(0.0, 1.0);
        if nx1 - nx0 < 0.002 || ny1 - ny0 < 0.002 {
            continue;
        }
        let area_px = (nx1 - nx0) * iw as f64 * (ny1 - ny0) * ih as f64;
        // Saber: textline.area > 16
        if area_px <= 16.0 {
            continue;
        }
        out.push(DetBox::from_xyxy(nx0, ny0, nx1, ny1, b.conf));
    }
    Ok(out)
}
