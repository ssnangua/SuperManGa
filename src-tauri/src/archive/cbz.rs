use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use zip::result::ZipError;
use zip::ZipArchive;

use super::measure::{self, PageMetric};
use super::names::{decode_zip_name, is_image, is_junk, normalize_rel};
use super::{ERR_PASSWORD_INCORRECT, ERR_PASSWORD_REQUIRED};

pub struct ZipBook {
    pub path: PathBuf,
    pub indices: Vec<usize>,
    pub password: Option<String>,
}

impl ZipBook {
    pub fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        let file = File::open(&self.path).map_err(|err| err.to_string())?;
        let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|err| err.to_string())?;
        let zip_index = *self.indices.get(index).ok_or("page is out of range")?;
        let mut entry = open_entry(&mut archive, zip_index, self.password.as_deref())?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|err| err.to_string())?;
        Ok(bytes)
    }

    pub fn probe_all(
        &self,
        session_id: u64,
        alive: &dyn Fn() -> bool,
        on_progress: &mut dyn FnMut(usize),
    ) -> Vec<PageMetric> {
        let mut pages = Vec::new();
        let Ok(file) = File::open(&self.path) else {
            return pages;
        };
        let Ok(mut archive) = ZipArchive::new(BufReader::new(file)) else {
            return pages;
        };
        for (page_index, zip_index) in self.indices.iter().copied().enumerate() {
            if !alive() {
                return pages;
            }
            if let Ok(entry) = open_entry(&mut archive, zip_index, self.password.as_deref()) {
                let name = normalize_rel(&decode_zip_name(entry.name_raw()));
                if let Some(size) = measure::reader(entry) {
                    pages.push(measure::metric(session_id, page_index, size, Some(name)));
                }
            }
            on_progress(page_index + 1);
        }
        pages
    }
}

fn open_entry<'a, R: std::io::Read + std::io::Seek>(
    archive: &'a mut ZipArchive<R>,
    zip_index: usize,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a, R>, String> {
    let result = match password {
        Some(password) => archive.by_index_decrypt(zip_index, password.as_bytes()),
        None => archive.by_index(zip_index),
    };
    result.map_err(|err| map_zip_err(err, password.is_some()))
}

fn map_zip_err(err: ZipError, had_password: bool) -> String {
    match err {
        ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED) => {
            ERR_PASSWORD_REQUIRED.to_string()
        }
        ZipError::InvalidPassword => {
            if had_password {
                ERR_PASSWORD_INCORRECT.to_string()
            } else {
                ERR_PASSWORD_REQUIRED.to_string()
            }
        }
        other => other.to_string(),
    }
}

pub fn any_encrypted(path: &Path, indices: &[usize]) -> Result<bool, String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|err| err.to_string())?;
    for &index in indices {
        let entry = archive
            .by_index_raw(index)
            .map_err(|err| err.to_string())?;
        if entry.encrypted() {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn verify_password(path: &Path, indices: &[usize], password: &str) -> Result<(), String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|err| err.to_string())?;
    let mut target = None;
    for &index in indices {
        let Ok(entry) = archive.by_index_raw(index) else {
            continue;
        };
        if entry.encrypted() {
            target = Some(index);
            break;
        }
    }
    let index = target.or_else(|| indices.first().copied());
    let Some(index) = index else {
        return Ok(());
    };
    // Re-open: by_index_raw leaves the reader mid-stream for a later decrypt read.
    let file = File::open(path).map_err(|err| err.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|err| err.to_string())?;
    let mut entry = open_entry(&mut archive, index, Some(password))?;
    // Force decryption / CRC so ZipCrypto false positives are less likely to pass.
    let mut sink = Vec::new();
    entry
        .read_to_end(&mut sink)
        .map_err(|_| ERR_PASSWORD_INCORRECT.to_string())?;
    Ok(())
}

pub fn list(path: &Path) -> Result<(Vec<String>, Vec<usize>), String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|err| err.to_string())?;
    let mut items = Vec::new();
    for index in 0..archive.len() {
        // Use name_raw + GBK fallback. `name_for_index` maps non-UTF8 bytes through
        // CP437, which turns Chinese Windows (GBK) zip names into mojibake.
        let Ok(entry) = archive.by_index_raw(index) else {
            continue;
        };
        let name = normalize_rel(&decode_zip_name(entry.name_raw()));
        if name.ends_with('/') || name.ends_with('\\') {
            continue;
        }
        if is_image(&name) && !is_junk(&name) {
            items.push((name, index));
        }
    }
    items.sort_by(|a, b| super::names::natural_cmp(&a.0, &b.0));
    let rels = items.iter().map(|(name, _)| name.clone()).collect();
    let indices = items.iter().map(|(_, index)| *index).collect();
    Ok((rels, indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn lists_only_images_in_natural_order() {
        let dir = std::env::temp_dir().join(format!("jmr-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.zip");
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("10.jpg", options).unwrap();
        zip.write_all(b"a").unwrap();
        zip.start_file("2.jpg", options).unwrap();
        zip.write_all(b"b").unwrap();
        zip.start_file("sub/1.png", options).unwrap();
        zip.write_all(b"c").unwrap();
        zip.start_file("__MACOSX/x.jpg", options).unwrap();
        zip.write_all(b"d").unwrap();
        zip.start_file("note.txt", options).unwrap();
        zip.write_all(b"e").unwrap();
        zip.finish().unwrap();

        let (rels, indices) = list(&path).unwrap();
        assert_eq!(rels, vec!["2.jpg", "10.jpg", "sub/1.png"]);
        assert_eq!(indices.len(), 3);
        let book = ZipBook {
            path: path.clone(),
            indices,
            password: None,
        };
        assert_eq!(book.read(0).unwrap(), b"b");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lists_gbk_named_entry_from_handwritten_zip() {
        // Minimal stored zip whose file name is GBK bytes and UTF-8 flag is clear.
        let dir = std::env::temp_dir().join(format!("jmr-zip-gbk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.zip");
        let (name_raw, _, _) = encoding_rs::GBK.encode("混搭封面.jpg");
        let data = b"jpeg";
        let mut bytes = Vec::new();
        // local file header
        bytes.extend_from_slice(&0x04034b50u32.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags (no UTF-8)
        bytes.extend_from_slice(&0u16.to_le_bytes()); // stored
        bytes.extend_from_slice(&0u16.to_le_bytes()); // time
        bytes.extend_from_slice(&0u16.to_le_bytes()); // date
        bytes.extend_from_slice(&0u32.to_le_bytes()); // crc (unchecked)
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(name_raw.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // extra
        bytes.extend_from_slice(&name_raw);
        bytes.extend_from_slice(data);
        let local_len = bytes.len() as u32;
        // central directory
        let cd_offset = bytes.len() as u32;
        bytes.extend_from_slice(&0x02014b50u32.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes()); // version made by
        bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        bytes.extend_from_slice(&0u16.to_le_bytes()); // method
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(name_raw.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // extra
        bytes.extend_from_slice(&0u16.to_le_bytes()); // comment
        bytes.extend_from_slice(&0u16.to_le_bytes()); // disk
        bytes.extend_from_slice(&0u16.to_le_bytes()); // int attr
        bytes.extend_from_slice(&0u32.to_le_bytes()); // ext attr
        bytes.extend_from_slice(&0u32.to_le_bytes()); // local header offset
        bytes.extend_from_slice(&name_raw);
        let cd_size = (bytes.len() as u32) - cd_offset;
        // end of central directory
        bytes.extend_from_slice(&0x06054b50u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&cd_size.to_le_bytes());
        bytes.extend_from_slice(&cd_offset.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        let _ = local_len;
        std::fs::write(&path, &bytes).unwrap();

        let (rels, _) = list(&path).unwrap();
        assert_eq!(rels, vec!["混搭封面.jpg"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lists_real_gbk_comic_zip_when_present() {
        let path = Path::new(
            r"H:\漫画\哆啦A梦\[藤子・F・不二雄] 哆啦A夢 大合輯 全彩版 [漢化&雙頁8K版 by milianaisu][20卷][青文]\[藤子・F・不二雄] 哆啦A夢 大合輯 全彩版 VOL.01.zip",
        );
        if !path.is_file() {
            return;
        }
        let (rels, _) = list(path).unwrap();
        assert!(
            rels.iter().any(|name| name.contains("混搭封面")),
            "cover name missing, first={:?}",
            rels.first()
        );
        assert!(
            rels.iter().any(|name| name.contains("目录")),
            "toc name missing (GBK-as-UTF8 trap), sample={:?}",
            rels.get(1)
        );
        assert!(
            !rels.iter().any(|name| name.contains('Ŀ') || name.contains('¼')),
            "latin accident still present: {:?}",
            rels.iter().take(3).collect::<Vec<_>>()
        );
    }
}
