//! GeoNames database access helpers.

use std::path::PathBuf;

/// Resolve GeoNames DB path.
pub fn geonames_db_path() -> PathBuf {
    PathBuf::from("data").join("geonames.db")
}

/// Check if bundled GeoNames DB exists.
pub fn geonames_db_exists() -> bool {
    geonames_db_path().exists()
}
