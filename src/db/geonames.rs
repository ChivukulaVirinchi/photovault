//! GeoNames database access helpers.

use std::path::{Path, PathBuf};

/// Resolve GeoNames DB path.
pub fn geonames_db_path() -> PathBuf {
    PathBuf::from("data").join("geonames.db")
}

/// Check if bundled GeoNames DB exists.
pub fn geonames_db_exists() -> bool {
    geonames_db_path().exists()
}

/// Validate minimal required schema by existence of the DB file.
pub fn validate_geonames_db(path: &Path) -> bool {
    path.exists() && path.is_file()
}
