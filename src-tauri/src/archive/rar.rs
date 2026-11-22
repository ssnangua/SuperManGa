use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use unrar_ng::Archive;

use super::extract::ExtractJob;
use super::names::{is_image, is_junk, normalize_rel};
use super::{ERR_PASSWORD_INCORRECT, ERR_PASSWORD_REQUIRED};

fn header_name(path: &Path) -> String {
    normalize_rel(&path.to_string_lossy())
}

fn map_rar_err(err: impl std::fmt::Display, had_password: bool) -> String {
    let message = err.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("password for encrypted archive not specified")
        || lower.contains("missingpassword")
    {
        return ERR_PASSWORD_REQUIRED.to_string();
    }
    if lower.contains("wrong password") || lower.contains("badpassword") {
        return if had_password {
            ERR_PASSWORD_INCORRECT.to_string()
        } else {
            ERR_PASSWORD_REQUIRED.to_string()
        };
    }
    if lower.contains("password") {
        return if had_password {
            ERR_PASSWORD_INCORRECT.to_string()
        } else {
            ERR_PASSWORD_REQUIRED.to_string()
        };
    }
    message
}

fn archive_with_password<'a>(path: &'a Path, password: Option<&'a [u8]>) -> Archive<'a> {
    match password {
        Some(password) => Archive::with_password(path, password),
        None => Archive::new(path),
    }
}

pub fn list(path: &Path, password: Option<&[u8]>) -> Result<(Vec<String>, bool), String> {
    let mut archive = archive_with_password(path, password)
        .open_for_listing()
        .map_err(|err| map_rar_err(err, password.is_some()))?;
    let mut rels = Vec::new();
    let mut encrypted = archive.has_encrypted_headers();
    for item in &mut archive {
        let header = item.map_err(|err| map_rar_err(err, password.is_some()))?;
        if header.is_encrypted() {
            encrypted = true;
        }
        if header.is_directory() {
            continue;
        }
        let name = header_name(&header.filename);
        if is_image(&name) && !is_junk(&name) {
            rels.push(name);
        }
    }
    rels.sort_by(|a, b| super::names::natural_cmp(a, b));
    Ok((rels, encrypted))
}

/// Confirm the password by reading the first image entry (content encryption).
pub fn verify_password(path: &Path, first_rel: &str, password: Option<&[u8]>) -> Result<(), String> {
    let mut archive = archive_with_password(path, password)
        .open_for_processing()
        .map_err(|err| map_rar_err(err, password.is_some()))?;
    loop {
        let header = match archive.read_header() {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(err) => return Err(map_rar_err(err, password.is_some())),
        };
        let name = header_name(&header.entry().filename);
        let directory = header.entry().is_directory();
        if !directory && name == first_rel {
            match header.read() {
                Ok(_) => return Ok(()),
                Err(err) => return Err(map_rar_err(err, password.is_some())),
            }
        }
        archive = match header.skip() {
            Ok(next) => next,
            Err(err) => return Err(map_rar_err(err, password.is_some())),
        };
    }
    Ok(())
}

pub fn extract(path: std::path::PathBuf, rels: Vec<String>, password: Option<String>, job: ExtractJob) {
    let mut index_of = HashMap::<String, usize>::new();
    for (index, name) in rels.iter().enumerate() {
        index_of.entry(name.clone()).or_insert(index);
    }
    let files = super::cache::file_names(&rels);
    let password_bytes = password.as_ref().map(|value| value.as_bytes());
    let mut archive = match archive_with_password(&path, password_bytes).open_for_processing() {
        Ok(archive) => archive,
        Err(err) => {
            job.fail(map_rar_err(err, password.is_some()));
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
                job.fail(map_rar_err(err, password.is_some()));
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
                        job.fail(map_rar_err(err, password.is_some()));
                        return;
                    }
                };
                archive = next;
                job.begin_page(index);
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
                job.fail(map_rar_err(err, password.is_some()));
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
