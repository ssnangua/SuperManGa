use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use unrar_ng::Archive;

use super::extract::ExtractJob;
use super::names::{is_image, is_junk, normalize_rel};

fn header_name(path: &Path) -> String {
    normalize_rel(&path.to_string_lossy())
}

pub fn list(path: &Path) -> Result<Vec<String>, String> {
    let mut archive = Archive::new(path)
        .open_for_listing()
        .map_err(|err| err.to_string())?;
    let mut rels = Vec::new();
    for item in &mut archive {
        let header = item.map_err(|err| err.to_string())?;
        if header.is_directory() {
            continue;
        }
        let name = header_name(&header.filename);
        if is_image(&name) && !is_junk(&name) {
            rels.push(name);
        }
    }
    rels.sort_by(|a, b| super::names::natural_cmp(a, b));
    Ok(rels)
}

pub fn extract(path: std::path::PathBuf, rels: Vec<String>, job: ExtractJob) {
    let mut index_of = HashMap::<String, usize>::new();
    for (index, name) in rels.iter().enumerate() {
        index_of.entry(name.clone()).or_insert(index);
    }
    let files = super::cache::file_names(&rels);
    let mut archive = match Archive::new(&path).open_for_processing() {
        Ok(archive) => archive,
        Err(err) => {
            job.fail(err.to_string());
            return;
        }
    };
    loop {
        if job.cancelled() {
            job.abort();
            return;
        }
        let header = match archive.read_header() {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(err) => {
                job.fail(err.to_string());
                return;
            }
        };
        let name = header_name(&header.entry().filename);
        let directory = header.entry().is_directory();
        if !directory {
            if let Some(&index) = index_of.get(&name) {
                let (bytes, next) = match header.read() {
                    Ok(pair) => pair,
                    Err(err) => {
                        job.fail(err.to_string());
                        return;
                    }
                };
                archive = next;
                let (dest, _) = job.destination(index, &rels[index]);
                if let Err(err) = write_bytes(&dest, &bytes) {
                    job.fail(err);
                    return;
                }
                job.mark_written(index, dest);
                continue;
            }
        }
        archive = match header.skip() {
            Ok(next) => next,
            Err(err) => {
                job.fail(err.to_string());
                return;
            }
        };
    }
    if job.cancelled() {
        job.abort();
    } else {
        job.succeed(files);
    }
}

fn write_bytes(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = File::create(dest).map_err(|err| err.to_string())?;
    file.write_all(bytes).map_err(|err| err.to_string())?;
    Ok(())
}
