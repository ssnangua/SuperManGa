use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub fn tr(locale: &str, zh: &str, en: &str) -> String {
    if locale.to_ascii_lowercase().starts_with("en") {
        en.to_string()
    } else {
        zh.to_string()
    }
}

pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_image_ext(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp")
}

pub fn is_image(rel: &str) -> bool {
    let ext = rel.rsplit('.').next().unwrap_or("");
    is_image_ext(&ext.to_ascii_lowercase())
}

pub fn is_junk(rel: &str) -> bool {
    let norm = rel.replace('\\', "/");
    for part in norm.split('/') {
        if part.is_empty() {
            continue;
        }
        if part.starts_with('.') || part.eq_ignore_ascii_case("__MACOSX") {
            return true;
        }
        if part.eq_ignore_ascii_case("thumbs.db") || part.eq_ignore_ascii_case("desktop.ini") {
            return true;
        }
    }
    false
}

pub fn normalize_rel(rel: &str) -> String {
    rel.replace('\\', "/")
}

pub fn file_title(rel: &str) -> String {
    let norm = normalize_rel(rel);
    norm.rsplit('/').next().unwrap_or(&norm).to_string()
}

pub fn mime_of(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    }
}

pub fn cache_filename(index: usize, original: &str) -> String {
    let ext = Path::new(original)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .filter(|ext| {
            !ext.is_empty()
                && ext.len() <= 5
                && ext.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        .unwrap_or_else(|| "img".to_string());
    format!("{index:05}.{ext}")
}

pub fn sibling_bookmarks(comic: &Path) -> PathBuf {
    let parent = comic.parent().unwrap_or_else(|| Path::new("."));
    let stem = comic.file_stem().unwrap_or_default().to_string_lossy();
    parent.join(format!("{stem}.smbookmarks"))
}

pub fn title_of(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "comic".to_string())
}

/// True when a UTF-8 string looks like a normal archive path (ASCII / CJK / kana),
/// not accidental Latin-1 leftovers from GBK bytes that happen to be valid UTF-8
/// (e.g. GBK「目录」→ UTF-8 `Ŀ¼`).
fn looks_like_archive_utf8(text: &str) -> bool {
    text.chars().all(|ch| {
        ch.is_ascii()
            || ('\u{4e00}'..='\u{9fff}').contains(&ch) // CJK Unified
            || ('\u{3400}'..='\u{4dbf}').contains(&ch) // CJK Extension A
            || ('\u{f900}'..='\u{faff}').contains(&ch) // CJK Compatibility
            || ('\u{3000}'..='\u{303f}').contains(&ch) // CJK punctuation
            || ('\u{3040}'..='\u{30ff}').contains(&ch) // Hiragana / Katakana
            || ('\u{ff00}'..='\u{ffef}').contains(&ch) // Halfwidth / fullwidth
            || ch == '・'
            || ch == 'ー'
            || ch == '～'
            || ch == '—'
            || ch == '–'
    })
}

fn score_decoded_name(text: &str) -> i32 {
    let mut score = 0i32;
    for ch in text.chars() {
        if ('\u{4e00}'..='\u{9fff}').contains(&ch)
            || ('\u{3400}'..='\u{4dbf}').contains(&ch)
            || ('\u{f900}'..='\u{faff}').contains(&ch)
        {
            score += 4;
        } else if ('\u{3040}'..='\u{30ff}').contains(&ch) {
            score += 3;
        } else if ch == '\u{fffd}' {
            score -= 8;
        } else if ('\u{2500}'..='\u{257f}').contains(&ch) {
            // CP437 box-drawing leftovers
            score -= 4;
        } else if ('\u{00a0}'..='\u{024f}').contains(&ch) {
            // Latin-1 / Latin Extended — typical GBK-as-UTF-8 accidents (Ŀ¼…)
            score -= 5;
        }
    }
    score
}

fn decode_legacy_zip_name(raw: &[u8]) -> String {
    let gbk = encoding_rs::GB18030.decode(raw).0.into_owned();
    let big5 = encoding_rs::BIG5.decode(raw).0.into_owned();
    if score_decoded_name(&big5) > score_decoded_name(&gbk) {
        big5
    } else {
        gbk
    }
}

fn utf8_looks_like_gbk_accident(text: &str) -> bool {
    // GBK double-bytes often decode as Latin-1 / Latin Extended when read as UTF-8
    // (e.g. 目录 → Ŀ¼). Box-drawing CP437 mojibake is a different failure mode.
    text.chars()
        .any(|ch| ('\u{00a0}'..='\u{024f}').contains(&ch))
}

/// Decode a ZIP entry name from raw central-directory bytes.
///
/// Chinese Windows zips often omit the UTF-8 flag and store GBK/Big5 bytes.
/// The `zip` crate remaps those through CP437 into mojibake — always use `name_raw()`.
///
/// Valid UTF-8 is kept when it looks like a real path. If it is valid UTF-8 but
/// only via Latin-1 accidents (GBK「目录」→ `Ŀ¼`), prefer GBK/Big5 by score.
pub fn decode_zip_name(raw: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(raw) {
        if looks_like_archive_utf8(text) {
            return text.to_string();
        }
        if utf8_looks_like_gbk_accident(text) {
            let legacy = decode_legacy_zip_name(raw);
            if score_decoded_name(&legacy) > score_decoded_name(text) {
                return legacy;
            }
        }
        return text.to_string();
    }
    decode_legacy_zip_name(raw)
}

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let left = a.to_lowercase();
    let right = b.to_lowercase();
    let mut ia = left.chars().peekable();
    let mut ib = right.chars().peekable();
    loop {
        match (ia.peek().copied(), ib.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na: u128 = 0;
                let mut nb: u128 = 0;
                let mut sa = String::new();
                let mut sb = String::new();
                while let Some(ch) = ia.peek().copied() {
                    if !ch.is_ascii_digit() {
                        break;
                    }
                    sa.push(ch);
                    na = na.saturating_mul(10).saturating_add((ch as u8 - b'0') as u128);
                    ia.next();
                }
                while let Some(ch) = ib.peek().copied() {
                    if !ch.is_ascii_digit() {
                        break;
                    }
                    sb.push(ch);
                    nb = nb.saturating_mul(10).saturating_add((ch as u8 - b'0') as u128);
                    ib.next();
                }
                let by_value = na.cmp(&nb);
                if by_value != Ordering::Equal {
                    return by_value;
                }
                let by_width = sa.len().cmp(&sb.len());
                if by_width != Ordering::Equal {
                    return by_width;
                }
            }
            (Some(ca), Some(cb)) => {
                ia.next();
                ib.next();
                let by_char = ca.cmp(&cb);
                if by_char != Ordering::Equal {
                    return by_char;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_puts_2_before_10() {
        let mut names: Vec<String> = vec!["10.jpg".into(), "2.jpg".into(), "1.jpg".into()];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["1.jpg", "2.jpg", "10.jpg"]);
    }

    #[test]
    fn junk_and_image_filters() {
        assert!(is_junk("__MACOSX/foo.jpg"));
        assert!(is_junk("Thumbs.db"));
        assert!(!is_junk("chapter/01.jpg"));
        assert!(is_image("a.JPEG"));
        assert!(!is_image("a.txt"));
    }

    #[test]
    fn gbk_zip_name_decodes() {
        let (bytes, _, _) = encoding_rs::GBK.encode("测试.jpg");
        assert!(std::str::from_utf8(&bytes).is_err());
        assert_eq!(decode_zip_name(&bytes), "测试.jpg");
        assert_eq!(decode_zip_name(b"plain.jpg"), "plain.jpg");
        let (cover, _, _) = encoding_rs::GBK.encode("FCD-DFC-01-0000-A-混搭封面.jpg");
        assert_eq!(decode_zip_name(&cover), "FCD-DFC-01-0000-A-混搭封面.jpg");
    }

    #[test]
    fn gbk_bytes_that_are_also_valid_utf8_prefer_gbk() {
        // GBK「目录」== c4 bf c2 bc, which UTF-8 reads as Latin `Ŀ¼`.
        let (bytes, _, _) = encoding_rs::GBK.encode("目录.jpg");
        assert!(std::str::from_utf8(&bytes).is_ok());
        assert_eq!(decode_zip_name(&bytes), "目录.jpg");
    }

    #[test]
    fn real_utf8_chinese_names_stay_utf8() {
        assert_eq!(decode_zip_name("测试.jpg".as_bytes()), "测试.jpg");
        assert_eq!(decode_zip_name("目录.jpg".as_bytes()), "目录.jpg");
    }

    #[test]
    fn cp437_remapped_bytes_are_not_recoverable_via_utf8() {
        // Simulates feeding zip's already-CP437-decoded mojibake back as UTF-8 bytes —
        // decode_zip_name must not "fix" that; callers must use name_raw instead.
        let mojibake = "FCD-DFC-01-0000-A-┐∞┤ε╥Γ├μ.jpg";
        assert_eq!(decode_zip_name(mojibake.as_bytes()), mojibake);
    }

    #[test]
    fn bookmarks_file_sits_beside_the_book() {
        let path = sibling_bookmarks(Path::new(r"D:\comics\my book.zip"));
        assert_eq!(
            path,
            PathBuf::from(r"D:\comics").join("my book.smbookmarks")
        );
    }
}
