//! DRM-free MOBI / AZW3 image extraction (PalmDB records).
//! Inspired by foliate-js mobi.js (MIT) — image resources only, no HTML pagination.
//!
//! Classic comic MOBIs often store images out of reading order in the PDB; page order
//! comes from `recindex` attributes in the PalmDOC-compressed HTML.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::extract::ExtractJob;
use super::names::tr;

pub struct MobiBook {
    pub path: PathBuf,
    /// Byte ranges (start, end) for each page image inside the file.
    pub ranges: Vec<(u64, u64)>,
}

impl MobiBook {
    pub fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        let (start, end) = *self.ranges.get(index).ok_or("page is out of range")?;
        let mut file = File::open(&self.path).map_err(|err| err.to_string())?;
        let len = end.saturating_sub(start) as usize;
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(start)).map_err(|err| err.to_string())?;
        file.read_exact(&mut buf).map_err(|err| err.to_string())?;
        Ok(trim_image_bytes(&buf))
    }
}

pub struct Listed {
    pub rels: Vec<String>,
    pub book: MobiBook,
    pub can_random_read: bool,
}

pub fn list(path: &Path, locale: &str) -> Result<Listed, String> {
    let data = std::fs::read(path).map_err(|err| err.to_string())?;
    if data.len() < 78 {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }
    let num_records = read_u16(&data, 76) as usize;
    if data.len() < 78 + num_records * 8 {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }
    let mut offsets = Vec::with_capacity(num_records);
    for i in 0..num_records {
        let at = 78 + i * 8;
        offsets.push(read_u32(&data, at) as usize);
    }
    if offsets.is_empty() {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }

    let rec0_end = offsets.get(1).copied().unwrap_or(data.len());
    if offsets[0] >= data.len() || rec0_end > data.len() || offsets[0] >= rec0_end {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }
    let rec0 = &data[offsets[0]..rec0_end];
    if rec0.len() < 16 {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }
    let encryption = read_u16(rec0, 12);
    if encryption != 0 {
        return Err(tr(
            locale,
            "不支持带 DRM 的 Kindle 电子书",
            "DRM-protected Kindle books are not supported",
        ));
    }

    if rec0.len() < 16 + 20 {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }
    let mobi = &rec0[16..];
    if &mobi[0..4] != b"MOBI" {
        return Err(tr(locale, "无效的 MOBI 文件", "Invalid MOBI file"));
    }

    let compression = read_u16(rec0, 0);
    let num_text = read_u16(rec0, 8) as usize;
    let first_image = first_image_index(rec0, mobi, num_text, num_records);
    let trailing_flags = if mobi.len() >= 244 {
        read_u32(mobi, 240)
    } else {
        0
    };

    // Map PDB record index → trimmed image range.
    let mut by_pdb: Vec<Option<(u64, u64, &'static str)>> = vec![None; num_records];
    for i in first_image..num_records {
        let start = offsets[i];
        let end = offsets.get(i + 1).copied().unwrap_or(data.len());
        if start >= end || end > data.len() {
            continue;
        }
        let record = &data[start..end];
        if record.len() >= 4 {
            let tag = &record[0..4];
            if tag == b"FONT"
                || tag == b"RESC"
                || tag == b"CRES"
                || tag == b"CONT"
                || tag == b"FLIS"
                || tag == b"FCIS"
                || tag == b"FDST"
                || tag == b"DATP"
                || tag == b"AUDI"
                || tag == b"VIDE"
            {
                continue;
            }
        }
        let cleaned = trim_image_bytes(record);
        if let Some(ext) = detect_image(&cleaned) {
            if cleaned.len() < 64 {
                continue;
            }
            by_pdb[i] = Some((start as u64, start as u64 + cleaned.len() as u64, ext));
        }
    }

    let mut ranges = Vec::new();
    let mut rels = Vec::new();
    let mut seen = HashSet::new();

    // Prefer HTML `recindex` reading order when PalmDOC text is available.
    if compression == 2 || compression == 1 {
        if let Some(html) = load_html_text(&data, &offsets, num_text, compression, trailing_flags) {
            let order = parse_recindex_order(&html);
            // Prepend EXTH cover when present and not already in the spine (matches common readers).
            if let Some(cover_rel) = exth_cover_offset(rec0, mobi) {
                let pdb = first_image.saturating_add(cover_rel);
                push_pdb(
                    pdb,
                    &by_pdb,
                    &mut ranges,
                    &mut rels,
                    &mut seen,
                    "cover",
                );
            }
            for (n, recindex) in order.into_iter().enumerate() {
                if recindex == 0 {
                    continue;
                }
                let pdb = first_image.saturating_add(recindex.saturating_sub(1));
                push_pdb(
                    pdb,
                    &by_pdb,
                    &mut ranges,
                    &mut rels,
                    &mut seen,
                    &format!("page-{n:05}"),
                );
            }
        }
    }

    // Fallback / no HTML order: PDB scan from first image, skipping cover+thumb when EXTH says so.
    if ranges.is_empty() {
        let skip = exth_skip_set(rec0, mobi, first_image);
        for (i, slot) in by_pdb.iter().enumerate().skip(first_image) {
            if skip.contains(&i) {
                continue;
            }
            if let Some((start, end, ext)) = slot {
                ranges.push((*start, *end));
                rels.push(format!("image-{i:05}.{ext}"));
            }
        }
        if ranges.is_empty() {
            for (i, slot) in by_pdb.iter().enumerate().skip(1) {
                if let Some((start, end, ext)) = slot {
                    ranges.push((*start, *end));
                    rels.push(format!("image-{i:05}.{ext}"));
                }
            }
        }
    }

    if ranges.is_empty() {
        return Err(tr(locale, "没有找到图片", "No images found"));
    }

    Ok(Listed {
        rels,
        book: MobiBook {
            path: path.to_path_buf(),
            ranges,
        },
        can_random_read: true,
    })
}

pub fn extract(path: PathBuf, book: MobiBook, job: ExtractJob) {
    let mut files = Vec::with_capacity(book.ranges.len());
    for index in 0..book.ranges.len() {
        if job.cancelled() {
            job.abort();
            return;
        }
        match book.read(index) {
            Ok(bytes) => {
                let ext = detect_image(&bytes).unwrap_or("img");
                let original = format!("image-{index:05}.{ext}");
                let (dest, name) = job.destination(index, &original);
                if let Err(err) = std::fs::write(&dest, &bytes) {
                    job.fail(err.to_string());
                    return;
                }
                files.push(name);
                job.mark_written(index, dest);
            }
            Err(err) => {
                job.fail(err);
                return;
            }
        }
    }
    let _ = path;
    job.succeed(files);
}

fn push_pdb(
    pdb: usize,
    by_pdb: &[Option<(u64, u64, &'static str)>],
    ranges: &mut Vec<(u64, u64)>,
    rels: &mut Vec<String>,
    seen: &mut HashSet<usize>,
    label: &str,
) {
    if pdb >= by_pdb.len() || !seen.insert(pdb) {
        return;
    }
    let Some((start, end, ext)) = by_pdb[pdb] else {
        return;
    };
    ranges.push((start, end));
    rels.push(format!("{label}.{ext}"));
}

fn first_image_index(rec0: &[u8], mobi: &[u8], num_text: usize, num_records: usize) -> usize {
    let first_image_108 = if mobi.len() >= 112 {
        read_u32(mobi, 108) as usize
    } else {
        0
    };
    let first_image_92 = if mobi.len() >= 96 {
        read_u32(mobi, 92) as usize
    } else {
        0
    };
    let _ = rec0;
    [first_image_108, first_image_92, num_text.saturating_add(1)]
        .into_iter()
        .find(|&index| index > 0 && index < num_records)
        .unwrap_or(1)
}

fn load_html_text(
    data: &[u8],
    offsets: &[usize],
    num_text: usize,
    compression: u16,
    trailing_flags: u32,
) -> Option<String> {
    if num_text == 0 {
        return None;
    }
    let mut out = Vec::new();
    for i in 1..=num_text {
        if i >= offsets.len() {
            break;
        }
        let start = offsets[i];
        let end = offsets.get(i + 1).copied().unwrap_or(data.len());
        if start >= end || end > data.len() {
            continue;
        }
        let mut chunk = data[start..end].to_vec();
        if trailing_flags != 0 {
            chunk = strip_text_trailing(&chunk, trailing_flags);
        }
        let decoded = match compression {
            1 => chunk, // no compression
            2 => decompress_palmdoc(&chunk),
            _ => return None, // HUFF/CDIC not handled yet for ordering
        };
        out.extend_from_slice(&decoded);
    }
    if out.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

fn parse_recindex_order(html: &str) -> Vec<usize> {
    let bytes = html.as_bytes();
    let lower = html.to_ascii_lowercase();
    let key = b"recindex";
    let mut out = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("recindex") {
        let at = search + rel;
        let after_key = at + key.len();
        let rest = &html[after_key..];
        let trimmed = rest.trim_start_matches(|ch: char| {
            ch.is_whitespace() || ch == '=' || ch == '"' || ch == '\'' || ch == ':'
        });
        let num: String = trimmed
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if let Ok(value) = num.parse::<usize>() {
            out.push(value);
        }
        search = after_key.max(at + 1);
        let _ = bytes;
    }
    out
}

/// EXTH record type 201 = cover offset relative to first image index.
fn exth_cover_offset(rec0: &[u8], mobi: &[u8]) -> Option<usize> {
    exth_u32(rec0, mobi, 201).map(|v| v as usize)
}

fn exth_skip_set(rec0: &[u8], mobi: &[u8], first_image: usize) -> HashSet<usize> {
    let mut set = HashSet::new();
    if let Some(cover) = exth_u32(rec0, mobi, 201) {
        set.insert(first_image.saturating_add(cover as usize));
    }
    if let Some(thumb) = exth_u32(rec0, mobi, 202) {
        set.insert(first_image.saturating_add(thumb as usize));
    }
    set
}

fn exth_u32(rec0: &[u8], mobi: &[u8], want: u32) -> Option<u32> {
    let mobi_len = read_u32(mobi, 4) as usize;
    let exth_at = 16usize.saturating_add(mobi_len);
    if exth_at + 12 > rec0.len() || &rec0[exth_at..exth_at + 4] != b"EXTH" {
        return None;
    }
    let count = read_u32(rec0, exth_at + 8) as usize;
    let mut p = exth_at + 12;
    for _ in 0..count {
        if p + 8 > rec0.len() {
            break;
        }
        let typ = read_u32(rec0, p);
        let len = read_u32(rec0, p + 4) as usize;
        if len < 8 || p + len > rec0.len() {
            break;
        }
        if typ == want && len >= 12 {
            return Some(read_u32(rec0, p + 8));
        }
        p += len;
    }
    None
}

fn decompress_palmdoc(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len().saturating_mul(2));
    let mut i = 0usize;
    while i < input.len() {
        let byte = input[i];
        i += 1;
        if byte == 0 {
            output.push(0);
        } else if byte <= 8 {
            let end = (i + byte as usize).min(input.len());
            output.extend_from_slice(&input[i..end]);
            i = end;
        } else if byte <= 0x7f {
            output.push(byte);
        } else if byte <= 0xbf {
            if i >= input.len() {
                break;
            }
            let next = input[i];
            i += 1;
            let bytes = u16::from(byte) << 8 | u16::from(next);
            let distance = ((bytes & 0x3fff) >> 3) as usize;
            let length = (bytes & 7) as usize + 3;
            for _ in 0..length {
                let idx = output.len().saturating_sub(distance);
                let value = output.get(idx).copied().unwrap_or(0);
                output.push(value);
            }
        } else {
            output.push(b' ');
            output.push(byte ^ 0x80);
        }
    }
    output
}

fn strip_text_trailing(data: &[u8], flags: u32) -> Vec<u8> {
    // Foliate/KindleUnpack: drop one multibyte-sized trailing entry per set bit in flags,
    // plus an extra size byte when the low bit pattern requires it.
    let mut end = data.len();
    let mut flags = flags;
    while flags != 0 {
        let size = varlen_from_end(&data[..end]);
        if size == 0 || size > end {
            break;
        }
        end -= size;
        flags &= flags.wrapping_sub(1); // clear lowest set bit
    }
    // Multibyte length of all trailing entries when bit 0 of original flags was set is
    // already consumed above; keep a conservative clip only.
    data[..end].to_vec()
}

fn varlen_from_end(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }
    let mut value = 0usize;
    let start = data.len().saturating_sub(4);
    for &byte in &data[start..] {
        if byte & 0x80 != 0 {
            value = 0;
        }
        value = (value << 7) | (byte & 0x7f) as usize;
    }
    value
}

fn read_u16(data: &[u8], at: usize) -> u16 {
    if at + 2 > data.len() {
        return 0;
    }
    u16::from_be_bytes([data[at], data[at + 1]])
}

fn read_u32(data: &[u8], at: usize) -> u32 {
    if at + 4 > data.len() {
        return 0;
    }
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn detect_image(data: &[u8]) -> Option<&'static str> {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some("jpg");
    }
    if data.len() >= 8 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if data.len() >= 6 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return Some("gif");
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Some("webp");
    }
    if data.len() >= 2 && data.starts_with(b"BM") {
        return Some("bmp");
    }
    None
}

fn trim_image_bytes(data: &[u8]) -> Vec<u8> {
    if data.len() < 4 {
        return data.to_vec();
    }
    if detect_image(data).is_some() {
        if data[0] == 0xFF && data[1] == 0xD8 {
            if let Some(end) = find_jpeg_end(data) {
                return data[..end].to_vec();
            }
        }
        if data.starts_with(b"\x89PNG") {
            if let Some(end) = find_png_end(data) {
                return data[..end].to_vec();
            }
        }
        return data.to_vec();
    }
    data.to_vec()
}

fn find_jpeg_end(data: &[u8]) -> Option<usize> {
    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD9 {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

fn find_png_end(data: &[u8]) -> Option<usize> {
    let marker = b"IEND";
    let mut i = 8;
    while i + 8 <= data.len() {
        if &data[i + 4..i + 8] == marker {
            return Some((i + 12).min(data.len()));
        }
        let len = read_u32(data, i) as usize;
        i = i.saturating_add(12).saturating_add(len);
        if i <= 8 {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_jpeg_magic() {
        assert_eq!(detect_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(detect_image(b"not-image"), None);
    }

    #[test]
    fn rejects_tiny_buffer_as_mobi() {
        let dir = std::env::temp_dir().join(format!("jmr-mobi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.mobi");
        std::fs::write(&path, b"short").unwrap();
        assert!(list(&path, "en").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_recindex_preserves_document_order() {
        let html = r#"<img recindex="00036"><img recindex='74'><img recindex=00064>"#;
        assert_eq!(parse_recindex_order(html), vec![36, 74, 64]);
    }

    #[test]
    fn finds_images_when_resource_start_is_zero() {
        let mut jpeg_rec = vec![0xFF, 0xD8, 0xFF, 0xE0];
        jpeg_rec.resize(78, 0x00);
        jpeg_rec.push(0xFF);
        jpeg_rec.push(0xD9);

        let mut rec0 = vec![0u8; 16 + 120];
        rec0[0] = 0;
        rec0[1] = 1;
        rec0[8] = 0;
        rec0[9] = 0;
        rec0[16..20].copy_from_slice(b"MOBI");
        rec0[20..24].copy_from_slice(&120u32.to_be_bytes());

        let name = b"TESTBOOK\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        let mut pdb = Vec::new();
        pdb.extend_from_slice(&name[..32]);
        pdb.extend_from_slice(&[0u8; 28]);
        pdb.extend_from_slice(b"BOOK");
        pdb.extend_from_slice(b"MOBI");
        pdb.extend_from_slice(&[0u8; 8]);
        pdb.extend_from_slice(&2u16.to_be_bytes());
        let header_end = 78 + 2 * 8;
        let rec0_off = header_end as u32;
        let rec1_off = rec0_off + rec0.len() as u32;
        pdb.extend_from_slice(&rec0_off.to_be_bytes());
        pdb.extend_from_slice(&0u32.to_be_bytes());
        pdb.extend_from_slice(&rec1_off.to_be_bytes());
        pdb.extend_from_slice(&0u32.to_be_bytes());
        pdb.extend_from_slice(&rec0);
        pdb.extend_from_slice(&jpeg_rec);

        let dir = std::env::temp_dir().join(format!("jmr-mobi-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("comic.mobi");
        std::fs::write(&path, &pdb).unwrap();
        let listed = list(&path, "en").unwrap();
        assert_eq!(listed.rels.len(), 1);
        assert!(listed.book.read(0).unwrap().starts_with(&[0xFF, 0xD8]));
        std::fs::remove_dir_all(&dir).ok();
    }
}
