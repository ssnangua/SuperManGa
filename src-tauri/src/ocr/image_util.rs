use image::imageops::FilterType;
use image::{DynamicImage, Rgba, RgbaImage};

use super::detector::DetBox;
use super::OcrRegion;

pub fn decode_rgba(bytes: &[u8]) -> Result<RgbaImage, String> {
    image::load_from_memory(bytes)
        .map_err(|err| err.to_string())
        .map(|img| img.to_rgba8())
}

pub fn to_rgb_resized(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    DynamicImage::ImageRgba8(img.clone())
        .resize_exact(w, h, FilterType::Triangle)
        .to_rgba8()
}

pub fn crop_norm(img: &RgbaImage, region: &OcrRegion) -> RgbaImage {
    crop_norm_padded(img, region, 0.0)
}

/// Crop a normalized region with extra padding (fraction of box size) for OCR.
pub fn crop_norm_padded(img: &RgbaImage, region: &OcrRegion, pad_frac: f64) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let pad_x = region.w * pad_frac;
    let pad_y = region.h * pad_frac;
    let x0 = ((region.x - pad_x) * iw as f64).floor().max(0.0) as u32;
    let y0 = ((region.y - pad_y) * ih as f64).floor().max(0.0) as u32;
    let x1 = (((region.x + region.w + pad_x) * iw as f64).ceil() as u32).min(iw);
    let y1 = (((region.y + region.h + pad_y) * ih as f64).ceil() as u32).min(ih);
    let w = x1.saturating_sub(x0).max(1);
    let h = y1.saturating_sub(y0).max(1);
    DynamicImage::ImageRgba8(img.clone())
        .crop_imm(x0, y0, w, h)
        .to_rgba8()
}

/// Saber `get_transformed_region`: perspective-warp a textline quad to height 48.
/// Vertical lines are rotated 90° CCW so CTC/color sees a horizontal strip.
pub fn crop_line_48(img: &RgbaImage, line: &DetBox) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return RgbaImage::from_pixel(48, 48, Rgba([0, 0, 0, 255]));
    }
    let mut src = [[0.0f64; 2]; 4];
    for i in 0..4 {
        src[i][0] = (line.pts[i][0] * iw as f64).clamp(0.0, (iw - 1) as f64);
        src[i][1] = (line.pts[i][1] * ih as f64).clamp(0.0, (ih - 1) as f64);
    }
    // Midpoints of edges → vertical / horizontal span (Saber).
    let mid = [
        [
            (src[0][0] + src[1][0]) * 0.5,
            (src[0][1] + src[1][1]) * 0.5,
        ],
        [
            (src[1][0] + src[2][0]) * 0.5,
            (src[1][1] + src[2][1]) * 0.5,
        ],
        [
            (src[2][0] + src[3][0]) * 0.5,
            (src[2][1] + src[3][1]) * 0.5,
        ],
        [
            (src[3][0] + src[0][0]) * 0.5,
            (src[3][1] + src[0][1]) * 0.5,
        ],
    ];
    let vec_v = [mid[2][0] - mid[0][0], mid[2][1] - mid[0][1]];
    let vec_h = [mid[1][0] - mid[3][0], mid[1][1] - mid[3][1]];
    let norm_v = (vec_v[0] * vec_v[0] + vec_v[1] * vec_v[1]).sqrt().max(1.0);
    let norm_h = (vec_h[0] * vec_h[0] + vec_h[1] * vec_h[1]).sqrt().max(1.0);
    let ratio = norm_v / norm_h;
    let vertical = line.h >= line.w * 1.05;
    let (dst_w, dst_h) = if vertical {
        let w = 48u32;
        let h = ((48.0 * ratio).round() as u32).clamp(8, 2048);
        (w, h)
    } else {
        let h = 48u32;
        let w = ((48.0 / ratio).round() as u32).clamp(8, 2048);
        (w, h)
    };
    let dst = [
        [0.0, 0.0],
        [(dst_w - 1) as f64, 0.0],
        [(dst_w - 1) as f64, (dst_h - 1) as f64],
        [0.0, (dst_h - 1) as f64],
    ];
    let Some(h_inv) = homography_inv(src, dst) else {
        // Fallback AABB resize.
        let draft = super::OcrRegion {
            id: "tmp".into(),
            x: line.x,
            y: line.y,
            w: line.w,
            h: line.h,
            text: String::new(),
            translation: None,
            fg_color: None,
            bg_color: None,
            stroke_color: None,
            vertical: None,
        };
        let aabb = crop_norm(img, &draft);
        return to_rgb_resized(&aabb, dst_w.max(48), 48);
    };
    let mut out = RgbaImage::new(dst_w, dst_h);
    for y in 0..dst_h {
        for x in 0..dst_w {
            let (sx, sy) = apply_homography(&h_inv, x as f64, y as f64);
            out.put_pixel(x, y, sample_bilinear(img, sx, sy));
        }
    }
    if vertical {
        // ROTATE_90_COUNTERCLOCKWISE → height becomes 48 for CTC.
        rotate90_ccw(&out)
    } else {
        out
    }
}

fn rotate90_ccw(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = RgbaImage::new(h, w);
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(y, w - 1 - x, *img.get_pixel(x, y));
        }
    }
    out
}

fn sample_bilinear(img: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return Rgba([0, 0, 0, 255]);
    }
    let x = x.clamp(0.0, (iw - 1) as f64);
    let y = y.clamp(0.0, (ih - 1) as f64);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(iw - 1);
    let y1 = (y0 + 1).min(ih - 1);
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut out = [0u8; 4];
    for c in 0..4 {
        let v = (1.0 - fx) * (1.0 - fy) * p00[c] as f64
            + fx * (1.0 - fy) * p10[c] as f64
            + (1.0 - fx) * fy * p01[c] as f64
            + fx * fy * p11[c] as f64;
        out[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

/// 3×3 inverse homography mapping destination → source.
fn homography_inv(src: [[f64; 2]; 4], dst: [[f64; 2]; 4]) -> Option<[f64; 9]> {
    // Solve H * dst = src (so out_xy → in_xy). Build 8×8 system for h0..h7 (h8=1).
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for i in 0..4 {
        let (x, y) = (dst[i][0], dst[i][1]);
        let (u, v) = (src[i][0], src[i][1]);
        let r = i * 2;
        a[r] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u];
        b[r] = u;
        a[r + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v];
        b[r + 1] = v;
    }
    let h8 = solve8(&mut a, &mut b)?;
    Some([
        h8[0], h8[1], h8[2], h8[3], h8[4], h8[5], h8[6], h8[7], 1.0,
    ])
}

fn apply_homography(h: &[f64; 9], x: f64, y: f64) -> (f64, f64) {
    let w = h[6] * x + h[7] * y + h[8];
    if w.abs() < 1e-12 {
        return (x, y);
    }
    (
        (h[0] * x + h[1] * y + h[2]) / w,
        (h[3] * x + h[4] * y + h[5]) / w,
    )
}

fn solve8(a: &mut [[f64; 8]; 8], b: &mut [f64; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        let mut pivot = col;
        for r in col + 1..8 {
            if a[r][col].abs() > a[pivot][col].abs() {
                pivot = r;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        if pivot != col {
            a.swap(pivot, col);
            b.swap(pivot, col);
        }
        let diag = a[col][col];
        for c in col..8 {
            a[col][c] /= diag;
        }
        b[col] /= diag;
        for r in 0..8 {
            if r == col {
                continue;
            }
            let f = a[r][col];
            for c in col..8 {
                a[r][c] -= f * a[col][c];
            }
            b[r] -= f * b[col];
        }
    }
    Some(*b)
}

/// Saber Paddle path: expand text-line quad by ~1 source pixel before CTC crop.
pub fn crop_norm_expand_px(img: &RgbaImage, region: &OcrRegion, px: i32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let x0 = ((region.x * iw as f64).floor() as i32 - px).max(0) as u32;
    let y0 = ((region.y * ih as f64).floor() as i32 - px).max(0) as u32;
    let x1 = (((region.x + region.w) * iw as f64).ceil() as i32 + px).clamp(0, iw as i32) as u32;
    let y1 = (((region.y + region.h) * ih as f64).ceil() as i32 + px).clamp(0, ih as i32) as u32;
    let w = x1.saturating_sub(x0).max(1);
    let h = y1.saturating_sub(y0).max(1);
    DynamicImage::ImageRgba8(img.clone())
        .crop_imm(x0, y0, w, h)
        .to_rgba8()
}

pub fn nchw_rgb_norm(
    img: &RgbaImage,
    mean: [f32; 3],
    std: [f32; 3],
) -> (Vec<usize>, Vec<f32>) {
    let (w, h) = img.dimensions();
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for c in 0..3 {
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x, y).0;
                let v = f32::from(p[c]) / 255.0;
                data.push((v - mean[c]) / std[c]);
            }
        }
    }
    (vec![1, 3, h as usize, w as usize], data)
}

pub fn nchw_rgb_plain(img: &RgbaImage) -> (Vec<usize>, Vec<f32>) {
    nchw_rgb_norm(img, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
}

/// NCHW as BGR / 255 (CTD ONNX / OpenCV `blobFromImage` effective order).
pub fn nchw_bgr_plain(img: &RgbaImage) -> (Vec<usize>, Vec<f32>) {
    let (w, h) = img.dimensions();
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for c in [2usize, 1, 0] {
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x, y).0;
                data.push(f32::from(p[c]) / 255.0);
            }
        }
    }
    (vec![1, 3, h as usize, w as usize], data)
}

/// CTD / YOLO-style letterbox: scale to fit `side`², pad bottom/right with black.
/// Returns `(canvas, scale, pad_w, pad_h, orig_w, orig_h)` where content sits at top-left.
pub fn letterbox_rgb(img: &RgbaImage, side: u32) -> (RgbaImage, f32, u32, u32, u32, u32) {
    let (iw, ih) = img.dimensions();
    let side = side.max(1);
    let scale = side as f32 / (iw.max(ih) as f32).max(1.0);
    let tw = ((iw as f32 * scale).round() as u32).max(1).min(side);
    let th = ((ih as f32 * scale).round() as u32).max(1).min(side);
    let pad_w = side.saturating_sub(tw);
    let pad_h = side.saturating_sub(th);
    let resized = to_rgb_resized(img, tw, th);
    let mut canvas = RgbaImage::from_pixel(side, side, image::Rgba([0, 0, 0, 255]));
    for y in 0..th {
        for x in 0..tw {
            canvas.put_pixel(x, y, *resized.get_pixel(x, y));
        }
    }
    (canvas, scale, pad_w, pad_h, iw, ih)
}

/// Paddle/RapidOCR det: resize long side, keep ratio, pad to 32.
pub fn paddle_det_input(img: &RgbaImage, limit_side: u32) -> (RgbaImage, f32, f32, u32, u32) {
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
    (canvas, scale, scale, w, h)
}

pub fn region(id: impl Into<String>, x: f64, y: f64, w: f64, h: f64, text: String) -> OcrRegion {
    OcrRegion {
        id: id.into(),
        x: x.clamp(0.0, 1.0),
        y: y.clamp(0.0, 1.0),
        w: w.clamp(0.0, 1.0),
        h: h.clamp(0.0, 1.0),
        text,
        translation: None,
        bg_color: None,
        fg_color: None,
        stroke_color: None,
        vertical: None,
    }
}

pub fn hex_rgb(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn luminance(r: u8, g: u8, b: u8) -> f64 {
    0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)
}

/// Fill `fg_color` / `bg_color` on every region from the source page.
pub fn apply_sampled_colors(img: &RgbaImage, regions: &mut [OcrRegion]) {
    for region in regions.iter_mut() {
        let (fg, bg) = sample_region_colors(img, region);
        region.fg_color = fg;
        region.bg_color = bg;
    }
}

/// Sample bubble paper (bg) and ink (fg) like Saber's solid inpaint fallback.
/// Uses border/corner pixels so center glyph ink does not dominate the fill.
pub fn sample_region_colors(
    img: &RgbaImage,
    region: &OcrRegion,
) -> (Option<String>, Option<String>) {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return (None, None);
    }
    let x0 = ((region.x) * iw as f64).floor().max(0.0) as u32;
    let y0 = ((region.y) * ih as f64).floor().max(0.0) as u32;
    let x1 = (((region.x + region.w) * iw as f64).ceil() as u32).min(iw).max(x0 + 1);
    let y1 = (((region.y + region.h) * ih as f64).ceil() as u32).min(ih).max(y0 + 1);
    let bw = x1 - x0;
    let bh = y1 - y0;

    let mut samples: Vec<(u8, u8, u8)> = Vec::with_capacity(64);
    let push = |samples: &mut Vec<(u8, u8, u8)>, x: u32, y: u32| {
        if x < iw && y < ih {
            let p = img.get_pixel(x, y).0;
            samples.push((p[0], p[1], p[2]));
        }
    };
    // Corners + edge midpoints (inset 1–2 px).
    let inset_x = (bw / 12).max(1).min(4);
    let inset_y = (bh / 12).max(1).min(4);
    let xs = [
        x0 + inset_x,
        x0 + bw / 2,
        x1.saturating_sub(inset_x + 1),
    ];
    let ys = [
        y0 + inset_y,
        y0 + bh / 2,
        y1.saturating_sub(inset_y + 1),
    ];
    for &y in &ys {
        for &x in &xs {
            push(&mut samples, x, y);
        }
    }
    // Extra border ring.
    for i in 0..8 {
        let t = i as f64 / 7.0;
        let x = x0 + ((bw as f64 - 1.0) * t).round() as u32;
        push(&mut samples, x, y0 + inset_y);
        push(&mut samples, x, y1.saturating_sub(inset_y + 1));
        let y = y0 + ((bh as f64 - 1.0) * t).round() as u32;
        push(&mut samples, x0 + inset_x, y);
        push(&mut samples, x1.saturating_sub(inset_x + 1), y);
    }
    if samples.is_empty() {
        return (None, None);
    }

    let mut lums: Vec<(f64, usize)> = samples
        .iter()
        .enumerate()
        .map(|(i, &(r, g, b))| (luminance(r, g, b), i))
        .collect();
    lums.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let dark_i = lums[lums.len() / 8].1;
    let light_i = lums[lums.len().saturating_sub(1 + lums.len() / 8)].1;
    let (fr, fg, fb) = samples[dark_i];
    let (br, bg, bb) = samples[light_i];
    // Manga bubbles are usually light; if sampling flipped, swap.
    let (mut fr, mut fg, mut fb, mut br, mut bg, mut bb) =
        if luminance(fr, fg, fb) > luminance(br, bg, bb) {
            (br, bg, bb, fr, fg, fb)
        } else {
            (fr, fg, fb, br, bg, bb)
        };
    // Ink-blot / SFX bubbles can sample near-black for both; force contrast.
    if (luminance(br, bg, bb) - luminance(fr, fg, fb)).abs() < 40.0 {
        if luminance(br, bg, bb) < 80.0 {
            // Dark fill → light text
            fr = 245;
            fg = 245;
            fb = 245;
            br = br.min(40);
            bg = bg.min(40);
            bb = bb.min(40);
        } else {
            fr = 20;
            fg = 20;
            fb = 20;
        }
    }
    // Kill JPEG/scan cast on near-grey ink (same idea as mit48 snap_render_color).
    let chroma = |r: u8, g: u8, b: u8| r.max(g).max(b).saturating_sub(r.min(g).min(b));
    if chroma(fr, fg, fb) < 50 {
        if luminance(fr, fg, fb) <= 150.0 {
            fr = 0;
            fg = 0;
            fb = 0;
        } else {
            fr = 255;
            fg = 255;
            fb = 255;
        }
    }
    (Some(hex_rgb(fr, fg, fb)), Some(hex_rgb(br, bg, bb)))
}
