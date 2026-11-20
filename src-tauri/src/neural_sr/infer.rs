use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use half::f16;
use image::{ImageBuffer, Rgba, RgbaImage};
use ort::session::{Session, SessionOutputs};
use ort::value::{Tensor, TensorElementType};
use tauri::{AppHandle, WebviewWindow};

use super::cache::resolve_results_root;
use super::cuda::cuda_ep_usable;
use super::models::{model_onnx_path, model_scale};
use super::ort_env::{build_session, ensure_ort, is_cuda_runtime_error};
use super::results::{self, CANCELLED};
use super::runtime::installed_flavor;

static CUDA_EP_OK: AtomicBool = AtomicBool::new(true);
static RUNNING_JOB: AtomicU64 = AtomicU64::new(0);
static CANCEL_JOB: AtomicU64 = AtomicU64::new(0);

fn job_cancelled(job_id: u64) -> bool {
    job_id != 0 && CANCEL_JOB.load(Ordering::Relaxed) == job_id
}

fn begin_job(job_id: u64) {
    RUNNING_JOB.store(job_id, Ordering::Relaxed);
}

fn finish_job(job_id: u64) {
    let _ = RUNNING_JOB.compare_exchange(job_id, 0, Ordering::Relaxed, Ordering::Relaxed);
}

fn decode_rgba(bytes: &[u8]) -> Result<RgbaImage, String> {
    let img = image::load_from_memory(bytes).map_err(|err| err.to_string())?;
    Ok(img.to_rgba8())
}

fn image_to_nchw_f32(img: &RgbaImage) -> (Vec<usize>, Vec<f32>) {
    let (w, h) = img.dimensions();
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for c in 0..3 {
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x, y).0;
                data.push(f32::from(p[c]) / 255.0);
            }
        }
    }
    (vec![1, 3, h as usize, w as usize], data)
}

fn image_to_nchw_f16(img: &RgbaImage) -> (Vec<usize>, Vec<f16>) {
    let (shape, f32s) = image_to_nchw_f32(img);
    let data = f32s.into_iter().map(f16::from_f32).collect();
    (shape, data)
}

fn input_is_float16(session: &Session) -> bool {
    session
        .inputs()
        .first()
        .and_then(|input| input.dtype().tensor_type())
        .is_some_and(|ty| ty == TensorElementType::Float16)
}

fn rgba_from_planar(shape: &[usize], sample: impl Fn(usize) -> f32) -> Result<RgbaImage, String> {
    if shape.len() != 4 {
        return Err(format!("unsupported output shape: {shape:?}"));
    }
    let (h, w, nchw) = if shape[1] == 3 {
        (shape[2], shape[3], true)
    } else if shape[3] == 3 {
        (shape[1], shape[2], false)
    } else {
        return Err(format!("unsupported output shape: {shape:?}"));
    };
    let mut out = ImageBuffer::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = if nchw {
                let r = sample(0 * h * w + y * w + x);
                let g = sample(1 * h * w + y * w + x);
                let b = sample(2 * h * w + y * w + x);
                (r, g, b)
            } else {
                let base = (y * w + x) * 3;
                (sample(base), sample(base + 1), sample(base + 2))
            };
            out.put_pixel(
                x as u32,
                y as u32,
                Rgba([
                    (r.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (g.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (b.clamp(0.0, 1.0) * 255.0).round() as u8,
                    255,
                ]),
            );
        }
    }
    Ok(out)
}

fn extract_output_rgba(outputs: &SessionOutputs<'_>) -> Result<RgbaImage, String> {
    let (_name, value) = outputs
        .iter()
        .next()
        .ok_or_else(|| "model returned no outputs".to_string())?;
    if let Ok(array) = value.try_extract_array::<f16>() {
        let shape = array.shape().to_vec();
        let flat: Vec<f16> = array.iter().copied().collect();
        return rgba_from_planar(&shape, |i| flat[i].to_f32());
    }
    let array = value
        .try_extract_array::<f32>()
        .map_err(|err| err.to_string())?;
    let shape = array.shape().to_vec();
    let flat: Vec<f32> = array.iter().copied().collect();
    rgba_from_planar(&shape, |i| flat[i])
}

fn want_cuda_ep(app: &AppHandle) -> bool {
    CUDA_EP_OK.load(Ordering::Relaxed) && cuda_ep_usable(installed_flavor(app))
}

fn run_tile(session: &mut Session, tile: &RgbaImage, use_f16: bool) -> Result<RgbaImage, String> {
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| "model has no inputs".to_string())?;
    let outputs = if use_f16 {
        let (shape, data) = image_to_nchw_f16(tile);
        let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
        session
            .run(ort::inputs![input_name.as_str() => tensor])
            .map_err(|err| format!("ORT run failed: {err}"))?
    } else {
        let (shape, data) = image_to_nchw_f32(tile);
        let tensor = Tensor::from_array((shape, data)).map_err(|err| err.to_string())?;
        session
            .run(ort::inputs![input_name.as_str() => tensor])
            .map_err(|err| format!("ORT run failed: {err}"))?
    };
    extract_output_rgba(&outputs)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InferProgress {
    label: String,
    done: u32,
    total: u32,
    percent: u32,
    finished: bool,
}

fn emit_infer_progress(_app: &AppHandle, label: &str, done: u32, total: u32, finished: bool) {
    let percent = if total == 0 {
        100
    } else {
        ((u64::from(done) * 100) / u64::from(total.max(1))).min(100) as u32
    };
    let payload = InferProgress {
        label: label.to_string(),
        done,
        total,
        percent,
        finished,
    };
    if let Some(owner) = crate::app_log::current_owner() {
        crate::app_log::emit_to_log_window(&owner, "neural-infer-progress", payload);
    }
}

fn upscale_tiled(
    app: &AppHandle,
    label: &str,
    session: &mut Session,
    src: &RgbaImage,
    scale: u32,
    job_id: u64,
) -> Result<RgbaImage, String> {
    let use_f16 = input_is_float16(session);
    let tile = 128u32;
    let pad = 8u32;
    let (sw, sh) = src.dimensions();
    let out_w = sw.saturating_mul(scale).max(1);
    let out_h = sh.saturating_mul(scale).max(1);
    let mut out = ImageBuffer::new(out_w, out_h);
    let tiles_x = sw.div_ceil(tile).max(1);
    let tiles_y = sh.div_ceil(tile).max(1);
    let total = tiles_x.saturating_mul(tiles_y);
    let mut done = 0u32;
    let mut last_emit = Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or_else(Instant::now);

    emit_infer_progress(app, label, 0, total, false);

    let mut y = 0u32;
    while y < sh {
        let mut x = 0u32;
        while x < sw {
            if job_cancelled(job_id) {
                emit_infer_progress(app, label, done, total, true);
                return Err(CANCELLED.into());
            }
            let x0 = x.saturating_sub(pad);
            let y0 = y.saturating_sub(pad);
            let x1 = (x + tile + pad).min(sw);
            let y1 = (y + tile + pad).min(sh);
            let tile_img = image::imageops::crop_imm(src, x0, y0, x1 - x0, y1 - y0).to_image();
            let up = run_tile(session, &tile_img, use_f16)?;
            let left = (x - x0) * scale;
            let top = (y - y0) * scale;
            let copy_w = ((x1.min(x + tile) - x) * scale).min(out_w.saturating_sub(x * scale));
            let copy_h = ((y1.min(y + tile) - y) * scale).min(out_h.saturating_sub(y * scale));
            for dy in 0..copy_h {
                for dx in 0..copy_w {
                    let sx = left + dx;
                    let sy = top + dy;
                    if sx < up.width() && sy < up.height() {
                        out.put_pixel(x * scale + dx, y * scale + dy, *up.get_pixel(sx, sy));
                    }
                }
            }
            done = done.saturating_add(1);
            let now = Instant::now();
            if done >= total
                || now.duration_since(last_emit).as_millis() >= 80
                || done == 1
            {
                emit_infer_progress(app, label, done, total, done >= total);
                last_emit = now;
            }
            x += tile;
        }
        y += tile;
    }
    if done < total || total == 0 {
        emit_infer_progress(app, label, total, total, true);
    }
    Ok(out)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpscaleResult {
    pub width: u32,
    pub height: u32,
    pub webp: Vec<u8>,
}

#[tauri::command]
pub fn cancel_neural_upscale(job_id: u64) {
    if job_id == 0 {
        let running = RUNNING_JOB.load(Ordering::Relaxed);
        if running != 0 {
            CANCEL_JOB.store(running, Ordering::Relaxed);
        }
        return;
    }
    CANCEL_JOB.store(job_id, Ordering::Relaxed);
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeuralCachedWebp {
    pub webp: Vec<u8>,
}

#[tauri::command]
pub async fn neural_result_load(
    app: AppHandle,
    _asset_id: String,
    cache_key: String,
) -> Result<Option<NeuralCachedWebp>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolve_results_root(&app)?;
        let Some(webp) = results::load_cached_webp(&root, &cache_key)? else {
            return Ok(None);
        };
        Ok(Some(NeuralCachedWebp { webp }))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn neural_upscale(
    window: WebviewWindow,
    app: AppHandle,
    asset_id: String,
    image_png: Vec<u8>,
    target_width: u32,
    target_height: u32,
    cache_key: Option<String>,
    job_id: Option<u64>,
) -> Result<UpscaleResult, String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        crate::app_log::scope(owner, || {
            neural_upscale_sync(
                app,
                asset_id,
                image_png,
                target_width,
                target_height,
                cache_key.unwrap_or_default(),
                job_id.unwrap_or(0),
            )
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

fn neural_upscale_sync(
    app: AppHandle,
    asset_id: String,
    image_png: Vec<u8>,
    target_width: u32,
    target_height: u32,
    cache_key: String,
    job_id: u64,
) -> Result<UpscaleResult, String> {
    begin_job(job_id);
    let result = neural_upscale_inner(
        app,
        asset_id,
        image_png,
        target_width,
        target_height,
        cache_key,
        job_id,
    );
    finish_job(job_id);
    result
}

fn cache_label(cache_key: &str) -> &str {
    Path::new(cache_key)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(cache_key)
}

fn neural_upscale_inner(
    app: AppHandle,
    asset_id: String,
    image_png: Vec<u8>,
    target_width: u32,
    target_height: u32,
    cache_key: String,
    job_id: u64,
) -> Result<UpscaleResult, String> {
    if job_cancelled(job_id) {
        return Err(CANCELLED.into());
    }
    let label = cache_label(&cache_key);
    let results_root = resolve_results_root(&app)?;
    if !cache_key.is_empty() {
        if let Some(webp) = results::load_cached_webp(&results_root, &cache_key)? {
            // Avoid RGBA round-trip on hit; frontend decodes WebP the same as disk loads.
            crate::app_log::info(
                "neural",
                format!("cache-hit {label} bytes={}", webp.len()),
            );
            return Ok(UpscaleResult {
                width: 0,
                height: 0,
                webp,
            });
        }
    }
    if job_cancelled(job_id) {
        return Err(CANCELLED.into());
    }

    let env = ensure_ort(&app)?;
    let onnx = model_onnx_path(&app, &asset_id)
        .ok_or_else(|| format!("model '{asset_id}' is not installed"))?;
    let scale = model_scale(&app, &asset_id).round().max(1.0) as u32;
    let src = decode_rgba(&image_png)?;
    let (src_w, src_h) = src.dimensions();

    let use_cuda = want_cuda_ep(&app);
    crate::app_log::info(
        "neural",
        format!(
            "start {label} asset={asset_id} src={src_w}x{src_h} threshold={target_width}x{target_height} scale={scale}x cuda={use_cuda}"
        ),
    );
    let started = Instant::now();
    let mut session = build_session(env, &onnx, use_cuda)?;
    let tiled = |session: &mut Session| {
        upscale_tiled(&app, &label, session, &src, scale, job_id)
    };
    let upscaled = match tiled(&mut session) {
        Ok(image) => image,
        Err(err) if err == CANCELLED => {
            crate::app_log::info("neural", format!("cancelled {label}"));
            return Err(err);
        }
        Err(err) if use_cuda && is_cuda_runtime_error(&err) => {
            crate::app_log::warn(
                "neural",
                format!("cuda failed, fallback to cpu: {err}"),
            );
            CUDA_EP_OK.store(false, Ordering::Relaxed);
            session = build_session(env, &onnx, false)?;
            tiled(&mut session)?
        }
        Err(err) => {
            emit_infer_progress(&app, &label, 0, 1, true);
            crate::app_log::error("neural", format!("failed {label}: {err}"));
            return Err(err);
        }
    };
    if job_cancelled(job_id) {
        crate::app_log::info("neural", format!("cancelled {label}"));
        return Err(CANCELLED.into());
    }
    // Keep model-native scale output (usually larger than the threshold). No Lanczos shrink.
    let (width, height) = upscaled.dimensions();
    let webp = results::encode_webp(&upscaled)?;
    if job_cancelled(job_id) {
        crate::app_log::info("neural", format!("cancelled {label}"));
        return Err(CANCELLED.into());
    }
    let _ = results::store_cached_webp(&app, &cache_key, &webp);
    crate::app_log::info(
        "neural",
        format!(
            "done {label} out={width}x{height} bytes={} elapsed={:.1}ms",
            webp.len(),
            started.elapsed().as_secs_f64() * 1000.0
        ),
    );
    Ok(UpscaleResult {
        width,
        height,
        webp,
    })
}
