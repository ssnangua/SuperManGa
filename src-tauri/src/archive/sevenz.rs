use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sevenz_rust2::{ArchiveReader, Password};

use super::extract::ExtractJob;
use super::names::{is_image, is_junk, normalize_rel};

pub struct SevenBook {
    pub path: std::path::PathBuf,
    pub names: Vec<String>,
}

impl SevenBook {
    pub fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        let name = self.names.get(index).ok_or("page is out of range")?;
        let mut reader = ArchiveReader::open(&self.path, Password::empty()).map_err(|err| err.to_string())?;
        reader.read_file(name).map_err(|err| err.to_string())
    }
}

struct Listed {
    sort_key: String,
    original: String,
}

fn list_entries(path: &Path) -> Result<(bool, Vec<Listed>), String> {
    let reader = ArchiveReader::open(path, Password::empty()).map_err(|err| err.to_string())?;
    let solid = reader.archive().is_solid;
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
    Ok((solid, items))
}

pub fn probe(
    path: std::path::PathBuf,
    names: Vec<String>,
    session_id: u64,
    alive: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(usize),
) -> Vec<super::measure::PageMetric> {
    let mut pages = Vec::new();
    for (index, name) in names.iter().enumerate() {
        if !alive() {
            return pages;
        }
        let Ok(mut reader) = ArchiveReader::open(&path, Password::empty()) else {
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

pub fn list(path: &Path) -> Result<(bool, Vec<String>, Vec<String>), String> {
    let (solid, items) = list_entries(path)?;
    let rels = items.iter().map(|item| item.sort_key.clone()).collect();
    let originals = items.into_iter().map(|item| item.original).collect();
    Ok((solid, rels, originals))
}

pub fn extract(path: std::path::PathBuf, originals: Vec<String>, job: ExtractJob) {
    let mut index_of = HashMap::<String, usize>::new();
    for (index, name) in originals.iter().enumerate() {
        index_of.entry(normalize_rel(name)).or_insert(index);
    }
    let files = super::cache::file_names(&originals);
    let mut reader = match ArchiveReader::open(&path, Password::empty()) {
        Ok(reader) => reader,
        Err(err) => {
            job.fail(err.to_string());
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
        Err(err) => job.fail(failed.unwrap_or_else(|| err.to_string())),
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
