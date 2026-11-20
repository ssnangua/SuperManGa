use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

const LIMIT: usize = 512 * 1024;
/// ~24fps is enough for a seek-bar buffer; higher rates mostly waste IPC.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(42);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMetric {
    pub session_id: u64,
    pub index: usize,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_path: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMetricsBatch {
    pub session_id: u64,
    pub pages: Vec<PageMetric>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureProgress {
    pub session_id: u64,
    pub done: usize,
    pub total: usize,
    pub finished: bool,
}

pub struct ProgressTicker {
    app: AppHandle,
    session_id: u64,
    total: usize,
    last_emit: Instant,
}

impl ProgressTicker {
    pub fn new(app: AppHandle, session_id: u64, total: usize) -> Self {
        emit_progress(&app, session_id, 0, total, false);
        Self {
            app,
            session_id,
            total,
            last_emit: Instant::now(),
        }
    }

    pub fn update(&mut self, done: usize) {
        if done >= self.total || self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            emit_progress(&self.app, self.session_id, done, self.total, false);
            self.last_emit = Instant::now();
        }
    }

    pub fn finish(&self) {
        emit_progress(&self.app, self.session_id, self.total, self.total, true);
    }
}

enum Probe {
    Done(u32, u32),
    Need(usize),
    Unknown,
}

pub fn file(path: &Path) -> Option<(u32, u32)> {
    let file = File::open(path).ok()?;
    reader(file)
}

pub fn bytes(data: &[u8]) -> Option<(u32, u32)> {
    match probe(data) {
        Probe::Done(width, height) => Some((width, height)),
        _ => None,
    }
}

pub fn reader(mut input: impl Read) -> Option<(u32, u32)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        match probe(&buf) {
            Probe::Done(width, height) => return Some((width, height)),
            Probe::Unknown => return None,
            Probe::Need(need) if need > buf.len() && buf.len() < LIMIT => {
                let want = need.saturating_sub(buf.len()).clamp(1, tmp.len());
                let n = input.read(&mut tmp[..want]).ok()?;
                if n == 0 {
                    return None;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            Probe::Need(_) => return None,
        }
    }
}

pub fn emit(
    app: &AppHandle,
    session_id: u64,
    index: usize,
    size: (u32, u32),
    name: Option<String>,
) {
    let rel_path = name.clone();
    let title = name.as_ref().map(|rel| super::names::file_title(rel));
    let _ = app.emit(
        "page-metric",
        PageMetric {
            session_id,
            index,
            width: size.0,
            height: size.1,
            name: title,
            rel_path,
        },
    );
}

pub fn emit_progress(app: &AppHandle, session_id: u64, done: usize, total: usize, finished: bool) {
    let _ = app.emit(
        "measure-progress",
        MeasureProgress {
            session_id,
            done,
            total,
            finished,
        },
    );
}

pub fn emit_batch(app: &AppHandle, session_id: u64, pages: Vec<PageMetric>) {
    if pages.is_empty() {
        return;
    }
    let _ = app.emit(
        "page-metrics",
        PageMetricsBatch { session_id, pages },
    );
}

pub fn metric(
    session_id: u64,
    index: usize,
    size: (u32, u32),
    name: Option<String>,
) -> PageMetric {
    let rel_path = name.clone();
    let title = name.as_ref().map(|rel| super::names::file_title(rel));
    PageMetric {
        session_id,
        index,
        width: size.0,
        height: size.1,
        name: title,
        rel_path,
    }
}

fn probe(data: &[u8]) -> Probe {
    if data.len() >= 8 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return probe_png(data);
    }
    if data.len() >= 6 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return probe_gif(data);
    }
    if data.len() >= 2 && data.starts_with(b"BM") {
        return probe_bmp(data);
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        return probe_webp(data);
    }
    if data.len() >= 2 && data.starts_with(&[0xff, 0xd8]) {
        return probe_jpeg(data);
    }
    if data.len() < 12 {
        Probe::Need(12)
    } else {
        Probe::Unknown
    }
}

fn probe_png(data: &[u8]) -> Probe {
    if data.len() < 24 {
        return Probe::Need(24);
    }
    if &data[12..16] != b"IHDR" {
        return Probe::Unknown;
    }
    done(
        u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
        u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
    )
}

fn probe_gif(data: &[u8]) -> Probe {
    if data.len() < 10 {
        return Probe::Need(10);
    }
    done(
        u16::from_le_bytes([data[6], data[7]]) as u32,
        u16::from_le_bytes([data[8], data[9]]) as u32,
    )
}

fn probe_bmp(data: &[u8]) -> Probe {
    if data.len() < 26 {
        return Probe::Need(26);
    }
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    done(width.unsigned_abs(), height.unsigned_abs())
}

fn probe_webp(data: &[u8]) -> Probe {
    // RIFF(4) + size(4) + WEBP(4) + chunk tag(4)
    if data.len() < 16 {
        return Probe::Need(16);
    }
    let tag = &data[12..16];
    if tag == b"VP8X" {
        if data.len() < 30 {
            return Probe::Need(30);
        }
        let width = 1 + u32::from_le_bytes([data[24], data[25], data[26], 0]);
        let height = 1 + u32::from_le_bytes([data[27], data[28], data[29], 0]);
        return done(width, height);
    }
    if tag == b"VP8 " {
        if data.len() < 30 {
            return Probe::Need(30);
        }
        if data[23] != 0x9d || data[24] != 0x01 || data[25] != 0x2a {
            return Probe::Unknown;
        }
        let width = u16::from_le_bytes([data[26], data[27]]) as u32 & 0x3fff;
        let height = u16::from_le_bytes([data[28], data[29]]) as u32 & 0x3fff;
        return done(width, height);
    }
    if tag == b"VP8L" {
        if data.len() < 25 {
            return Probe::Need(25);
        }
        if data[20] != 0x2f {
            return Probe::Unknown;
        }
        let bits = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
        let width = (bits & 0x3fff) + 1;
        let height = ((bits >> 14) & 0x3fff) + 1;
        return done(width, height);
    }
    Probe::Unknown
}

fn probe_jpeg(data: &[u8]) -> Probe {
    let mut index = 2;
    while index < data.len() {
        if data[index] != 0xff {
            return Probe::Unknown;
        }
        while index < data.len() && data[index] == 0xff {
            index += 1;
        }
        if index >= data.len() {
            return Probe::Need(data.len() + 4);
        }
        let marker = data[index];
        index += 1;
        if marker == 0xd9 || marker == 0xda {
            return Probe::Unknown;
        }
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        if index + 2 > data.len() {
            return Probe::Need(index + 2);
        }
        let len = u16::from_be_bytes([data[index], data[index + 1]]) as usize;
        if len < 2 {
            return Probe::Unknown;
        }
        if index + len > data.len() {
            return Probe::Need(index + len);
        }
        if matches!(
            marker,
            0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
        ) {
            if len < 7 {
                return Probe::Unknown;
            }
            let height = u16::from_be_bytes([data[index + 3], data[index + 4]]) as u32;
            let width = u16::from_be_bytes([data[index + 5], data[index + 6]]) as u32;
            return done(width, height);
        }
        index += len;
    }
    Probe::Need(data.len() + 4)
}

fn done(width: u32, height: u32) -> Probe {
    if width == 0 || height == 0 {
        Probe::Unknown
    } else {
        Probe::Done(width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_headers_without_the_rest_of_the_file() {
        let mut png = vec![
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D', b'R',
        ];
        png.extend_from_slice(&100u32.to_be_bytes());
        png.extend_from_slice(&40u32.to_be_bytes());
        assert_eq!(bytes(&png), Some((100, 40)));

        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08];
        jpeg.extend_from_slice(&30u16.to_be_bytes());
        jpeg.extend_from_slice(&80u16.to_be_bytes());
        jpeg.extend_from_slice(&[1, 0x11, 0, 0]);
        assert_eq!(bytes(&jpeg), Some((80, 30)));

        assert_eq!(bytes(b"GIF89a\x2c\x01\x90\x00"), Some((300, 144)));
    }

    #[test]
    fn short_webp_header_does_not_panic() {
        let short = b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(short.len(), 12);
        assert_eq!(bytes(short), None);
        assert!(matches!(probe(short), Probe::Need(16)));

        let mut vp8x = b"RIFF\x00\x00\x00\x00WEBPVP8X".to_vec();
        vp8x.extend_from_slice(&[0u8; 14]);
        // canvas size fields at 24..30: width-1 and height-1 as 24-bit LE
        vp8x[24] = 99;
        vp8x[25] = 0;
        vp8x[26] = 0;
        vp8x[27] = 49;
        vp8x[28] = 0;
        vp8x[29] = 0;
        assert_eq!(bytes(&vp8x), Some((100, 50)));
    }
}
