//! Manga OCR (ViT encoder + autoregressive decoder) via ONNX.
//!
//! Saber `manga_ocr`: recognize one crop per bubble (TextBlock AABB).

use ort::value::Tensor;
use tauri::AppHandle;

use crate::neural_sr::ort_env::open_session;
use crate::ocr_assets;

use super::detector::{self, TextBlock};
use super::image_util::{
    apply_sampled_colors, crop_norm, decode_rgba, nchw_rgb_norm, region, to_rgb_resized,
};
use super::OcrRegion;

const PACK: &str = "ocr-manga-ocr";
const CLS: i64 = 2;
const SEP: i64 = 3;
const MAX_LEN: usize = 64;

fn load_vocab(path: &std::path::Path) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok(raw.lines().map(|l| l.trim_end_matches('\r').to_string()).collect())
}

fn recognize_crop(app: &AppHandle, crop: &image::RgbaImage, vocab: &[String]) -> Result<String, String> {
    let enc_path = ocr_assets::pack_file(app, PACK, "encoder_model.onnx")?;
    let dec_path = ocr_assets::pack_file(app, PACK, "decoder_model.onnx")?;
    let resized = to_rgb_resized(crop, 224, 224);
    let (shape, data) = nchw_rgb_norm(
        &resized,
        [0.485, 0.456, 0.406],
        [0.229, 0.224, 0.225],
    );

    let mut enc = open_session(app, &enc_path)?;
    let enc_in = enc
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "manga encoder has no inputs".to_string())?;
    let pixel = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
    let enc_out = enc
        .run(ort::inputs![enc_in.as_str() => pixel])
        .map_err(|err| format!("manga encoder failed: {err}"))?;
    let hidden_val = enc_out
        .iter()
        .next()
        .ok_or_else(|| "manga encoder has no outputs".to_string())?
        .1;
    let hidden = hidden_val
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let h_shape = hidden.shape().to_vec();
    let h_data: Vec<f32> = hidden.iter().copied().collect();

    let mut dec = open_session(app, &dec_path)?;
    let dec_inputs: Vec<String> = dec.inputs().iter().map(|i| i.name().to_string()).collect();
    let ids_name = dec_inputs
        .iter()
        .find(|n| n.contains("input_ids") || *n == "input_ids")
        .cloned()
        .or_else(|| dec_inputs.first().cloned())
        .ok_or_else(|| "manga decoder missing input_ids".to_string())?;
    let enc_name = dec_inputs
        .iter()
        .find(|n| n.contains("encoder_hidden") || n.contains("hidden"))
        .cloned()
        .or_else(|| dec_inputs.get(1).cloned())
        .ok_or_else(|| "manga decoder missing encoder_hidden_states".to_string())?;

    let mut tokens = vec![CLS];
    for _ in 0..MAX_LEN {
        let ids_tensor =
            Tensor::from_array(([1usize, tokens.len()], tokens.clone())).map_err(|err| err.to_string())?;
        let enc_tensor =
            Tensor::from_array((h_shape.clone(), h_data.clone())).map_err(|err| err.to_string())?;
        let outputs = dec
            .run(ort::inputs![
                ids_name.as_str() => ids_tensor,
                enc_name.as_str() => enc_tensor
            ])
            .map_err(|err| format!("manga decoder failed: {err}"))?;
        let logits_val = outputs
            .iter()
            .next()
            .ok_or_else(|| "manga decoder has no outputs".to_string())?
            .1;
        let logits = logits_val
            .try_extract_array::<f32>()
            .map_err(|err| err.to_string())?;
        let shape = logits.shape().to_vec();
        let (seq, vocab_size) = match shape.as_slice() {
            [_, s, v] => (*s, *v),
            [s, v] => (*s, *v),
            _ => return Err(format!("unexpected decoder logits: {shape:?}")),
        };
        let flat: Vec<f32> = logits.iter().copied().collect();
        let last = seq.saturating_sub(1);
        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for c in 0..vocab_size {
            let v = flat[last * vocab_size + c];
            if v > best_v {
                best_v = v;
                best_i = c;
            }
        }
        let next = best_i as i64;
        tokens.push(next);
        if next == SEP {
            break;
        }
    }

    let mut out = String::new();
    for &tok in tokens.iter().skip(1) {
        if tok == SEP || tok == CLS {
            continue;
        }
        if let Some(piece) = vocab.get(tok as usize) {
            let p = piece.strip_prefix("##").unwrap_or(piece.as_str());
            if piece.starts_with('[') && piece.ends_with(']') {
                continue;
            }
            out.push_str(p);
        }
    }
    Ok(out)
}

/// Saber path: one MangaOCR call per bubble AABB.
pub fn recognize_on_blocks(
    app: &AppHandle,
    bytes: &[u8],
    blocks: &[TextBlock],
) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    let vocab_path = ocr_assets::pack_file(app, PACK, "vocab.txt")?;
    let vocab = load_vocab(&vocab_path)?;
    let img = decode_rgba(bytes)?;
    let mut regions = detector::regions_from_block_ocr(blocks, |b| {
        let draft = region("tmp", b.x, b.y, b.w, b.h, String::new());
        let crop = crop_norm(&img, &draft);
        recognize_crop(app, &crop, &vocab).unwrap_or_default()
    });
    apply_sampled_colors(&img, &mut regions);
    Ok(regions)
}

/// Minimal fallback when no Saber detector blocks are available.
pub fn recognize_fallback(app: &AppHandle, bytes: &[u8]) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    let vocab_path = ocr_assets::pack_file(app, PACK, "vocab.txt")?;
    let vocab = load_vocab(&vocab_path)?;
    let img = decode_rgba(bytes)?;
    let text = recognize_crop(app, &img, &vocab)?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![region("r0", 0.1, 0.1, 0.8, 0.3, text)])
}
