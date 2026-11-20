#![cfg(windows)]

use super::image_util::{decode_rgba, sample_region_colors};
use super::OcrRegion;
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader, RgbaImage};
use std::cmp::Ordering;
use std::io::Cursor;
use windows::core::{Error as WinError, HSTRING};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap,
};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

/// Windows.Media.Ocr fails with 0xC00D7170 on large / bad bitmaps.
const MAX_OCR_SIDE: u32 = 1600;

#[derive(Clone)]
struct WordBox {
    text: String,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

fn map_err(err: WinError) -> String {
    err.to_string()
}

fn is_auto_source(source_lang: &str) -> bool {
    let s = source_lang.trim().to_lowercase();
    s.is_empty() || s == "auto"
}

fn language_from_tags(tags: &[&str]) -> Option<Language> {
    for tag in tags {
        if let Ok(lang) = Language::CreateLanguage(&HSTRING::from(*tag)) {
            if OcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
                return Some(lang);
            }
        }
    }
    None
}

fn first_installed_language() -> Result<Language, String> {
    let langs = OcrEngine::AvailableRecognizerLanguages().map_err(map_err)?;
    let iter = langs.First().map_err(map_err)?;
    iter.Current()
        .map_err(|_| "???????? OCR ???".to_string())
}

fn installed_matching(short: &str) -> Option<Language> {
    let langs = OcrEngine::AvailableRecognizerLanguages().ok()?;
    let size = langs.Size().ok()?;
    let needle = short.to_lowercase();
    for i in 0..size {
        let lang = langs.GetAt(i).ok()?;
        let tag = lang.LanguageTag().ok()?.to_string().to_lowercase();
        if tag == needle || tag.starts_with(&format!("{needle}-")) {
            return Some(lang);
        }
    }
    None
}

fn pick_language(source_lang: &str) -> Result<Language, String> {
    let raw = source_lang.trim();
    let lower = raw.to_lowercase();
    let short = lower.split('-').next().unwrap_or(&lower);

    let extra: &[&str] = match short {
        "zh" if lower.contains("tw") || lower.contains("hk") || lower.contains("hant") => {
            &["zh-Hant", "zh-TW", "zh-HK"]
        }
        "zh" => &["zh-Hans", "zh-CN", "zh"],
        "ja" => &["ja", "ja-JP"],
        "ko" => &["ko", "ko-KR"],
        "en" => &["en", "en-US", "en-GB"],
        "pt" if lower.contains("br") => &["pt-BR", "pt"],
        "pt" => &["pt-PT", "pt-BR", "pt"],
        "fr" => &["fr-FR", "fr-CA", "fr"],
        "es" => &["es-ES", "es-MX", "es"],
        "nb" | "no" => &["nb-NO", "nb", "no"],
        _ => &[],
    };

    let mut tags: Vec<&str> = Vec::with_capacity(6);
    tags.push(raw);
    if lower != raw {
        tags.push(lower.as_str());
    }
    tags.push(short);
    tags.extend_from_slice(extra);

    if let Some(lang) = language_from_tags(tags.as_slice()) {
        return Ok(lang);
    }
    if let Some(lang) = installed_matching(short) {
        return Ok(lang);
    }
    first_installed_language()
}

fn create_engine(source_lang: &str) -> Result<OcrEngine, String> {
    if is_auto_source(source_lang) {
        if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
            return Ok(engine);
        }
    }
    let lang = pick_language(source_lang)?;
    OcrEngine::TryCreateFromLanguage(&lang).map_err(map_err)
}

fn even_side(v: u32) -> u32 {
    let n = v.max(2);
    n - (n % 2)
}

fn software_bitmap_from_image_bytes(
    bytes: &[u8],
) -> Result<(SoftwareBitmap, u32, u32, RgbaImage), String> {
    let decoded = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let (src_w, src_h) = decoded.dimensions();
    let max_side = src_w.max(src_h).max(1);
    let scaled = if max_side > MAX_OCR_SIDE {
        decoded.resize(MAX_OCR_SIDE, MAX_OCR_SIDE, FilterType::Triangle)
    } else {
        decoded
    };
    let (scaled_w, scaled_h) = scaled.dimensions();
    let scaled_w = even_side(scaled_w.max(2));
    let scaled_h = even_side(scaled_h.max(2));
    let scaled = if scaled.width() != scaled_w || scaled.height() != scaled_h {
        scaled.resize_exact(scaled_w, scaled_h, FilterType::Triangle)
    } else {
        scaled
    };
    let rgba = scaled.to_rgba8();

    let mut png = Cursor::new(Vec::new());
    scaled
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let png_bytes = png.into_inner();

    let stream = InMemoryRandomAccessStream::new().map_err(map_err)?;
    {
        let writer = DataWriter::CreateDataWriter(&stream).map_err(map_err)?;
        writer.WriteBytes(&png_bytes).map_err(map_err)?;
        writer.StoreAsync().map_err(map_err)?.join().map_err(map_err)?;
        let _ = writer.DetachStream().map_err(map_err)?;
    }
    stream.Seek(0).map_err(map_err)?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(map_err)?
        .join()
        .map_err(map_err)?;
    let bitmap = decoder
        .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied)
        .map_err(map_err)?
        .join()
        .map_err(map_err)?;
    let bw = bitmap.PixelWidth().map_err(map_err)? as u32;
    let bh = bitmap.PixelHeight().map_err(map_err)? as u32;
    let width = if bw > 0 { bw } else { scaled_w };
    let height = if bh > 0 { bh } else { scaled_h };
    Ok((bitmap, width, height, rgba))
}

fn median_f64(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) * 0.5
    } else {
        values[mid]
    }
}

fn box_gap_x(a: &WordBox, b: &WordBox) -> f64 {
    if a.x1 < b.x0 {
        b.x0 - a.x1
    } else if b.x1 < a.x0 {
        a.x0 - b.x1
    } else {
        0.0
    }
}

fn box_gap_y(a: &WordBox, b: &WordBox) -> f64 {
    if a.y1 < b.y0 {
        b.y0 - a.y1
    } else if b.y1 < a.y0 {
        a.y0 - b.y1
    } else {
        0.0
    }
}

fn overlap_1d(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Saber-like proximity merge: only glue words on the same line/column.
fn can_merge_words(a: &WordBox, b: &WordBox, gap: f64) -> bool {
    let wa = (a.x1 - a.x0).max(1.0);
    let ha = (a.y1 - a.y0).max(1.0);
    let wb = (b.x1 - b.x0).max(1.0);
    let hb = (b.y1 - b.y0).max(1.0);
    let font = ha.min(hb);
    if font <= 0.0 {
        return false;
    }
    if ha.max(hb) / font > 2.6 {
        return false;
    }
    let gx = box_gap_x(a, b);
    let gy = box_gap_y(a, b);
    let v_overlap = overlap_1d(a.y0, a.y1, b.y0, b.y1);
    let h_overlap = overlap_1d(a.x0, a.x1, b.x0, b.x1);
    if v_overlap >= 0.35 * font && gx <= gap * 1.15 {
        return true;
    }
    if h_overlap >= 0.35 * font.min(wa.min(wb)) && gy <= gap * 1.35 {
        return true;
    }
    false
}

fn cluster_words(words: &[WordBox]) -> Vec<Vec<usize>> {
    let n = words.len();
    if n == 0 {
        return Vec::new();
    }
    let mut heights: Vec<f64> = words.iter().map(|w| (w.y1 - w.y0).max(1.0)).collect();
    let median_h = median_f64(&mut heights).max(1.0);
    let gap = median_h * 1.25;

    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut x = i;
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if can_merge_words(&words[i], &words[j], gap) {
                let a = find(&mut parent, i);
                let b = find(&mut parent, j);
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let root = find(&mut parent, i);
        groups[root].push(i);
    }
    groups.into_iter().filter(|g| !g.is_empty()).collect()
}

fn push_cluster(
    regions: &mut Vec<OcrRegion>,
    id: String,
    words: &[WordBox],
    indices: &[usize],
    img_w: f64,
    img_h: f64,
    rgba: &RgbaImage,
) {
    if indices.is_empty() {
        return;
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for &i in indices {
        let w = &words[i];
        min_x = min_x.min(w.x0);
        min_y = min_y.min(w.y0);
        max_x = max_x.max(w.x1);
        max_y = max_y.max(w.y1);
    }
    let bw = (max_x - min_x).max(1.0);
    let bh = (max_y - min_y).max(1.0);

    let mut ordered: Vec<usize> = indices.to_vec();
    ordered.sort_by(|&a, &b| {
        words[a]
            .y0
            .partial_cmp(&words[b].y0)
            .unwrap_or(Ordering::Equal)
            .then(
                words[a]
                    .x0
                    .partial_cmp(&words[b].x0)
                    .unwrap_or(Ordering::Equal),
            )
    });
    let text = ordered
        .iter()
        .map(|&i| words[i].text.as_str())
        .collect::<Vec<_>>()
        .join("");
    if text.trim().is_empty() {
        return;
    }

    // Saber-style expand so strokes at the edge are covered.
    let pad_x = (bw * 0.10 + img_w * 0.003).min(img_w * 0.04);
    let pad_y = (bh * 0.12 + img_h * 0.003).min(img_h * 0.04);
    let x0 = (min_x - pad_x).max(0.0);
    let y0 = (min_y - pad_y).max(0.0);
    let x1 = (max_x + pad_x).min(img_w);
    let y1 = (max_y + pad_y).min(img_h);
    let rw = ((x1 - x0) / img_w).clamp(0.0, 1.0);
    let rh = ((y1 - y0) / img_h).clamp(0.0, 1.0);
    let rx = (x0 / img_w).clamp(0.0, 1.0);
    let ry = (y0 / img_h).clamp(0.0, 1.0);

    let word_area: f64 = indices
        .iter()
        .map(|&i| {
            let w = &words[i];
            (w.x1 - w.x0).max(0.0) * (w.y1 - w.y0).max(0.0)
        })
        .sum();
    let box_area = bw * bh;
    let fill = if box_area > 0.0 {
        word_area / box_area
    } else {
        0.0
    };
    // Reject page-sized false positives; keep room for multi-line speech bubbles.
    if rw > 0.7 || rh > 0.55 || rw * rh > 0.18 {
        return;
    }
    if fill < 0.15 && rw * rh > 0.04 {
        return;
    }
    if rw < 0.004 || rh < 0.003 {
        return;
    }

    let mut region = OcrRegion {
        id,
        x: rx,
        y: ry,
        w: rw,
        h: rh,
        text,
        translation: None,
        bg_color: None,
        fg_color: None,
        stroke_color: None,
        vertical: None,
    };
    let (fg, bg) = sample_region_colors(rgba, &region);
    region.fg_color = fg;
    region.bg_color = bg;
    regions.push(region);
}

pub fn recognize(bytes: &[u8], source_lang: &str) -> Result<(Vec<OcrRegion>, String), String> {
    let engine = create_engine(source_lang)?;
    let (bitmap, width, height, rgba) = software_bitmap_from_image_bytes(bytes)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(map_err)?
        .join()
        .map_err(map_err)?;

    let lines = result.Lines().map_err(map_err)?;
    let count = lines.Size().map_err(map_err)?;
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;

    // Per-line word lists only ? page-wide union-find merged every bubble into one box.
    let mut line_words: Vec<Vec<WordBox>> = Vec::new();
    let mut extent_x = w;
    let mut extent_y = h;
    for i in 0..count {
        let line = lines.GetAt(i).map_err(map_err)?;
        let word_list = line.Words().map_err(map_err)?;
        let word_count = word_list.Size().map_err(map_err)?;
        let mut words = Vec::new();
        for wi in 0..word_count {
            let word = word_list.GetAt(wi).map_err(map_err)?;
            let text = word.Text().map_err(map_err)?.to_string();
            if text.trim().is_empty() {
                continue;
            }
            let rect = word.BoundingRect().map_err(map_err)?;
            let x0 = f64::from(rect.X);
            let y0 = f64::from(rect.Y);
            let x1 = x0 + f64::from(rect.Width);
            let y1 = y0 + f64::from(rect.Height);
            extent_x = extent_x.max(x1);
            extent_y = extent_y.max(y1);
            words.push(WordBox {
                text,
                x0,
                y0,
                x1,
                y1,
            });
        }
        if !words.is_empty() {
            line_words.push(words);
        }
    }

    let sx = if extent_x > w * 1.05 { extent_x } else { w };
    let sy = if extent_y > h * 1.05 { extent_y } else { h };
    if (sx - w).abs() > 0.5 || (sy - h).abs() > 0.5 {
        let kx = w / sx;
        let ky = h / sy;
        for words in &mut line_words {
            for word in words {
                word.x0 *= kx;
                word.x1 *= kx;
                word.y0 *= ky;
                word.y1 *= ky;
            }
        }
    }

    let mut regions = Vec::new();
    let mut next_id = 1usize;
    for words in &line_words {
        // Split a sparse Windows "line" into tight gap clusters; keep dense lines whole.
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut word_area = 0.0;
        for word in words {
            min_x = min_x.min(word.x0);
            min_y = min_y.min(word.y0);
            max_x = max_x.max(word.x1);
            max_y = max_y.max(word.y1);
            word_area += (word.x1 - word.x0).max(0.0) * (word.y1 - word.y0).max(0.0);
        }
        let bw = (max_x - min_x).max(1.0);
        let bh = (max_y - min_y).max(1.0);
        let fill = word_area / (bw * bh);
        let groups = if fill >= 0.22 && bw / w <= 0.7 && bh / h <= 0.55 {
            vec![(0..words.len()).collect::<Vec<_>>()]
        } else {
            cluster_words(words)
        };
        for group in groups {
            push_cluster(
                &mut regions,
                format!("{next_id}"),
                words,
                &group,
                w,
                h,
                &rgba,
            );
            next_id += 1;
        }
    }

    regions.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    Ok((regions, "windows-ocr".into()))
}

fn recognize_text_only(bytes: &[u8], source_lang: &str) -> Result<String, String> {
    let engine = create_engine(source_lang)?;
    let (bitmap, _, _, _) = software_bitmap_from_image_bytes(bytes)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(map_err)?
        .join()
        .map_err(map_err)?;
    let lines = result.Lines().map_err(map_err)?;
    let count = lines.Size().map_err(map_err)?;
    let mut parts = Vec::new();
    for i in 0..count {
        let line = lines.GetAt(i).map_err(map_err)?;
        let text = line.Text().map_err(map_err)?.to_string();
        if !text.trim().is_empty() {
            parts.push(text);
        }
    }
    Ok(parts.join(""))
}

/// Detector boxes look usable (not just a thin page-edge disclaimer strip).
fn detector_regions_usable(regions: &[OcrRegion]) -> bool {
    if regions.is_empty() {
        return false;
    }
    let body = regions
        .iter()
        .filter(|r| r.h >= 0.02 && r.w * r.h >= 0.0015 && r.y + r.h * 0.5 > 0.04)
        .count();
    body >= 1 || regions.len() >= 4
}

/// Consume Saber TextBlocks; Windows OCR only recognizes line/bubble crops.
pub fn recognize_on_blocks(
    _app: &tauri::AppHandle,
    bytes: &[u8],
    source_lang: &str,
    blocks: &[super::detector::TextBlock],
) -> Result<(Vec<OcrRegion>, String), String> {
    use super::detector;
    use super::image_util::{crop_norm, region as make_region};
    use image::ImageEncoder;

    if blocks.is_empty() {
        return recognize(bytes, source_lang);
    }
    let img = decode_rgba(bytes)?;
    let ocr_line = |line: &super::detector::DetBox| -> String {
        let cx = line.x + line.w * 0.5;
        let cy = line.y + line.h * 0.5;
        let nw = (line.w * 1.12).min(1.0);
        let nh = (line.h * 1.14).min(1.0);
        let nx = (cx - nw * 0.5).clamp(0.0, 1.0);
        let ny = (cy - nh * 0.5).clamp(0.0, 1.0);
        let nw = (nx + nw).min(1.0) - nx;
        let nh = (ny + nh).min(1.0) - ny;
        if nw < 0.004 || nh < 0.003 {
            return String::new();
        }
        let geo = make_region("tmp", nx, ny, nw, nh, String::new());
        let crop = crop_norm(&img, &geo);
        let mut png = Cursor::new(Vec::new());
        {
            let (cw, ch) = crop.dimensions();
            let encoder = image::codecs::png::PngEncoder::new(&mut png);
            if encoder
                .write_image(crop.as_raw(), cw, ch, image::ExtendedColorType::Rgba8)
                .is_err()
            {
                return String::new();
            }
        }
        recognize_text_only(&png.into_inner(), source_lang).unwrap_or_default()
    };
    let mut regions = detector::regions_from_line_ocr(blocks, ocr_line);
    regions.retain(|r| {
        r.w <= 0.7 && r.h <= 0.55 && r.w * r.h <= 0.18 && r.w >= 0.004 && r.h >= 0.003
    });
    for r in &mut regions {
        let (fg, bg) = sample_region_colors(&img, r);
        r.fg_color = fg;
        r.bg_color = bg;
    }
    if !detector_regions_usable(&regions) {
        return recognize(bytes, source_lang);
    }
    Ok((regions, "windows-ocr+detector".into()))
}
