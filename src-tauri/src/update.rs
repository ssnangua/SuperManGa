//! Portable-build update check against GitHub Releases (notify only; no in-app install).

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::neural_sr::download::{build_agent_with_timeout, NetworkConfig};

const RELEASES_LATEST_API: &str =
    "https://api.github.com/repos/ssnangua/SuperManGa/releases/latest";
const RELEASES_LATEST_URL: &str =
    "https://github.com/ssnangua/SuperManGa/releases/latest";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: Option<String>,
}

fn strip_v(version: &str) -> &str {
    version.trim().trim_start_matches(['v', 'V'])
}

/// Compare dotted numeric versions; returns true when `remote` is strictly newer than `local`.
fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        strip_v(s)
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let a = parse(remote);
    let b = parse(local);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

fn check_app_update_sync(network: NetworkConfig) -> Result<UpdateCheckResult, String> {
    let current = env!("APP_VERSION").to_string();
    // Keep this short: startup checks must fail fast on bad networks.
    let agent = build_agent_with_timeout(&network, Duration::from_secs(8))?;
    let response = agent
        .get(RELEASES_LATEST_API)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", &format!("supermanga/{current}"))
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if status == 404 {
        return Ok(UpdateCheckResult {
            update_available: false,
            current_version: current.clone(),
            latest_version: current,
            html_url: RELEASES_LATEST_URL.to_string(),
        });
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub API HTTP {status}"));
    }
    let body = response.into_string().map_err(|err| err.to_string())?;
    let release: GithubRelease =
        serde_json::from_str(&body).map_err(|err| err.to_string())?;
    let latest = strip_v(&release.tag_name).to_string();
    let html_url = release
        .html_url
        .filter(|u: &String| !u.trim().is_empty())
        .unwrap_or_else(|| RELEASES_LATEST_URL.to_string());
    Ok(UpdateCheckResult {
        update_available: is_newer(&latest, &current),
        current_version: current,
        latest_version: latest,
        html_url,
    })
}

/// Async so ureq cannot block the UI/main thread when DNS/proxy hangs.
#[tauri::command]
pub async fn check_app_update(network: NetworkConfig) -> Result<UpdateCheckResult, String> {
    tauri::async_runtime::spawn_blocking(move || check_app_update_sync(network))
        .await
        .map_err(|err| err.to_string())?
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_semver_like() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
    }
}
