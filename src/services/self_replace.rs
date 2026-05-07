//! Per-install-method update download + install logic.
//!
//! The update checker hands us a `LatestRelease` and the detected
//! `InstallMethod`; this module picks the right artifact, downloads
//! it, verifies its SHA256 against the published `SHA256SUMS`, and
//! either self-replaces the running binary or triggers the platform
//! installer.
//!
//! Critical invariant: for package-manager installs we **never**
//! download or replace anything. The package manager owns the
//! binary; stomping on it would confuse the system. Callers should
//! check `InstallMethod` before calling `install_update`, but the
//! function defensively short-circuits on those variants anyway.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::install_method::InstallMethod;
use super::update_checker::{LatestRelease, ReleaseAsset};

/// What happened when the user clicked "Download update".
#[derive(Debug, Clone)]
#[allow(dead_code)] // variant payload fields are observed via match
                    // arms; Rust's dead-code pass can't see through.
pub enum InstallOutcome {
    /// The running binary was replaced in place. The UI should prompt
    /// the user to relaunch.
    ReplacedRestartRequired,

    /// A platform installer was spawned (msiexec, `open dmg`). The app
    /// should exit so the installer can complete. The UI prompts.
    InstallerLaunched { installer: String },

    /// The user's install method is managed externally (apt, brew, ...).
    /// Banner should have shown the upgrade command; we refused to
    /// touch the install.
    HandedOffToPackageManager { name: String, upgrade_cmd: String },

    /// The install method is unknown; open the release page in the
    /// user's default browser as a fallback.
    OpenedReleasePage { url: String },
}

/// Errors from the install pipeline. Distinguished so the UI can
/// decide whether to show a retry affordance.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("no matching release asset for this platform")]
    NoAssetForPlatform,

    #[error("SHA256SUMS file missing from the release")]
    NoChecksumFile,

    #[error("downloaded {got} bytes, expected {expected}")]
    SizeMismatch { got: u64, expected: u64 },

    #[error("SHA256 checksum mismatch for {asset}")]
    ChecksumMismatch { asset: String },

    #[error("SHA256SUMS does not list an entry for {asset}")]
    ChecksumEntryMissing { asset: String },

    #[error("network error while downloading: {0}")]
    Network(String),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to replace running binary: {0}")]
    Replace(String),

    #[error("failed to launch platform installer: {0}")]
    SpawnInstaller(String),

    #[error("failed to open the release page: {0}")]
    OpenBrowser(String),
}

/// Full install flow. Dispatches to the right strategy based on the
/// detected install method.
///
/// `progress` is an optional callback invoked with
/// `(downloaded_bytes, total_bytes)` as bytes stream in, so the UI
/// can render a progress bar. Called at most every ~256 KB.
pub type ProgressCallback = Box<dyn FnMut(u64, u64) + Send>;

pub async fn install_update(
    method: InstallMethod,
    latest: LatestRelease,
    progress: Option<ProgressCallback>,
) -> Result<InstallOutcome, UpdateError> {
    match method {
        // Managed externally — never download.
        InstallMethod::PackageManager { name, upgrade_cmd } => {
            Ok(InstallOutcome::HandedOffToPackageManager { name, upgrade_cmd })
        }

        // Unknown install → open the release page instead of guessing.
        InstallMethod::Unknown | InstallMethod::SourceBuild => {
            open_in_browser(&latest.html_url)?;
            Ok(InstallOutcome::OpenedReleasePage {
                url: latest.html_url,
            })
        }

        // AppImage on Linux or portable zip on Windows: atomic swap.
        InstallMethod::Portable => {
            let asset = pick_portable_asset(&latest.assets)?.clone();
            let downloaded = download_and_verify(&asset, &latest.assets, progress).await?;
            replace_running_binary(&downloaded).await?;
            Ok(InstallOutcome::ReplacedRestartRequired)
        }

        // Windows MSI: download + invoke msiexec with UAC.
        InstallMethod::MsiInstaller => {
            let asset = pick_msi_asset(&latest.assets)?.clone();
            let downloaded = download_and_verify(&asset, &latest.assets, progress).await?;
            launch_msi(&downloaded)?;
            Ok(InstallOutcome::InstallerLaunched {
                installer: asset.name,
            })
        }

        // macOS .dmg: download + open so the user drags into /Applications.
        InstallMethod::MacOsApp => {
            let asset = pick_dmg_asset(&latest.assets)?.clone();
            let downloaded = download_and_verify(&asset, &latest.assets, progress).await?;
            open_dmg(&downloaded)?;
            Ok(InstallOutcome::InstallerLaunched {
                installer: asset.name,
            })
        }
    }
}

// --- Asset selection --------------------------------------------------

fn pick_portable_asset(assets: &[ReleaseAsset]) -> Result<&ReleaseAsset, UpdateError> {
    #[cfg(target_os = "linux")]
    let patterns: &[&str] = &[".AppImage"];
    #[cfg(target_os = "windows")]
    let patterns: &[&str] = &["pc-windows-msvc.zip", "windows-x64.zip"];
    #[cfg(target_os = "macos")]
    let patterns: &[&str] = &["apple-darwin.tar.gz"];

    assets
        .iter()
        .find(|a| patterns.iter().any(|p| a.name.contains(p)))
        .ok_or(UpdateError::NoAssetForPlatform)
}

fn pick_msi_asset(assets: &[ReleaseAsset]) -> Result<&ReleaseAsset, UpdateError> {
    assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with(".msi"))
        .ok_or(UpdateError::NoAssetForPlatform)
}

fn pick_dmg_asset(assets: &[ReleaseAsset]) -> Result<&ReleaseAsset, UpdateError> {
    assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with(".dmg"))
        .ok_or(UpdateError::NoAssetForPlatform)
}

// --- Download + verify -------------------------------------------------

/// Download `asset` to a temp file and verify its SHA256 matches the
/// entry in the release's `SHA256SUMS` asset. Returns the path to the
/// verified temp file; caller is responsible for moving it into place.
async fn download_and_verify(
    asset: &ReleaseAsset,
    all_assets: &[ReleaseAsset],
    mut progress: Option<ProgressCallback>,
) -> Result<PathBuf, UpdateError> {
    let expected_hash = fetch_expected_hash(all_assets, &asset.name).await?;

    let temp_path = std::env::temp_dir().join(format!("smriti-update-{}", asset.name));
    let client = build_client()?;

    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let total = response.content_length().unwrap_or(asset.size_bytes);
    let mut file = tokio::fs::File::create(&temp_path).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_progress_report: u64 = 0;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| UpdateError::Network(e.to_string()))?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if let Some(cb) = progress.as_mut() {
            // Report at most once per ~256 KB to keep the UI event
            // stream from drowning in ticks.
            if downloaded - last_progress_report > 256 * 1024 || downloaded == total {
                cb(downloaded, total);
                last_progress_report = downloaded;
            }
        }
    }
    file.flush().await?;
    drop(file);

    if asset.size_bytes != 0 && downloaded != asset.size_bytes {
        return Err(UpdateError::SizeMismatch {
            got: downloaded,
            expected: asset.size_bytes,
        });
    }

    let actual_hash = format!("{:x}", hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(&expected_hash) {
        // Clean up the tainted file so we don't accidentally install
        // it on a retry.
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(UpdateError::ChecksumMismatch {
            asset: asset.name.clone(),
        });
    }

    Ok(temp_path)
}

/// Download and parse the `SHA256SUMS` file, returning the expected
/// hash for `asset_name`.
async fn fetch_expected_hash(
    assets: &[ReleaseAsset],
    asset_name: &str,
) -> Result<String, UpdateError> {
    let sums_asset = assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("SHA256SUMS"))
        .ok_or(UpdateError::NoChecksumFile)?;

    let client = build_client()?;
    let body = client
        .get(&sums_asset.browser_download_url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?
        .text()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    parse_sha256sums(&body, asset_name).ok_or_else(|| UpdateError::ChecksumEntryMissing {
        asset: asset_name.to_string(),
    })
}

/// Parse a single entry out of a standard `sha256sum` file. Each line
/// looks like: `<64-hex-chars>  <filename>` or `<64-hex-chars> *<filename>`.
pub(crate) fn parse_sha256sums(body: &str, want: &str) -> Option<String> {
    for line in body.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        let name = name.trim_start_matches('*');
        if name == want {
            return Some(hash.to_lowercase());
        }
    }
    None
}

fn build_client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(format!("smriti/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

// --- Binary swap / installer spawn ------------------------------------

async fn replace_running_binary(new_binary: &Path) -> Result<(), UpdateError> {
    let current = std::env::current_exe().map_err(UpdateError::Io)?;
    let new_binary = new_binary.to_path_buf();
    let current_clone = current.clone();

    tokio::task::spawn_blocking(move || -> Result<(), UpdateError> {
        // On Linux we also need the executable bit set on the
        // downloaded AppImage before the move.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&new_binary)?.permissions();
            perms.set_mode(perms.mode() | 0o755);
            std::fs::set_permissions(&new_binary, perms)?;
        }

        self_update::Move::from_source(&new_binary)
            .to_dest(&current_clone)
            .map_err(|e| UpdateError::Replace(e.to_string()))
    })
    .await
    .map_err(|e| UpdateError::Replace(format!("spawn_blocking panicked: {}", e)))??;

    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_msi(msi: &Path) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("msiexec")
        .arg("/i")
        .arg(msi)
        .arg("/passive")
        .arg("/norestart")
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP so our exit
        // doesn't take msiexec down with us.
        .creation_flags(0x0000_0008 | 0x0000_0200)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::SpawnInstaller(e.to_string()))
}

#[cfg(not(target_os = "windows"))]
fn launch_msi(_msi: &Path) -> Result<(), UpdateError> {
    Err(UpdateError::SpawnInstaller(
        "MSI launch is only supported on Windows".into(),
    ))
}

#[cfg(target_os = "macos")]
fn open_dmg(dmg: &Path) -> Result<(), UpdateError> {
    std::process::Command::new("open")
        .arg("-g")
        .arg(dmg)
        .spawn()
        .map(|_| ())
        .map_err(|e| UpdateError::SpawnInstaller(e.to_string()))
}

#[cfg(not(target_os = "macos"))]
fn open_dmg(_dmg: &Path) -> Result<(), UpdateError> {
    Err(UpdateError::SpawnInstaller(
        "DMG launch is only supported on macOS".into(),
    ))
}

fn open_in_browser(url: &str) -> Result<(), UpdateError> {
    open::that(url).map_err(|e| UpdateError::OpenBrowser(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256sums_parse_plain_format() {
        let body = "\
abc123  Smriti-x86_64.AppImage
def456  Smriti-Setup-x64.msi
";
        assert_eq!(
            parse_sha256sums(body, "Smriti-x86_64.AppImage"),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_sha256sums(body, "Smriti-Setup-x64.msi"),
            Some("def456".to_string())
        );
    }

    #[test]
    fn sha256sums_parse_binary_star_format() {
        // Some sha256sum implementations use `*` to mark binary mode.
        let body = "abc123 *Smriti-x86_64.AppImage\n";
        assert_eq!(
            parse_sha256sums(body, "Smriti-x86_64.AppImage"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn sha256sums_returns_none_when_missing() {
        let body = "abc123  other-file\n";
        assert!(parse_sha256sums(body, "Smriti-x86_64.AppImage").is_none());
    }

    #[test]
    fn sha256sums_skips_blank_lines() {
        let body = "\n\nabc123  Smriti-x86_64.AppImage\n\n";
        assert_eq!(
            parse_sha256sums(body, "Smriti-x86_64.AppImage"),
            Some("abc123".to_string())
        );
    }
}
