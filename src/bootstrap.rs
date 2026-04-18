//! Runtime bootstrap checks and setup helpers.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use zip::read::ZipArchive;

#[cfg(target_os = "windows")]
pub const SETUP_ASSETS_HINT: &str =
    "powershell -ExecutionPolicy Bypass -File scripts\\setup_assets.ps1";

#[cfg(not(target_os = "windows"))]
pub const SETUP_ASSETS_HINT: &str = "./scripts/setup_assets.sh";

pub const ASSET_PACK_URL_DEFAULT: &str =
    "https://github.com/ChivukulaVirinchi/photovault/releases/latest/download/PhotoVault-Assets.zip";

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
#[cfg(not(target_os = "windows"))]
const ORT_LIB_NAME: &str = "libonnxruntime.so";

pub fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn default_asset_install_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("photovault")
        .join("assets")
}

pub fn asset_pack_url() -> String {
    std::env::var("PHOTOVAULT_ASSET_PACK_URL")
        .unwrap_or_else(|_| ASSET_PACK_URL_DEFAULT.to_string())
}

fn find_runtime_in_dir(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join(ORT_LIB_NAME);
    if direct.exists() {
        return Some(direct);
    }

    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(ORT_LIB_NAME) {
                return Some(path);
            }
        }
    }

    None
}

pub fn onnx_runtime_exists() -> bool {
    for root in candidate_asset_roots() {
        let candidate = root.join("libs").join("onnxruntime");
        if find_runtime_in_dir(&candidate).is_some() {
            return true;
        }
    }
    false
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

fn candidate_asset_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    roots.push(default_asset_install_dir());

    if let Ok(from_env) = std::env::var("PHOTOVAULT_ASSET_DIR") {
        let p = PathBuf::from(from_env);
        roots.push(p);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // Portable/AppImage layout
            roots.push(exe_dir.to_path_buf());
            // Debian install layout: /usr/bin/photovault -> /usr/lib/photovault
            roots.push(exe_dir.join("..").join("lib").join("photovault"));
        }
    }

    roots.push(project_root());
    roots.push(PathBuf::from("/usr/lib/photovault"));

    roots
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
    model_dir().join("glintr100.onnx")
}

pub fn has_face_models() -> bool {
    detector_model_path().exists() && embedder_model_path().exists()
}

pub fn ensure_geonames_db() {
    use crate::db::geonames::{build_geonames_db, geonames_db_exists, geonames_db_path};

    if geonames_db_exists() {
        return;
    }

    let root = project_root();
    let data_dir = root.join("data");
    let cities = data_dir.join("cities1000.txt");
    let countries = data_dir.join("country_codes.txt");
    let db_path = geonames_db_path();

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
    let bytes = if let Ok(local_path) = std::env::var("PHOTOVAULT_ASSET_PACK_PATH") {
        std::fs::read(&local_path).map_err(|e| {
            format!(
                "Failed to read PHOTOVAULT_ASSET_PACK_PATH {}: {}",
                local_path, e
            )
        })?
    } else {
        let url = asset_pack_url();
        let response = reqwest::get(&url)
            .await
            .map_err(|e| format!("Failed downloading asset pack from {}: {}", url, e))?;
        if !response.status().is_success() {
            return Err(format!(
                "Asset pack download failed with HTTP status {} from {}",
                response.status(),
                url
            ));
        }
        response
            .bytes()
            .await
            .map_err(|e| format!("Failed reading asset pack response body: {}", e))?
            .to_vec()
    };

    let install_root = default_asset_install_dir();
    std::fs::create_dir_all(&install_root).map_err(|e| {
        format!(
            "Failed creating asset install dir {}: {}",
            install_root.display(),
            e
        )
    })?;

    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid asset zip: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed reading zip entry {}: {}", i, e))?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe path in zip entry: {}", file.name()))?;

        let out_path = install_root.join(enclosed);
        if file.name().ends_with('/') {
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

    let health = asset_health();
    if health.missing_any() {
        return Err(format!(
            "Asset pack installed but required files are still missing. {}",
            health.summary()
        ));
    }

    Ok(format!("Assets installed to {}", install_root.display()))
}
