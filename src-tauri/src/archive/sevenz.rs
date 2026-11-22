use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sevenz_rust2::{ArchiveReader, Password};

use super::extract::ExtractJob;
use super::names::{is_image, is_junk, normalize_rel};
use super::{ERR_PASSWORD_INCORRECT, ERR_PASSWORD_REQUIRED};

pub struct SevenBook {
    pub path: std::path::PathBuf,
    pub names: Vec<String>,
    pub password: Option<String>,
}

impl SevenBook {
    pub fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        let name = self.names.get(index).ok_or("page is out of range")?;
        let mut reader = open_reader(&self.path, self.password.as_deref())?;
        reader
            .read_file(name)
            .map_err(|err| map_7z_err(err, self.password.is_some()))
    }
}

struct Listed {
    sort_key: String,
    original: String,
}

fn password_of(password: Option<&str>) -> Password {
    match password {
        Some(value) if !value.is_empty() => Password::from(value),
        _ => Password::empty(),
    }
}

fn map_7z_err(err: impl std::fmt::Display, had_password: bool) -> String {
    let message = err.to_string();
    if message.contains("PasswordRequired") {
        return if had_password {
            ERR_PASSWORD_INCORRECT.to_string()
        } else {
            ERR_PASSWORD_REQUIRED.to_string()
        };
    }
    if message.contains("MaybeBadPassword") {
        return if had_password {
            ERR_PASSWORD_INCORRECT.to_string()
        } else {
            ERR_PASSWORD_REQUIRED.to_string()
        };
    }
    message
}

fn open_reader(path: &Path, password: Option<&str>) -> Result<ArchiveReader<File>, String> {
    ArchiveReader::open(path, password_of(password))
        .map_err(|err| map_7z_err(err, password.is_some()))
}

fn archive_is_encrypted(reader: &ArchiveReader<File>) -> bool {
    let aes_id = sevenz_rust2::EncoderMethod::AES256_SHA256.id();
    reader.archive().blocks.iter().any(|block| {
        block
            .coders
            .iter()
            .any(|coder| coder.encoder_method_id() == aes_id)
    })
}

fn list_entries(path: &Path, password: Option<&str>) -> Result<(bool, bool, Vec<Listed>), String> {
    let reader = open_reader(path, password)?;
    let solid = reader.archive().is_solid;
    let encrypted = archive_is_encrypted(&reader);
    let mut items = Vec::new();
    for entry in &reader.archive().files {
        if entry.is_directory() {
            continue;
        }
        let original = entry.name().to_string();
        let sort_key = normalize_rel(&original);
        if is_image(&sort_key) && !is_junk(&sort_key) {
            items.push(Listed { sort_key, original });
        }
    }
    items.sort_by(|a, b| super::names::natural_cmp(&a.sort_key, &b.sort_key));
    Ok((solid, encrypted, items))
}

pub fn verify_password(path: &Path, first_name: &str, password: Option<&str>) -> Result<(), String> {
    let mut reader = open_reader(path, password)?;
    reader
        .read_file(first_name)
        .map_err(|err| map_7z_err(err, password.is_some()))?;
    Ok(())
}

pub fn probe(
    path: std::path::PathBuf,
    names: Vec<String>,
    password: Option<String>,
    session_id: u64,
    alive: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(usize),
) -> Vec<super::measure::PageMetric> {
    let mut pages = Vec::new();
    for (index, name) in names.iter().enumerate() {
        if !alive() {
            return pages;
        }
        let Ok(mut reader) = open_reader(&path, password.as_deref()) else {
            return pages;
        };
        if let Ok(bytes) = reader.read_file(name) {
            if let Some(size) = super::measure::bytes(&bytes) {
                pages.push(super::measure::metric(session_id, index, size, None));
            }
        }
        on_progress(index + 1);
    }
    pages
}

pub fn list(
    path: &Path,
    password: Option<&str>,
) -> Result<(bool, bool, Vec<String>, Vec<String>), String> {
    let (solid, encrypted, items) = list_entries(path, password)?;
    let rels = items.iter().map(|item| item.sort_key.clone()).collect();
    let originals = items.into_iter().map(|item| item.original).collect();
    Ok((solid, encrypted, rels, originals))
}

pub fn extract(
    path: std::path::PathBuf,
    originals: Vec<String>,
    password: Option<String>,
    job: ExtractJob,
) {
    let mut index_of = HashMap::<String, usize>::new();
    for (index, name) in originals.iter().enumerate() {
        index_of.entry(normalize_rel(name)).or_insert(index);
    }
    let files = super::cache::file_names(&originals);
    let mut reader = match open_reader(&path, password.as_deref()) {
        Ok(reader) => reader,
        Err(err) => {
            job.fail(err);
            return;
        }
    };
    let mut failed: Option<String> = None;
    let result = reader.for_each_entries(|entry, reader| {
        if job.cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled").into());
        }
        let sort_key = normalize_rel(entry.name());
        if let Some(&index) = index_of.get(&sort_key) {
            job.begin_page(index);
            let (dest, _) = job.destination(index, &originals[index]);
            match write_reader(reader, &dest) {
                Ok(()) => job.mark_written(index, dest),
                Err(err) => {
                    failed = Some(err);
                    return Err(io::Error::new(io::ErrorKind::Other, "write").into());
                }
            }
        } else if let Err(err) = io::copy(reader, &mut io::sink()) {
            failed = Some(err.to_string());
            return Err(err.into());
        }
        Ok(true)
    });
    match result {
        Ok(()) if !job.cancelled() => job.succeed(files),
        Err(err) if err.to_string().contains("cancelled") || job.cancelled() => job.abort(),
        Err(err) => {
            let message = failed.unwrap_or_else(|| err.to_string());
            job.fail(map_7z_err(message, password.is_some()));
        }
        Ok(()) => job.abort(),
    }
}

fn write_reader(reader: &mut dyn Read, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = File::create(dest).map_err(|err| err.to_string())?;
    io::copy(reader, &mut file).map_err(|err| err.to_string())?;
    Ok(())
}
