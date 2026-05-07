//! GitHub Releases API client — "is a newer Smriti out?"
//!
//! This module owns the HTTP call, JSON parsing, semver comparison,
//! and all the error cases that come with talking to a remote API.
//! The Tauri command handler just turns the result into a DTO.
//!
//! Everything here is opt-in — the call only runs when the user has
//! enabled "Automatically check for updates" in Settings or explicitly
//! clicks "Check for updates now". See PRIVACY.md.

use semver::Version;
use serde::Deserialize;
use std::time::Duration;

/// Where GitHub's API lives. Overridable via `SMRITI_GITHUB_API_URL`
/// (legacy `PHOTOVAULT_GITHUB_API_URL` still honoured) for integration
/// tests without hitting the real API.
const DEFAULT_API_URL: &str =
    "https://api.github.com/repos/ChivukulaVirinchi/photovault/releases/latest";

/// Timeout for the single API call. Short enough that a hung network
/// doesn't block app startup, long enough to survive normal jitter.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// A release asset attached to a GitHub release (MSI, AppImage,
/// .dmg, SHA256SUMS, etc.). `self_replace` consumes these.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(rename = "size")]
    pub size_bytes: u64,
}

/// Raw shape of the GitHub `/releases/latest` JSON payload — just
/// the fields we actually use.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

/// Normalized latest-release descriptor the rest of the app talks to.
#[derive(Debug, Clone)]
#[allow(dead_code)] // `version` and `is_prerelease` aren't displayed
                    // yet but are part of the public shape for
                    // release-channel selection (Phase 3+) and tests.
pub struct LatestRelease {
    pub version: Version,
    pub tag_name: String,
    pub html_url: String,
    pub body: String,
    pub assets: Vec<ReleaseAsset>,
    pub is_prerelease: bool,
}

/// Result of a single update check: the running version, the latest
/// published one, and whether an upgrade is available.
#[derive(Debug, Clone)]
pub struct UpdateStatus {
    pub current: Version,
    pub latest: LatestRelease,
    pub newer_available: bool,
}

/// Typed errors the UI/handler needs to distinguish so the user sees
/// a useful message instead of a raw `reqwest` string.
#[derive(Debug, thiserror::Error)]
pub enum UpdateCheckError {
    #[error("failed to reach GitHub's release API: {0}")]
    Network(String),

    #[error("GitHub returned HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },

    #[error("failed to parse GitHub's release JSON: {0}")]
    Parse(String),

    #[error("couldn't parse the release's tag as a semver: {0}")]
    InvalidTag(String),

    #[error("couldn't parse Smriti's own version as a semver: {0}")]
    InvalidSelfVersion(String),
}

/// Check whether a newer release is available.
///
/// Hits `api.github.com/repos/ChivukulaVirinchi/photovault/releases/latest`
/// by default. Set `SMRITI_GITHUB_API_URL` (or the legacy
/// `PHOTOVAULT_GITHUB_API_URL`) to override — used by integration
/// tests to point at a local mock.
pub async fn check_for_updates() -> Result<UpdateStatus, UpdateCheckError> {
    let url = std::env::var("SMRITI_GITHUB_API_URL")
        .or_else(|_| std::env::var("PHOTOVAULT_GITHUB_API_URL"))
        .unwrap_or_else(|_| DEFAULT_API_URL.to_string());

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("smriti/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| UpdateCheckError::Network(e.to_string()))?;

    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| UpdateCheckError::Network(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(UpdateCheckError::HttpStatus {
            status: status.as_u16(),
            url,
        });
    }

    let payload: GithubRelease = response
        .json()
        .await
        .map_err(|e| UpdateCheckError::Parse(e.to_string()))?;

    let latest_version = parse_tag_as_version(&payload.tag_name)?;
    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| UpdateCheckError::InvalidSelfVersion(e.to_string()))?;

    let newer_available = latest_version > current_version;

    Ok(UpdateStatus {
        current: current_version,
        newer_available,
        latest: LatestRelease {
            version: latest_version,
            tag_name: payload.tag_name,
            html_url: payload.html_url,
            body: payload.body,
            assets: payload.assets,
            is_prerelease: payload.prerelease,
        },
    })
}

/// Parse a GitHub tag name into a `semver::Version`. Handles the
/// common `v1.2.3` form by stripping the leading `v` before parsing.
fn parse_tag_as_version(tag: &str) -> Result<Version, UpdateCheckError> {
    let trimmed = tag.trim().trim_start_matches('v').trim_start_matches('V');
    Version::parse(trimmed).map_err(|e| UpdateCheckError::InvalidTag(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_parses_with_v_prefix() {
        assert_eq!(
            parse_tag_as_version("v1.2.3").unwrap(),
            Version::parse("1.2.3").unwrap()
        );
    }

    #[test]
    fn tag_parses_without_v_prefix() {
        assert_eq!(
            parse_tag_as_version("1.2.3").unwrap(),
            Version::parse("1.2.3").unwrap()
        );
    }

    #[test]
    fn tag_parses_prerelease() {
        let v = parse_tag_as_version("v1.0.0-beta.1").unwrap();
        assert_eq!(v, Version::parse("1.0.0-beta.1").unwrap());
        assert!(!v.pre.is_empty(), "prerelease segment should be present");
    }

    #[test]
    fn tag_parses_uppercase_v() {
        assert_eq!(
            parse_tag_as_version("V2.0.0").unwrap(),
            Version::parse("2.0.0").unwrap()
        );
    }

    #[test]
    fn tag_rejects_nonsemver() {
        assert!(matches!(
            parse_tag_as_version("banana"),
            Err(UpdateCheckError::InvalidTag(_))
        ));
    }

    #[test]
    fn tag_rejects_empty() {
        assert!(matches!(
            parse_tag_as_version(""),
            Err(UpdateCheckError::InvalidTag(_))
        ));
    }
}
