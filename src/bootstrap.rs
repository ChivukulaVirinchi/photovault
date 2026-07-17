//! Runtime bootstrap checks and setup helpers.

use std::io::Write;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use zip::read::ZipArchive;

const MAX_ASSET_PACK_FILES: usize = 1_000;
const MAX_ASSET_PACK_DOWNLOAD_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ASSET_PACK_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[cfg(target_os = "windows")]
pub const SETUP_ASSETS_HINT: &str =
    "powershell -ExecutionPolicy Bypass -File scripts\\setup_assets.ps1";

#[cfg(not(target_os = "windows"))]
pub const SETUP_ASSETS_HINT: &str = "./scripts/setup_assets.sh";

pub const ASSET_PACK_URL_DEFAULT: &str = concat!(
    "https://github.com/ChivukulaVirinchi/photovault/releases/download/v",
    env!("CARGO_PKG_VERSION"),
    "/Smriti-Assets.zip"
);
const ASSET_PACK_LATEST_URL: &str =
    "https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/Smriti-Assets.zip";

#[derive(Debug, Clone, Default)]
pub struct AssetHealth {
    pub missing_face_models: bool,
    pub missing_onnx_runtime: bool,
    pub missing_geonames_db: bool,
}

impl AssetHealth {
    pub fn missing_any(&self) -> bool {
        self.missing_face_models || self.missing_onnx_runtime || self.missing_geonames_db
    }

    pub fn summary(&self) -> String {
        let mut missing = Vec::new();
        if self.missing_face_models {
            missing.push("face models");
        }
        if self.missing_onnx_runtime {
            missing.push("ONNX runtime");
        }
        if self.missing_geonames_db {
            missing.push("GeoNames database");
        }
        if missing.is_empty() {
            "All optional assets are installed.".to_string()
        } else {
            format!("Missing: {}", missing.join(", "))
        }
    }
}

#[cfg(target_os = "windows")]
const ORT_LIB_NAME: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
const ORT_LIB_NAME: &str = "libonnxruntime.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const ORT_LIB_NAME: &str = "libonnxruntime.so";
#[cfg(target_os = "windows")]
const ORT_PLATFORM_DIR: &str = "windows";
#[cfg(target_os = "linux")]
const ORT_PLATFORM_DIR: &str = "linux";
#[cfg(target_os = "macos")]
const ORT_PLATFORM_DIR: &str = "macos";
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
const ORT_PLATFORM_DIR: &str = "";

const MIN_BINARY_ASSET_BYTES: u64 = 1024 * 1024;

fn usable_binary_asset(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() >= MIN_BINARY_ASSET_BYTES)
}

fn runtime_file_name_matches(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        name.starts_with("libonnxruntime") && name.ends_with(".dylib")
    }
    #[cfg(not(target_os = "macos"))]
    {
        name.starts_with(ORT_LIB_NAME)
    }
}

pub fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn default_asset_install_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("smriti")
        .join("assets")
}

/// Pre-rename install dir, kept as a secondary lookup so users who
/// installed assets under the old "photovault" path keep working
/// after the upgrade. Removed in a future release.
fn legacy_asset_install_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("photovault")
        .join("assets")
}

pub fn asset_pack_url() -> String {
    // Read SMRITI_* first, fall back to the legacy PHOTOVAULT_*
    // env name for one release worth of grace.
    std::env::var("SMRITI_ASSET_PACK_URL")
        .or_else(|_| std::env::var("PHOTOVAULT_ASSET_PACK_URL"))
        .unwrap_or_else(|_| ASSET_PACK_URL_DEFAULT.to_string())
}

fn find_runtime_in_dir(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join(ORT_LIB_NAME);
    if usable_binary_asset(&direct) {
        return Some(direct);
    }

    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if runtime_file_name_matches(name) && usable_binary_asset(&path) {
                return Some(path);
            }
        }
    }

    None
}

pub fn onnx_runtime_path() -> Option<PathBuf> {
    for root in candidate_asset_roots() {
        if let Some(path) = onnx_runtime_path_in_root(&root) {
            return Some(path);
        }
    }
    None
}

fn onnx_runtime_path_in_root(root: &Path) -> Option<PathBuf> {
    let candidate = root.join("libs").join("onnxruntime");
    if let Some(path) = find_runtime_in_dir(&candidate) {
        return Some(path);
    }
    if !ORT_PLATFORM_DIR.is_empty() {
        return find_runtime_in_dir(&candidate.join(ORT_PLATFORM_DIR));
    }
    None
}

pub fn onnx_runtime_install_path() -> PathBuf {
    let base = default_asset_install_dir().join("libs").join("onnxruntime");
    if ORT_PLATFORM_DIR.is_empty() {
        base.join(ORT_LIB_NAME)
    } else {
        base.join(ORT_PLATFORM_DIR).join(ORT_LIB_NAME)
    }
}

pub fn onnx_runtime_exists() -> bool {
    onnx_runtime_path().is_some()
}

pub fn geonames_db_exists() -> bool {
    crate::db::geonames::geonames_db_exists()
}

pub fn asset_health() -> AssetHealth {
    AssetHealth {
        missing_face_models: !has_face_models(),
        missing_onnx_runtime: !onnx_runtime_exists(),
        missing_geonames_db: !geonames_db_exists(),
    }
}

fn asset_health_in_root(root: &Path) -> AssetHealth {
    let model_dir = root.join("models");
    let detector = model_dir.join("scrfd_10g_bnkps.onnx");
    let embedder = model_dir.join(crate::config::AppConfig::load().face_embedder_model);
    AssetHealth {
        missing_face_models: !usable_binary_asset(&detector) || !usable_binary_asset(&embedder),
        missing_onnx_runtime: onnx_runtime_path_in_root(root).is_none(),
        missing_geonames_db: !crate::db::geonames::geonames_db_is_current(
            &root.join("data").join("geonames.db"),
        ),
    }
}

fn candidate_asset_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    roots.push(default_asset_install_dir());
    // Legacy fallback so users with assets installed under the old
    // PhotoVault path keep working through the rename.
    roots.push(legacy_asset_install_dir());

    if let Ok(from_env) = std::env::var("SMRITI_ASSET_DIR") {
        roots.push(PathBuf::from(from_env));
    }
    if let Ok(from_env) = std::env::var("PHOTOVAULT_ASSET_DIR") {
        roots.push(PathBuf::from(from_env));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // Portable/AppImage layout
            roots.push(exe_dir.to_path_buf());
            // Debian install layout: /usr/bin/<name> -> /usr/lib/<name>
            roots.push(exe_dir.join("..").join("lib").join("smriti"));
            // Legacy install path under the old name.
            roots.push(exe_dir.join("..").join("lib").join("photovault"));
            // `cargo tauri dev` runs the binary from target/debug/.
            // Walk up two levels to land on the workspace root, where
            // the dev tree's `libs/` and `models/` actually live.
            roots.push(exe_dir.join("..").join(".."));
        }
    }
    // `cargo tauri dev` sets CWD to src-tauri/. Walk up so the
    // workspace root's libs/ + models/ are reachable.
    if let Some(parent) = std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(|p| p.parent())
    {
        roots.push(parent.to_path_buf());
    }

    roots.push(project_root());
    roots.push(PathBuf::from("/usr/lib/smriti"));
    roots.push(PathBuf::from("/usr/lib/photovault")); // legacy

    roots
}

pub fn asset_roots() -> Vec<PathBuf> {
    candidate_asset_roots()
}

pub fn model_dir() -> PathBuf {
    for root in candidate_asset_roots() {
        let candidate = root.join("models");
        if candidate.exists() {
            return candidate;
        }
    }

    project_root().join("models")
}

pub fn detector_model_path() -> PathBuf {
    model_dir().join("scrfd_10g_bnkps.onnx")
}

pub fn embedder_model_path() -> PathBuf {
    model_dir().join(crate::config::AppConfig::load().face_embedder_model)
}

pub fn has_face_models() -> bool {
    usable_binary_asset(&detector_model_path()) && usable_binary_asset(&embedder_model_path())
}

pub fn ensure_geonames_db() {
    use crate::db::geonames::{build_geonames_db, geonames_db_exists, geonames_db_path};

    if geonames_db_exists() {
        return;
    }

    let existing_path = geonames_db_path();
    let root = existing_path
        .parent()
        .and_then(Path::parent)
        .filter(|root| {
            root.join("data").join("cities1000.txt").is_file()
                && root.join("data").join("country_codes.txt").is_file()
        })
        .map(Path::to_path_buf)
        .or_else(|| {
            candidate_asset_roots().into_iter().find(|root| {
                root.join("data").join("cities1000.txt").is_file()
                    && root.join("data").join("country_codes.txt").is_file()
            })
        })
        .unwrap_or_else(project_root);
    let data_dir = root.join("data");
    let cities = data_dir.join("cities1000.txt");
    let countries = data_dir.join("country_codes.txt");
    let db_path = data_dir.join("geonames.db");

    if !cities.exists() || !countries.exists() {
        tracing::warn!(
            "GeoNames source files missing (expected {} and {}). Geocoding disabled until provided. Run {}",
            cities.display(),
            countries.display(),
            SETUP_ASSETS_HINT
        );
        return;
    }

    tracing::info!("GeoNames DB not found; building {}", db_path.display());

    match build_geonames_db(&root) {
        Ok(()) => tracing::info!("GeoNames DB created at {}", db_path.display()),
        Err(e) => tracing::warn!("Failed to auto-build GeoNames DB: {}", e),
    }
}

pub async fn install_asset_pack() -> Result<String, String> {
    let install_root = default_asset_install_dir();
    let install_parent = install_root
        .parent()
        .ok_or_else(|| "Asset install path has no parent directory".to_string())?;
    std::fs::create_dir_all(install_parent).map_err(|e| {
        format!(
            "Failed creating asset parent dir {}: {}",
            install_parent.display(),
            e
        )
    })?;

    // Honour SMRITI_* env vars first, fall back to legacy PHOTOVAULT_*
    // for one release worth of grace.
    let local_path = std::env::var("SMRITI_ASSET_PACK_PATH")
        .or_else(|_| std::env::var("PHOTOVAULT_ASSET_PACK_PATH"))
        .ok();
    let (archive_path, remove_archive_after) = if let Some(local_path) = local_path {
        let path = PathBuf::from(&local_path);
        let metadata = std::fs::metadata(&path).map_err(|e| {
            format!(
                "Failed to read SMRITI_ASSET_PACK_PATH {}: {}",
                local_path, e
            )
        })?;
        if !metadata.is_file() || metadata.len() > MAX_ASSET_PACK_DOWNLOAD_BYTES {
            return Err("Local asset pack is not a file or exceeds the 1 GiB limit".to_string());
        }
        (path, false)
    } else {
        let primary_url = asset_pack_url();
        let fallback_url = std::env::var("SMRITI_ASSET_PACK_FALLBACK_URL")
            .or_else(|_| std::env::var("PHOTOVAULT_ASSET_PACK_FALLBACK_URL"))
            .unwrap_or_else(|_| ASSET_PACK_LATEST_URL.to_string());

        let download_path = install_parent.join(format!(
            ".smriti-assets-download-{}.zip",
            std::process::id()
        ));
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(30 * 60))
            .build()
            .map_err(|error| format!("Failed creating asset download client: {error}"))?;
        let mut last_error = None;
        let mut downloaded = false;
        for url in [primary_url.as_str(), fallback_url.as_str()] {
            let response = match client.get(url).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(format!("request error from {}: {}", url, e));
                    continue;
                }
            };

            if !response.status().is_success() {
                last_error = Some(format!("HTTP {} from {}", response.status(), url));
                continue;
            }

            if response
                .content_length()
                .is_some_and(|length| length > MAX_ASSET_PACK_DOWNLOAD_BYTES)
            {
                last_error = Some(format!("asset pack from {} exceeds 1 GiB", url));
                continue;
            }

            let mut output = match std::fs::File::create(&download_path) {
                Ok(file) => file,
                Err(error) => {
                    last_error = Some(format!("cannot create download file: {}", error));
                    break;
                }
            };
            let mut stream = response.bytes_stream();
            let mut written = 0_u64;
            let mut stream_error = None;
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        stream_error = Some(format!("body read error from {}: {}", url, error));
                        break;
                    }
                };
                written = written.saturating_add(chunk.len() as u64);
                if written > MAX_ASSET_PACK_DOWNLOAD_BYTES {
                    stream_error = Some(format!("asset pack from {} exceeds 1 GiB", url));
                    break;
                }
                if let Err(error) = output.write_all(&chunk) {
                    stream_error = Some(format!("download write failed: {}", error));
                    break;
                }
            }
            drop(output);
            if let Some(error) = stream_error {
                last_error = Some(error);
                let _ = std::fs::remove_file(&download_path);
                continue;
            }
            if written == 0 {
                last_error = Some(format!("empty response from {}", url));
                let _ = std::fs::remove_file(&download_path);
                continue;
            }
            downloaded = true;
            break;
        }

        if !downloaded {
            return Err(format!(
                "Asset pack download failed. {}. If you are testing locally before publishing a release, set SMRITI_ASSET_PACK_PATH to a local Smriti-Assets.zip.",
                last_error.unwrap_or_else(|| "No successful download source".to_string())
            ));
        }
        (download_path, true)
    };

    let install_root_for_worker = install_root.clone();
    let install_parent_for_worker = install_parent.to_path_buf();
    tokio::task::spawn_blocking(move || {
        install_downloaded_asset_pack(
            &archive_path,
            remove_archive_after,
            &install_root_for_worker,
            &install_parent_for_worker,
        )
    })
    .await
    .map_err(|error| format!("Asset installation worker failed: {error}"))??;

    Ok(format!("Assets installed to {}", install_root.display()))
}

fn install_downloaded_asset_pack(
    archive_path: &Path,
    remove_archive_after: bool,
    install_root: &Path,
    install_parent: &Path,
) -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let staging = install_parent.join(format!(
        ".smriti-assets-installing-{}-{nonce}",
        std::process::id()
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|e| format!("Failed clearing stale asset staging directory: {e}"))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("Failed creating asset staging directory: {e}"))?;

    let install_result = (|| {
        extract_asset_archive(archive_path, &staging)?;
        prepare_staged_asset_pack(&staging)?;
        merge_asset_tree(&staging, install_root)
    })();

    let cleanup_result = std::fs::remove_dir_all(&staging);
    if remove_archive_after {
        let _ = std::fs::remove_file(archive_path);
    }
    install_result?;
    if let Err(error) = cleanup_result {
        tracing::warn!(
            "Assets installed, but staging cleanup failed at {}: {}",
            staging.display(),
            error
        );
    }

    Ok(())
}

fn prepare_staged_asset_pack(staging: &Path) -> Result<(), String> {
    // v0.3.1's published pack accidentally contained GeoNames source text
    // but no geonames.db. Repair that pack locally so one-click setup works
    // for existing releases as well as corrected future ones.
    let staged_geonames = staging.join("data").join("geonames.db");
    if !crate::db::geonames::geonames_db_is_current(&staged_geonames) {
        let cities = staging.join("data").join("cities1000.txt");
        let countries = staging.join("data").join("country_codes.txt");
        if cities.is_file() && countries.is_file() {
            crate::db::geonames::build_geonames_db(staging)?;
        }
    }

    // Validate this download itself. Looking across all candidate roots could
    // let an old development asset mask an incomplete release archive.
    let health = asset_health_in_root(staging);
    if health.missing_any() {
        return Err(format!(
            "Downloaded asset pack is incomplete. {}",
            health.summary()
        ));
    }
    Ok(())
}

fn extract_asset_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let reader = std::fs::File::open(archive_path)
        .map_err(|e| format!("Failed opening downloaded asset zip: {}", e))?;
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid asset zip: {}", e))?;
    if archive.len() > MAX_ASSET_PACK_FILES {
        return Err(format!(
            "Asset zip contains too many entries ({}; maximum {})",
            archive.len(),
            MAX_ASSET_PACK_FILES
        ));
    }

    let mut expanded_bytes = 0_u64;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed reading zip entry {}: {}", i, e))?;
        expanded_bytes = expanded_bytes
            .checked_add(file.size())
            .ok_or_else(|| "Asset zip expanded size overflow".to_string())?;
        if expanded_bytes > MAX_ASSET_PACK_EXPANDED_BYTES {
            return Err("Asset zip expands beyond the 2 GiB safety limit".to_string());
        }
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe path in zip entry: {}", file.name()))?;

        let out_path = destination.join(enclosed);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed creating directory {}: {}", out_path.display(), e))?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed creating parent directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        let mut out = std::fs::File::create(&out_path)
            .map_err(|e| format!("Failed creating {}: {}", out_path.display(), e))?;
        std::io::copy(&mut file, &mut out)
            .map_err(|e| format!("Failed writing {}: {}", out_path.display(), e))?;
    }
    Ok(())
}

fn merge_asset_tree(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|e| format!("Failed creating {}: {}", destination.display(), e))?;
    for entry in std::fs::read_dir(source)
        .map_err(|e| format!("Failed reading {}: {}", source.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed reading asset entry: {}", e))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|e| format!("Failed reading asset file type: {}", e))?
            .is_dir()
        {
            merge_asset_tree(&source_path, &destination_path)?;
        } else {
            let temp_path = destination_path.with_extension("smriti-installing");
            std::fs::copy(&source_path, &temp_path).map_err(|e| {
                format!(
                    "Failed staging asset {} to {}: {}",
                    source_path.display(),
                    temp_path.display(),
                    e
                )
            })?;
            if destination_path.exists() {
                std::fs::remove_file(&destination_path).map_err(|e| {
                    format!(
                        "Failed replacing asset {}: {}",
                        destination_path.display(),
                        e
                    )
                })?;
            }
            std::fs::rename(&temp_path, &destination_path).map_err(|e| {
                format!(
                    "Failed installing asset {}: {}",
                    destination_path.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_runtime_from_asset_pack_platform_subdir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = if ORT_PLATFORM_DIR.is_empty() {
            temp.path().join("libs").join("onnxruntime")
        } else {
            temp.path()
                .join("libs")
                .join("onnxruntime")
                .join(ORT_PLATFORM_DIR)
        };
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let runtime = runtime_dir.join(ORT_LIB_NAME);
        std::fs::write(&runtime, vec![0_u8; MIN_BINARY_ASSET_BYTES as usize])
            .expect("runtime marker");

        assert_eq!(onnx_runtime_path_in_root(temp.path()), Some(runtime));
    }

    #[test]
    fn runtime_install_path_matches_asset_pack_layout() {
        let path = onnx_runtime_install_path();
        let suffix = if ORT_PLATFORM_DIR.is_empty() {
            PathBuf::from("libs").join("onnxruntime").join(ORT_LIB_NAME)
        } else {
            PathBuf::from("libs")
                .join("onnxruntime")
                .join(ORT_PLATFORM_DIR)
                .join(ORT_LIB_NAME)
        };

        assert!(path.ends_with(suffix));
    }

    #[test]
    fn default_asset_pack_matches_running_app_version() {
        assert!(ASSET_PACK_URL_DEFAULT.contains(&format!(
            "/v{}/Smriti-Assets.zip",
            env!("CARGO_PKG_VERSION")
        )));
    }

    #[test]
    fn rejects_truncated_runtime_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("libs").join("onnxruntime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        std::fs::write(runtime_dir.join(ORT_LIB_NAME), b"download error page")
            .expect("runtime marker");

        assert_eq!(onnx_runtime_path_in_root(temp.path()), None);
    }

    #[test]
    fn repairs_published_pack_with_geonames_sources_but_no_database() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let data = root.join("data");
        let models = root.join("models");
        let runtime_dir = if ORT_PLATFORM_DIR.is_empty() {
            root.join("libs").join("onnxruntime")
        } else {
            root.join("libs").join("onnxruntime").join(ORT_PLATFORM_DIR)
        };
        std::fs::create_dir_all(&data).expect("data");
        std::fs::create_dir_all(&models).expect("models");
        std::fs::create_dir_all(&runtime_dir).expect("runtime");
        std::fs::write(data.join("country_codes.txt"), "ZZ\tTest Country\n").expect("countries");
        let mut cities = String::new();
        for id in 1..=1_001 {
            cities.push_str(&format!(
                "{id}\tCity {id}\tCity {id}\t\t1.0\t2.0\tP\tPPL\tZZ\t\t\t\t\t\t{id}\t\t\tUTC\t2026-01-01\n"
            ));
        }
        std::fs::write(data.join("cities1000.txt"), cities).expect("cities");
        let marker = vec![0_u8; MIN_BINARY_ASSET_BYTES as usize];
        std::fs::write(models.join("scrfd_10g_bnkps.onnx"), &marker).expect("detector");
        std::fs::write(models.join("adaface_ir101_webface12m.onnx"), &marker).expect("embedder");
        std::fs::write(runtime_dir.join(ORT_LIB_NAME), marker).expect("runtime");

        prepare_staged_asset_pack(root).expect("repair and validate pack");

        assert!(crate::db::geonames::geonames_db_is_current(
            &data.join("geonames.db")
        ));
    }
}
