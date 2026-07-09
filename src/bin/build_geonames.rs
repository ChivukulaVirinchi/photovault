//! Build helper for GeoNames SQLite database.
//!
//! Delegates to `smriti::db::geonames::build_geonames_db` so the
//! schema and ingestion logic live in exactly one place — the
//! library. Earlier this bin inlined its own CREATE TABLE which
//! drifted out of sync with the library version (missing the
//! `feature_code` column added for admin-seat-aware geocoding) and
//! produced a populated-but-uncoded cities table.
//!
//! Run from the workspace root via `cargo run --release --bin
//! build_geonames`. `data/cities1000.txt` and
//! `data/country_codes.txt` must exist alongside (downloaded by
//! `scripts/setup_assets.sh`).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_root = PathBuf::from(".");
    smriti::db::geonames::build_geonames_db(&project_root)?;
    println!(
        "GeoNames database created at {}",
        smriti::db::geonames::geonames_db_path().display()
    );
    Ok(())
}
