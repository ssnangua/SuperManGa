use std::fs;
use std::path::{Path, PathBuf};

use super::names::{is_image, is_junk, normalize_rel};

pub fn collect(root: &Path, recursive: bool) -> Result<Vec<(String, PathBuf)>, String> {
    let mut found = Vec::new();
    walk(root, root, recursive, &mut found)?;
    found.sort_by(|a, b| super::names::natural_cmp(&a.0, &b.0));
    Ok(found)
}

fn walk(dir: &Path, root: &Path, recursive: bool, found: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| {
        if dir == root {
            err.to_string()
        } else {
            String::new()
        }
    });
    let entries = match entries {
        Ok(entries) => entries,
        Err(err) if err.is_empty() => return Ok(()),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if recursive {
                walk(&path, root, true, found)?;
            }
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let rel = normalize_rel(&rel);
        if is_image(&rel) && !is_junk(&rel) {
            found.push((rel, path));
        }
    }
    Ok(())
}
