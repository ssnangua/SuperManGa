//! PaddleOCR / RapidOCR PP-OCRv6 medium ONNX pipeline.
//!
//! Recognition is dispatched from [`super::ocr_page_bytes`] after a single
//! Saber detect pass; this module remains a thin pack helper.

#![allow(dead_code)]

use tauri::AppHandle;

use crate::ocr_assets;

use super::detector::TextBlock;
use super::rapid_ocr;
use super::OcrRegion;

const PACK: &str = "ocr-paddle";

pub fn recognize_on_blocks(
    app: &AppHandle,
    bytes: &[u8],
    blocks: &[TextBlock],
) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    rapid_ocr::recognize_on_text_blocks(app, bytes, PACK, blocks)
}

pub fn recognize_fallback(app: &AppHandle, bytes: &[u8]) -> Result<Vec<OcrRegion>, String> {
    if !ocr_assets::is_pack_installed(app, PACK) {
        return Err(format!("OCR_MODELS_REQUIRED:{PACK}"));
    }
    rapid_ocr::recognize_pack(app, bytes, PACK)
}
