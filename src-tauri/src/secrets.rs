use std::fs;
use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use keyring::Entry;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::app_cache;

const SERVICE: &str = "SuperManGa";
const TRANSLATION_USER: &str = "translation.providerKeys";
const FILE_NAME: &str = "translation.providerKeys.v1";
const APP_BOUND_SALT: &[u8] = b"supermanga-secrets-v1";
/// Stable seed so file-backed secrets survive copying SuperManGaData across machines.
const STABLE_SEED: &[u8] = b"supermanga-portable-secrets-v2";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsEncryptionStatus {
    pub available: bool,
    pub backend: String,
}

fn translation_entry() -> Result<Entry, String> {
    Entry::new(SERVICE, TRANSLATION_USER).map_err(|err| err.to_string())
}

fn secrets_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app_cache::subdir(app, app_cache::SECRETS_DIR)
}

fn secrets_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(secrets_dir(app)?.join(FILE_NAME))
}

fn stable_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(APP_BOUND_SALT);
    hasher.update(STABLE_SEED);
    hasher.update(APP_BOUND_SALT);
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn encrypt_app_bound(plain: &str) -> String {
    let key = stable_key();
    let bytes = plain.as_bytes();
    let mut out = Vec::with_capacity(1 + bytes.len() + 4);
    out.push(1); // version
    let mut checksum = Sha256::new();
    checksum.update(bytes);
    let sum = checksum.finalize();
    for (i, b) in bytes.iter().enumerate() {
        out.push(b ^ key[i % key.len()]);
    }
    out.extend_from_slice(&sum[..4]);
    B64.encode(out)
}

fn decrypt_app_bound(payload_b64: &str) -> Result<String, String> {
    let key = stable_key();
    let buf = B64.decode(payload_b64.trim()).map_err(|err| err.to_string())?;
    if buf.len() < 6 || buf[0] != 1 {
        return Err("invalid secrets payload".into());
    }
    let body = &buf[1..buf.len() - 4];
    let sum = &buf[buf.len() - 4..];
    let mut plain = Vec::with_capacity(body.len());
    for (i, b) in body.iter().enumerate() {
        plain.push(b ^ key[i % key.len()]);
    }
    let mut checksum = Sha256::new();
    checksum.update(&plain);
    let expect = checksum.finalize();
    if sum != &expect[..4] {
        return Err("secrets checksum mismatch".into());
    }
    String::from_utf8(plain).map_err(|err| err.to_string())
}

/// Decode a vault blob: prefer app-bound ciphertext; accept legacy plaintext JSON.
fn decode_stored_blob(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    match decrypt_app_bound(trimmed) {
        Ok(plain) => Ok(plain),
        // Legacy: keyring used to store plaintext JSON (and may have mangled `=` padding).
        Err(_) => Ok(trimmed.to_string()),
    }
}

fn read_file_secret(app: &AppHandle) -> Result<String, String> {
    let path = secrets_file(app)?;
    if !path.exists() {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
    decode_stored_blob(&raw)
}

fn write_file_secret(app: &AppHandle, enc_blob: &str) -> Result<(), String> {
    let path = secrets_file(app)?;
    if enc_blob.trim().is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    fs::write(path, format!("{}\n", enc_blob.trim())).map_err(|err| err.to_string())
}

fn read_keyring_secret() -> Result<Option<String>, String> {
    match translation_entry()?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

fn write_keyring_secret(enc_blob: &str) -> Result<(), String> {
    let entry = translation_entry()?;
    let trimmed = enc_blob.trim();
    if trimmed.is_empty() {
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    } else {
        // Store ciphertext — never plaintext JSON — so OS password managers cannot
        // "normalize" Base64 secrets (e.g. strip trailing `=`).
        entry.set_password(trimmed).map_err(|err| err.to_string())
    }
}

#[tauri::command]
pub fn secrets_encryption_status(app: AppHandle) -> SecretsEncryptionStatus {
    let keyring_ok = translation_entry().is_ok();
    let file_ok = secrets_dir(&app).is_ok();
    if keyring_ok {
        SecretsEncryptionStatus {
            available: true,
            backend: "keyring".into(),
        }
    } else if file_ok {
        SecretsEncryptionStatus {
            available: true,
            backend: "appBound".into(),
        }
    } else {
        SecretsEncryptionStatus {
            available: false,
            backend: "unavailable".into(),
        }
    }
}

#[tauri::command]
pub fn secrets_translation_get(app: AppHandle) -> Result<String, String> {
    match read_keyring_secret() {
        Ok(Some(value)) if !value.trim().is_empty() => {
            return decode_stored_blob(&value);
        }
        Ok(_) => {}
        Err(_) => {}
    }
    read_file_secret(&app)
}

#[tauri::command]
pub fn secrets_translation_set(app: AppHandle, value: String) -> Result<(), String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        let _ = write_keyring_secret("");
        return write_file_secret(&app, "");
    }
    let enc = encrypt_app_bound(&trimmed);
    match write_keyring_secret(&enc) {
        Ok(()) => {
            let _ = write_file_secret(&app, &enc);
            Ok(())
        }
        Err(keyring_err) => write_file_secret(&app, &enc)
            .map_err(|file_err| format!("keyring: {keyring_err}; file: {file_err}")),
    }
}
