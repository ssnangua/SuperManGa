//! Central-directory listing.
//!
//! `ZipArchive::by_index_raw` seeks to every local file header. Comic zips have
//! hundreds of those, so opening used to stall on random reads. Names, the
//! UTF-8 flag, and the encryption bit all live in the central directory, which
//! is one sequential read at the end of the file.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const CD_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];
const ZIP64_LOC_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
const MAX_CD_BYTES: u64 = 32 * 1024 * 1024;

/// One central-directory record, indexed the same way as `zip::ZipArchive`.
pub struct CdEntry {
    pub index: usize,
    pub name_raw: Vec<u8>,
    pub encrypted: bool,
}

pub fn entries(path: &Path) -> Result<Vec<CdEntry>, String> {
    let mut file = File::open(path).map_err(|err| err.to_string())?;
    let len = file.metadata().map_err(|err| err.to_string())?.len();
    if len < 22 {
        return Err("zip is too small".into());
    }
    let located = locate(&mut file, len)?;
    if located.count == 0 {
        return Ok(Vec::new());
    }
    let mut cd = vec![0u8; located.size as usize];
    file.seek(SeekFrom::Start(located.offset))
        .map_err(|err| err.to_string())?;
    file.read_exact(&mut cd).map_err(|err| err.to_string())?;
    parse_cd(&cd, located.count)
}

struct Located {
    offset: u64,
    size: u64,
    count: u64,
}

fn locate(file: &mut File, len: u64) -> Result<Located, String> {
    let tail_len = std::cmp::min(len, 22 + 65535 + 20 + 65536) as usize;
    let tail_start = len - tail_len as u64;
    let mut tail = vec![0u8; tail_len];
    file.seek(SeekFrom::Start(tail_start))
        .map_err(|err| err.to_string())?;
    file.read_exact(&mut tail).map_err(|err| err.to_string())?;

    if tail_len < 22 {
        return Err("zip is too small".into());
    }
    let mut index = tail_len - 22;
    loop {
        if tail[index..index + 4] == EOCD_SIG {
            let comment_len = u16_at(&tail, index + 20) as usize;
            if index + 22 + comment_len <= tail_len {
                if let Some(located) = accept_eocd(file, len, &tail, tail_start, index)? {
                    return Ok(located);
                }
            }
        }
        if index == 0 {
            break;
        }
        index -= 1;
    }
    Err("central directory not found".into())
}

fn accept_eocd(
    file: &mut File,
    len: u64,
    tail: &[u8],
    tail_start: u64,
    index: usize,
) -> Result<Option<Located>, String> {
    let disk = u16_at(tail, index + 4);
    let disk_cd = u16_at(tail, index + 6);
    let entries_disk = u16_at(tail, index + 8) as u64;
    let total = u16_at(tail, index + 10) as u64;
    let cd_size = u32_at(tail, index + 12) as u64;
    let cd_offset = u32_at(tail, index + 16) as u64;
    let eocd_pos = tail_start + index as u64;
    let needs_zip64 = entries_disk == 0xffff
        || total == 0xffff
        || cd_size == 0xffff_ffff
        || cd_offset == 0xffff_ffff;

    if needs_zip64 {
        if let Some(located) = zip64_location(file, len, tail, tail_start, index)? {
            return Ok(Some(located));
        }
    }

    if disk != disk_cd {
        return Ok(None);
    }
    if entries_disk == 0xffff || cd_size == 0xffff_ffff || cd_offset == 0xffff_ffff {
        return Ok(None);
    }
    let count = entries_disk;
    if let Some(located) = checked(file, len, eocd_pos.saturating_sub(cd_size), cd_size, count)? {
        return Ok(Some(located));
    }
    checked(file, len, cd_offset, cd_size, count)
}

fn zip64_location(
    file: &mut File,
    len: u64,
    tail: &[u8],
    tail_start: u64,
    eocd_index: usize,
) -> Result<Option<Located>, String> {
    if eocd_index < 20 {
        return Ok(None);
    }
    let locator = eocd_index - 20;
    if tail[locator..locator + 4] != ZIP64_LOC_SIG {
        return Ok(None);
    }
    let disks = u32_at(tail, locator + 16);
    if disks > 1 {
        return Err("multi-disk zip is not supported".into());
    }
    let search_from = locator.saturating_sub(65536);
    let last = locator.saturating_sub(56);
    if search_from > last {
        return Ok(None);
    }
    let mut pos = last;
    loop {
        if tail[pos..pos + 4] == ZIP64_EOCD_SIG {
            let record_size = u64_at(tail, pos + 4);
            if record_size >= 44 && pos as u64 + 12 + record_size == locator as u64 {
                let disk = u32_at(tail, pos + 16);
                let disk_cd = u32_at(tail, pos + 20);
                if disk == disk_cd {
                    let count = u64_at(tail, pos + 32);
                    let cd_size = u64_at(tail, pos + 40);
                    let zip64_pos = tail_start + pos as u64;
                    if let Some(cd_pos) = zip64_pos.checked_sub(cd_size) {
                        if let Some(located) = checked(file, len, cd_pos, cd_size, count)? {
                            return Ok(Some(located));
                        }
                    }
                }
            }
        }
        if pos == search_from {
            break;
        }
        pos -= 1;
    }
    Ok(None)
}

fn checked(
    file: &mut File,
    len: u64,
    offset: u64,
    size: u64,
    count: u64,
) -> Result<Option<Located>, String> {
    if count == 0 && size == 0 {
        return Ok(Some(Located {
            offset,
            size,
            count,
        }));
    }
    if size == 0 || size > MAX_CD_BYTES || count > usize::MAX as u64 {
        return Ok(None);
    }
    if count.saturating_mul(46) > size {
        return Ok(None);
    }
    let Some(end) = offset.checked_add(size) else {
        return Ok(None);
    };
    if end > len {
        return Ok(None);
    }
    let mut sig = [0u8; 4];
    file.seek(SeekFrom::Start(offset))
        .map_err(|err| err.to_string())?;
    file.read_exact(&mut sig).map_err(|err| err.to_string())?;
    if sig != CD_SIG {
        return Ok(None);
    }
    Ok(Some(Located {
        offset,
        size,
        count,
    }))
}

struct Slot {
    name_raw: Vec<u8>,
    encrypted: bool,
}

fn parse_cd(cd: &[u8], count: u64) -> Result<Vec<CdEntry>, String> {
    let mut pos = 0usize;
    let mut slots: Vec<Slot> = Vec::new();
    let mut keys: HashMap<String, usize> = HashMap::new();
    for _ in 0..count {
        if pos + 46 > cd.len() {
            return Err("central directory ended early".into());
        }
        if cd[pos..pos + 4] != CD_SIG {
            return Err("bad central directory header".into());
        }
        let flags = u16_at(cd, pos + 8);
        let name_len = u16_at(cd, pos + 28) as usize;
        let extra_len = u16_at(cd, pos + 30) as usize;
        let comment_len = u16_at(cd, pos + 32) as usize;
        let name_at = pos + 46;
        let extra_at = name_at + name_len;
        let comment_at = extra_at + extra_len;
        let next = comment_at + comment_len;
        if next > cd.len() {
            return Err("central directory entry overruns".into());
        }
        let mut raw = cd[name_at..extra_at].to_vec();
        let mut utf8 = flags & (1 << 11) != 0;
        apply_unicode_path(&mut raw, &mut utf8, &cd[extra_at..comment_at])?;
        let key = if utf8 {
            String::from_utf8_lossy(&raw).into_owned()
        } else {
            cp437(&raw)
        };
        let slot = Slot {
            name_raw: raw,
            encrypted: flags & 1 != 0,
        };
        if let Some(&index) = keys.get(&key) {
            slots[index] = slot;
        } else {
            keys.insert(key, slots.len());
            slots.push(slot);
        }
        pos = next;
    }
    Ok(slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| CdEntry {
            index,
            name_raw: slot.name_raw,
            encrypted: slot.encrypted,
        })
        .collect())
}

/// Info-ZIP Unicode Path (0x7075). A checksum mismatch rejects the archive,
/// matching the `zip` crate; a truncated extra field is ignored.
fn apply_unicode_path(raw: &mut Vec<u8>, utf8: &mut bool, extra: &[u8]) -> Result<(), String> {
    let mut pos = 0usize;
    while pos + 4 <= extra.len() {
        let id = u16_at(extra, pos);
        let size = u16_at(extra, pos + 2) as usize;
        let data_at = pos + 4;
        let data_end = data_at + size;
        if data_end > extra.len() {
            break;
        }
        if id == 0x7075 {
            if size < 5 {
                return Err("unicode path extra is too small".into());
            }
            let data = &extra[data_at..data_end];
            let expect = u32_at(data, 1);
            let content = &data[5..];
            if crc32(raw) != expect {
                return Err("unicode path checksum failed".into());
            }
            if std::str::from_utf8(content).is_err() {
                return Err("unicode path is not utf-8".into());
            }
            *raw = content.to_vec();
            *utf8 = true;
        }
        pos = data_end;
    }
    Ok(())
}

fn cp437(bytes: &[u8]) -> String {
    if bytes.iter().all(|byte| *byte < 0x80) {
        return String::from_utf8(bytes.to_vec()).unwrap_or_default();
    }
    bytes
        .iter()
        .map(|byte| {
            if *byte < 0x80 {
                *byte as char
            } else {
                char::from_u32(CP437_HIGH[(*byte - 0x80) as usize]).unwrap_or('\u{FFFD}')
            }
        })
        .collect()
}

fn u16_at(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([bytes[index], bytes[index + 1]])
}

fn u32_at(bytes: &[u8], index: usize) -> u32 {
    u32::from_le_bytes([
        bytes[index],
        bytes[index + 1],
        bytes[index + 2],
        bytes[index + 3],
    ])
}

fn u64_at(bytes: &[u8], index: usize) -> u64 {
    u64::from_le_bytes([
        bytes[index],
        bytes[index + 1],
        bytes[index + 2],
        bytes[index + 3],
        bytes[index + 4],
        bytes[index + 5],
        bytes[index + 6],
        bytes[index + 7],
    ])
}

pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// IBM code page 437, bytes 0x80..=0xFF. Matches the `zip` crate's table so
/// duplicate-name collapsing uses the same key.
const CP437_HIGH: [u32; 128] = [
    0x00c7, 0x00fc, 0x00e9, 0x00e2, 0x00e4, 0x00e0, 0x00e5, 0x00e7, 0x00ea, 0x00eb, 0x00e8, 0x00ef,
    0x00ee, 0x00ec, 0x00c4, 0x00c5, 0x00c9, 0x00e6, 0x00c6, 0x00f4, 0x00f6, 0x00f2, 0x00fb, 0x00f9,
    0x00ff, 0x00d6, 0x00dc, 0x00a2, 0x00a3, 0x00a5, 0x20a7, 0x0192, 0x00e1, 0x00ed, 0x00f3, 0x00fa,
    0x00f1, 0x00d1, 0x00aa, 0x00ba, 0x00bf, 0x2310, 0x00ac, 0x00bd, 0x00bc, 0x00a1, 0x00ab, 0x00bb,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x2561, 0x2562, 0x2556, 0x2555, 0x2563, 0x2551, 0x2557,
    0x255d, 0x255c, 0x255b, 0x2510, 0x2514, 0x2534, 0x252c, 0x251c, 0x2500, 0x253c, 0x255e, 0x255f,
    0x255a, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256c, 0x2567, 0x2568, 0x2564, 0x2565, 0x2559,
    0x2558, 0x2552, 0x2553, 0x256b, 0x256a, 0x2518, 0x250c, 0x2588, 0x2584, 0x258c, 0x2590, 0x2580,
    0x03b1, 0x00df, 0x0393, 0x03c0, 0x03a3, 0x03c3, 0x00b5, 0x03c4, 0x03a6, 0x0398, 0x03a9, 0x03b4,
    0x221e, 0x03c6, 0x03b5, 0x2229, 0x2261, 0x00b1, 0x2265, 0x2264, 0x2320, 0x2321, 0x00f7, 0x2248,
    0x00b0, 0x2219, 0x00b7, 0x221a, 0x207f, 0x00b2, 0x25a0, 0x00a0,
];
