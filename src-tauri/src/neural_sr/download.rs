use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use super::ProgressEvent;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub proxy_enabled: Option<bool>,
    pub proxy_type: Option<String>,
    pub proxy_host: Option<String>,
    pub proxy_port: Option<u16>,
    pub proxy_user: Option<String>,
    pub proxy_pass: Option<String>,
}

fn proxy_url(cfg: &NetworkConfig) -> Result<Option<String>, String> {
    if !cfg.proxy_enabled.unwrap_or(false) {
        return Ok(None);
    }
    let host = cfg.proxy_host.as_deref().unwrap_or("").trim();
    if host.is_empty() {
        return Err("proxy host is empty".into());
    }
    let port = cfg.proxy_port.unwrap_or(0);
    if port == 0 {
        return Err("proxy port is invalid".into());
    }
    let scheme = match cfg.proxy_type.as_deref().unwrap_or("http").to_ascii_lowercase().as_str() {
        "socks4" | "socks4a" => "socks4",
        "socks5" | "socks" => "socks5",
        _ => "http",
    };
    let user = cfg.proxy_user.as_deref().unwrap_or("").trim();
    let pass = cfg.proxy_pass.as_deref().unwrap_or("");
    if !user.is_empty() {
        let enc_user = urlencoding_lite(user);
        let enc_pass = urlencoding_lite(pass);
        Ok(Some(format!(
            "{scheme}://{enc_user}:{enc_pass}@{host}:{port}"
        )))
    } else {
        Ok(Some(format!("{scheme}://{host}:{port}")))
    }
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub(crate) fn build_agent(cfg: &NetworkConfig) -> Result<ureq::Agent, String> {
    build_agent_with_timeout(cfg, Duration::from_secs(120))
}

pub(crate) fn build_agent_with_timeout(
    cfg: &NetworkConfig,
    timeout: Duration,
) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new().timeout(timeout);
    if let Some(proxy) = proxy_url(cfg)? {
        let proxy = ureq::Proxy::new(&proxy).map_err(|err| err.to_string())?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build())
}

pub fn download_to_file_with_options(
    app: &AppHandle,
    kind: &str,
    id: &str,
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    cfg: &NetworkConfig,
    emit_completion: bool,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|s| s.to_str()).unwrap_or("bin")
    ));
    let agent = build_agent(cfg)?;
    let response = agent
        .get(url)
        .call()
        .map_err(|err| format!("download failed: {err}"))?;
    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = File::create(&part).map_err(|err| err.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut done = 0u64;
    let mut last_emit = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|err| err.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        hasher.update(&buf[..n]);
        done += n as u64;
        let crossed_percent = match (total, last_emit) {
            (Some(t), last) if t > 0 => {
                let prev = ((last.min(t) as f64 / t as f64) * 100.0).floor() as u32;
                let curr = ((done.min(t) as f64 / t as f64) * 100.0).floor() as u32;
                curr > prev
            }
            _ => false,
        };
        if done - last_emit >= 512 * 1024 || crossed_percent || total.is_some_and(|t| done >= t) {
            last_emit = done;
            let percent = total.map(|t| ((done.min(t) as f64 / t as f64) * 100.0).round() as u32);
            let _ = app.emit(
                "asset-progress",
                ProgressEvent {
                    kind: kind.into(),
                    id: id.into(),
                    done,
                    total,
                    percent,
                },
            );
        }
    }
    file.flush().map_err(|err| err.to_string())?;
    drop(file);
    let digest = hex::encode(hasher.finalize());
    if let Some(expected) = expected_sha256.map(str::trim).filter(|s| !s.is_empty()) {
        if !digest.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&part);
            return Err(format!(
                "checksum mismatch: expected {expected}, got {digest}"
            ));
        }
    }
    if dest.exists() {
        fs::remove_file(dest).map_err(|err| err.to_string())?;
    }
    fs::rename(&part, dest).map_err(|err| err.to_string())?;
    if emit_completion {
        let _ = app.emit(
            "asset-progress",
            ProgressEvent {
                kind: kind.into(),
                id: id.into(),
                done,
                total: total.or(Some(done)),
                percent: Some(100),
            },
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn test_download_proxy(cfg: NetworkConfig, test_url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let url = test_url.trim();
        if url.is_empty() {
            return Err("test url is empty".into());
        }
        let agent = build_agent(&cfg)?;
        let response = agent
            .get(url)
            .call()
            .map_err(|err| format!("proxy test failed: {err}"))?;
        Ok(format!("HTTP {}", response.status()))
    })
    .await
    .map_err(|err| err.to_string())?
}

