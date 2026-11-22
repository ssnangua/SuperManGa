use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use zip::ZipArchive;

use super::cbz::ZipBook;
use super::names::{decode_zip_name, is_image, is_junk, normalize_rel};

/// List comic page images from an EPUB in spine order.
/// Returns display names and zip entry indices (same shape as CBZ).
pub fn list(path: &Path) -> Result<(Vec<String>, Vec<usize>), String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|err| err.to_string())?;
    let entries = index_entries(&mut archive)?;
    let container = read_entry_text(&mut archive, &entries, "META-INF/container.xml")?;
    let opf_path = rootfile_path(&container).ok_or_else(|| "invalid EPUB container".to_string())?;
    let opf_text = read_entry_text(&mut archive, &entries, &opf_path)?;
    let opf_dir = parent_dir(&opf_path);
    let (manifest, spine) = parse_opf(&opf_text)?;
    if spine.is_empty() {
        return Err("invalid EPUB spine".into());
    }

    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    for id in spine {
        let Some(item) = manifest.get(&id) else {
            continue;
        };
        let href = resolve_href(&opf_dir, &item.href);
        if is_image_type(&item.media_type) || is_image(&href) {
            push_page(&mut pages, &mut seen, &entries, &href);
            continue;
        }
        if is_document_type(&item.media_type) || is_document_path(&href) {
            let Ok(xhtml) = read_entry_text(&mut archive, &entries, &href) else {
                continue;
            };
            let base = parent_dir(&href);
            for img in document_images(&xhtml) {
                let resolved = resolve_href(&base, &img);
                push_page(&mut pages, &mut seen, &entries, &resolved);
            }
        }
    }

    if pages.is_empty() {
        // Fallback: every image in the package, natural order (still better than failing early
        // for odd comic EPUBs that omit spine image refs).
        let mut fallback = Vec::new();
        for (name, index) in &entries {
            if is_image(name) && !is_junk(name) {
                fallback.push((name.clone(), *index));
            }
        }
        fallback.sort_by(|a, b| super::names::natural_cmp(&a.0, &b.0));
        pages = fallback;
    }

    let rels = pages.iter().map(|(name, _)| name.clone()).collect();
    let indices = pages.iter().map(|(_, index)| *index).collect();
    Ok((rels, indices))
}

pub fn open_book(path: &Path) -> Result<(Vec<String>, ZipBook), String> {
    let (rels, indices) = list(path)?;
    Ok((
        rels,
        ZipBook {
            path: path.to_path_buf(),
            indices,
            password: None,
        },
    ))
}

struct ManifestItem {
    href: String,
    media_type: String,
}

fn index_entries(archive: &mut ZipArchive<BufReader<File>>) -> Result<HashMap<String, usize>, String> {
    let mut map = HashMap::new();
    for index in 0..archive.len() {
        let Some(raw) = archive.name_for_index(index) else {
            continue;
        };
        if raw.ends_with('/') || raw.ends_with('\\') {
            continue;
        }
        let name = normalize_rel(&decode_zip_name(raw.as_bytes()));
        map.insert(name, index);
        // EPUB paths are often looked up case-sensitively; also keep a lower-case key for recovery.
        // Prefer first occurrence.
    }
    Ok(map)
}

fn find_entry<'a>(entries: &'a HashMap<String, usize>, path: &str) -> Option<&'a usize> {
    let norm = normalize_rel(path.trim_start_matches('/'));
    if let Some(index) = entries.get(&norm) {
        return Some(index);
    }
    let lower = norm.to_ascii_lowercase();
    entries
        .iter()
        .find(|(name, _)| name.to_ascii_lowercase() == lower)
        .map(|(_, index)| index)
}

fn read_entry_text(
    archive: &mut ZipArchive<BufReader<File>>,
    entries: &HashMap<String, usize>,
    path: &str,
) -> Result<String, String> {
    let index = find_entry(entries, path).copied().ok_or_else(|| format!("missing EPUB entry: {path}"))?;
    let mut entry = archive.by_index(index).map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(|err| err.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn rootfile_path(container: &str) -> Option<String> {
    // <rootfile ... full-path="OEBPS/content.opf" ... />
    attr_values(container, "rootfile", "full-path")
        .into_iter()
        .next()
        .map(|path| normalize_rel(&path))
}

fn parse_opf(opf: &str) -> Result<(HashMap<String, ManifestItem>, Vec<String>), String> {
    let mut manifest = HashMap::new();
    for tag in tags_named(opf, "item") {
        let id = attr_in_tag(&tag, "id");
        let href = attr_in_tag(&tag, "href");
        if id.is_empty() || href.is_empty() {
            continue;
        }
        let media_type = attr_in_tag(&tag, "media-type");
        manifest.insert(
            id,
            ManifestItem {
                href: html_decode(&href),
                media_type,
            },
        );
    }
    let mut spine = Vec::new();
    for tag in tags_named(opf, "itemref") {
        let idref = attr_in_tag(&tag, "idref");
        if !idref.is_empty() {
            spine.push(idref);
        }
    }
    Ok((manifest, spine))
}

fn document_images(xhtml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |value: String| {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("data:") {
            return;
        }
        if !out.iter().any(|existing| existing == trimmed) {
            out.push(html_decode(trimmed));
        }
    };
    for value in attr_values(xhtml, "img", "src") {
        push(value);
    }
    for value in attr_values(xhtml, "image", "href") {
        push(value);
    }
    for value in attr_values(xhtml, "image", "xlink:href") {
        push(value);
    }
    // CSS background-image: url(...)
    let mut search = xhtml;
    while let Some(pos) = search.to_ascii_lowercase().find("url(") {
        let after = &search[pos + 4..];
        let end = after.find(')').unwrap_or(after.len());
        let mut raw = after[..end].trim();
        if (raw.starts_with('"') && raw.ends_with('"')) || (raw.starts_with('\'') && raw.ends_with('\''))
        {
            raw = &raw[1..raw.len() - 1];
        }
        push(raw.to_string());
        search = &after[end.saturating_add(1).min(after.len())..];
    }
    out
}

fn push_page(
    pages: &mut Vec<(String, usize)>,
    seen: &mut HashSet<usize>,
    entries: &HashMap<String, usize>,
    href: &str,
) {
    if is_junk(href) || !is_image(href) {
        return;
    }
    let Some(&index) = find_entry(entries, href) else {
        return;
    };
    if !seen.insert(index) {
        return;
    }
    pages.push((normalize_rel(href), index));
}

fn parent_dir(path: &str) -> String {
    let norm = normalize_rel(path);
    match norm.rfind('/') {
        Some(pos) => norm[..pos].to_string(),
        None => String::new(),
    }
}

fn resolve_href(base: &str, href: &str) -> String {
    let href = normalize_rel(href.trim());
    if href.starts_with('/') {
        return href.trim_start_matches('/').to_string();
    }
    if base.is_empty() {
        return strip_dot_segments(&href);
    }
    let joined = format!("{base}/{href}");
    strip_dot_segments(&joined)
}

fn strip_dot_segments(path: &str) -> String {
    let mut stack = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    stack.join("/")
}

fn is_image_type(media: &str) -> bool {
    let m = media.to_ascii_lowercase();
    m.starts_with("image/")
}

fn is_document_type(media: &str) -> bool {
    let m = media.to_ascii_lowercase();
    m.contains("html") || m.contains("xml") || m == "text/html" || m == "application/xhtml+xml"
}

fn is_document_path(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "xhtml" | "html" | "htm" | "xml")
}

fn tags_named<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    let patterns = [
        format!("<{name} "),
        format!("<{name}>"),
        format!("<{name}/>"),
        format!("<{name} /"),
    ];
    // Also match prefixed tags: <opf:item ...>
    while !rest.is_empty() {
        let mut best: Option<usize> = None;
        for (idx, _) in rest.char_indices() {
            let slice = &rest[idx..];
            let lower_start = slice.as_bytes().first().copied() == Some(b'<');
            if !lower_start {
                continue;
            }
            if tag_starts_with(slice, name) {
                best = Some(idx);
                break;
            }
        }
        let Some(idx) = best else {
            break;
        };
        let from = &rest[idx..];
        let end = find_tag_end(from).unwrap_or(from.len());
        out.push(&from[..end]);
        rest = &from[end..];
        let _ = &patterns;
    }
    out
}

fn tag_starts_with(slice: &str, name: &str) -> bool {
    if !slice.starts_with('<') {
        return false;
    }
    let body = &slice[1..];
    let body = body.strip_prefix('/').unwrap_or(body);
    let local = match body.find(':') {
        Some(pos) if body[..pos].chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') => {
            // prefix:name — only treat as prefix if next part matches
            let after = &body[pos + 1..];
            after
        }
        _ => body,
    };
    if !local.to_ascii_lowercase().starts_with(&name.to_ascii_lowercase()) {
        return false;
    }
    let rest = &local[name.len()..];
    rest.is_empty()
        || rest.starts_with(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
}

fn find_tag_end(tag: &str) -> Option<usize> {
    let bytes = tag.as_bytes();
    let mut i = 0;
    let mut in_quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_quote {
            if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_quote = Some(b);
            i += 1;
            continue;
        }
        if b == b'>' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn attr_values(xml: &str, tag_name: &str, attr: &str) -> Vec<String> {
    tags_named(xml, tag_name)
        .into_iter()
        .map(|tag| attr_in_tag(tag, attr))
        .filter(|value| !value.is_empty())
        .collect()
}

fn attr_in_tag(tag: &str, attr: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{attr}=");
    let key_lower = key.to_ascii_lowercase();
    let Some(pos) = lower.find(&key_lower) else {
        // try prefixed attr like xlink:href already passed as full name
        return String::new();
    };
    let after = &tag[pos + key.len()..];
    let after = after.trim_start();
    let bytes = after.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }
    if bytes[0] == b'"' || bytes[0] == b'\'' {
        let quote = bytes[0];
        let rest = &after[1..];
        return match rest.as_bytes().iter().position(|&b| b == quote) {
            Some(end) => rest[..end].to_string(),
            None => rest.to_string(),
        };
    }
    after
        .split(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
        .next()
        .unwrap_or("")
        .to_string()
}

fn html_decode(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_epub(path: &Path, files: &[(&str, &str)]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, body) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        // minimal 1x1 png
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
            0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
            0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
            0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xA7, 0x35, 0x81, 0x84, 0x00, 0x00, 0x00,
            0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        zip.start_file("OEBPS/images/p1.png", options).unwrap();
        zip.write_all(png).unwrap();
        zip.start_file("OEBPS/images/p2.png", options).unwrap();
        zip.write_all(png).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn lists_spine_images_from_xhtml() {
        let dir = std::env::temp_dir().join(format!("jmr-epub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.epub");
        write_epub(
            &path,
            &[
                (
                    "META-INF/container.xml",
                    r#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
                ),
                (
                    "OEBPS/content.opf",
                    r#"<?xml version="1.0"?>
                    <package>
                      <manifest>
                        <item id="n1" href="text/1.xhtml" media-type="application/xhtml+xml"/>
                        <item id="n2" href="text/2.xhtml" media-type="application/xhtml+xml"/>
                        <item id="i1" href="images/p1.png" media-type="image/png"/>
                        <item id="i2" href="images/p2.png" media-type="image/png"/>
                      </manifest>
                      <spine>
                        <itemref idref="n1"/>
                        <itemref idref="n2"/>
                      </spine>
                    </package>"#,
                ),
                (
                    "OEBPS/text/1.xhtml",
                    r#"<html><body><img src="../images/p1.png"/></body></html>"#,
                ),
                (
                    "OEBPS/text/2.xhtml",
                    r#"<html><body><svg><image xlink:href="../images/p2.png"/></svg></body></html>"#,
                ),
            ],
        );
        let (rels, indices) = list(&path).unwrap();
        assert_eq!(rels, vec!["OEBPS/images/p1.png", "OEBPS/images/p2.png"]);
        assert_eq!(indices.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_and_strip_dots() {
        assert_eq!(resolve_href("OEBPS/text", "../images/a.png"), "OEBPS/images/a.png");
        assert_eq!(strip_dot_segments("a/./b/../c"), "a/c");
    }
}
