//! Release notifications only: no downloaded executable is installed or run.
use serde::{Deserialize, Serialize};
use std::time::Duration;

const REPOSITORY: &str = "Table-IQ/Table-IQ";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset { name: String, state: String }

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    version: String,
    release_url: String,
}

fn select_update(releases: Vec<Release>, current: &str) -> Result<Option<AvailableUpdate>, String> {
    let current = semver::Version::parse(current).map_err(|_| "Invalid installed version")?;
    let mut candidates: Vec<_> = releases.into_iter().filter_map(|r| {
        if r.draft || !r.assets.iter().any(|a| a.state == "uploaded" && a.name.ends_with(".dmg")) {
            return None;
        }
        // Both public developer-preview tags and future stable tags are supported.
        let raw = r.tag_name.strip_prefix("preview-v").or_else(|| r.tag_name.strip_prefix('v'))?;
        let version = semver::Version::parse(raw).ok()?;
        if version <= current || !r.tag_name.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)) {
            return None;
        }
        Some((version, r.tag_name))
    }).collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(candidates.into_iter().next().map(|(version, tag)| AvailableUpdate {
        version: version.to_string(),
        release_url: format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
    }))
}

/// Public, unauthenticated request; no installation identity or database information.
#[tauri::command]
pub async fn check_for_updates() -> Result<Option<AvailableUpdate>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("TableIQ-release-check")
        .build().map_err(|_| "Could not start the update check")?;
    // /latest omits prereleases. Preview users need the published release list.
    let mut response = client.get(format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page=30"))
        .header("Accept", "application/vnd.github+json")
        .send().await.map_err(|_| "Could not reach GitHub. Check your internet connection and try again.")?;
    if !response.status().is_success() {
        return Err(format!("GitHub returned HTTP {}. Try again later or visit the releases page.", response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "The update response was interrupted")? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err("The update response was too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let releases = serde_json::from_slice(&bytes).map_err(|_| "GitHub returned an unexpected update response")?;
    select_update(releases, env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str, draft: bool, downloadable: bool) -> Release {
        Release { tag_name: tag.into(), draft, assets: if downloadable {
            vec![Asset { name: "TableIQ_aarch64.dmg".into(), state: "uploaded".into() }]
        } else { vec![] } }
    }
    #[test]
    fn finds_newest_downloadable_preview_numerically() {
        let result = select_update(vec![release("preview-v0.8.9", false, true), release("preview-v0.8.16", false, true), release("v0.9.0", false, false)], "0.8.15").unwrap().unwrap();
        assert_eq!(result.version, "0.8.16");
        assert_eq!(result.release_url, "https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.16");
    }
    #[test]
    fn never_recommends_draft_equal_or_older_releases() {
        assert!(select_update(vec![release("v0.8.15", false, true), release("preview-v0.8.14", false, true), release("v0.9.0", true, true)], "0.8.15").unwrap().is_none());
    }
    #[test]
    fn rejects_invalid_tags_and_respects_semver_prerelease_order() {
        let result = select_update(vec![release("v0.8.16-rc.1", false, true), release("v0.8.16", false, true), release("v0.9.0/evil", false, true)], "0.8.15").unwrap().unwrap();
        assert_eq!(result.version, "0.8.16");
        assert!(select_update(vec![], "invalid").is_err());
    }
}
