//! Runtime bootstrap checks and setup helpers.

use std::path::PathBuf;

#[cfg(target_os = "windows")]
pub const SETUP_ASSETS_HINT: &str =
    "powershell -ExecutionPolicy Bypass -File scripts\\setup_assets.ps1";

#[cfg(not(target_os = "windows"))]
pub const SETUP_ASSETS_HINT: &str = "./scripts/setup_assets.sh";

pub fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn model_dir() -> PathBuf {
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
