//! GeoNames database access helpers.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;

use rusqlite::Connection;

/// Resolve GeoNames DB path.
///
/// Searches the same candidate roots as the rest of bootstrap (asset
/// install dir, $PHOTOVAULT_ASSET_DIR, executable dir, project root,
/// /usr/lib/photovault) plus the CWD-relative `data/`. The previous
/// implementation skipped the project-root candidate, so `cargo tauri
/// dev` runs from a sub-directory couldn't find the dev-tree DB.
pub fn geonames_db_path() -> PathBuf {
    // 1. Walk every bootstrap-known asset root.
    for root in candidate_geonames_roots() {
        let p = root.join("data").join("geonames.db");
        if p.exists() {
            return p;
        }
    }
    // 2. Last-resort: CWD-relative literal "data/geonames.db".
    let cwd = PathBuf::from("data").join("geonames.db");
    if cwd.exists() {
        return cwd;
    }
    // 3. Fallback path that may not exist; caller checks.
    crate::bootstrap::default_asset_install_dir()
        .join("data")
        .join("geonames.db")
}

/// Candidate root directories under which `data/geonames.db` may live.
/// Mirrors `bootstrap::candidate_asset_roots` but inlined here to avoid
/// pulling that helper into a public surface.
fn candidate_geonames_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    roots.push(crate::bootstrap::default_asset_install_dir());
    if let Ok(from_env) = std::env::var("PHOTOVAULT_ASSET_DIR") {
        roots.push(PathBuf::from(from_env));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            // Debian install layout
            roots.push(dir.join("..").join("lib").join("photovault"));
            // target/debug/<binary> → walk up to workspace root
            roots.push(dir.join("..").join(".."));
            roots.push(dir.join("..").join("..").join(".."));
        }
    }
    let cwd = crate::bootstrap::project_root();
    // `cargo tauri dev` typically sets CWD to src-tauri/. Walk up so the
    // dev tree's data/ directory is reachable.
    if let Some(parent) = cwd.parent() {
        roots.push(parent.to_path_buf());
        if let Some(grand) = parent.parent() {
            roots.push(grand.to_path_buf());
        }
    }
    roots.push(cwd);
    roots.push(PathBuf::from("/usr/lib/photovault"));
    roots
}

/// Check if bundled GeoNames DB exists.
pub fn geonames_db_exists() -> bool {
    geonames_db_path().exists()
}

pub fn build_geonames_db(project_root: &Path) -> Result<(), String> {
    let data_dir = project_root.join("data");
    let countries_path = data_dir.join("country_codes.txt");
    let cities_path = data_dir.join("cities1000.txt");
    let db_path = geonames_db_path();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create db dir: {}", e))?;
    }

    let conn = Connection::open(&db_path).map_err(|e| format!("Failed to open DB: {}", e))?;

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS cities (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            ascii_name TEXT NOT NULL,
            latitude REAL NOT NULL,
            longitude REAL NOT NULL,
            country_code TEXT NOT NULL,
            country_name TEXT NOT NULL,
            population INTEGER,
            timezone TEXT
        );

        CREATE TABLE IF NOT EXISTS countries (
            code TEXT PRIMARY KEY,
            name TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_cities_coords ON cities(latitude, longitude);
        "#,
    )
    .map_err(|e| format!("Failed creating schema: {}", e))?;

    let countries = std::fs::read_to_string(&countries_path)
        .map_err(|e| format!("Failed reading {}: {}", countries_path.display(), e))?;
    for line in countries.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            conn.execute(
                "INSERT OR IGNORE INTO countries (code, name) VALUES (?1, ?2)",
                [parts[0], parts[1]],
            )
            .map_err(|e| format!("Failed inserting country: {}", e))?;
        }
    }

    let file = File::open(&cities_path)
        .map_err(|e| format!("Failed opening {}: {}", cities_path.display(), e))?;
    let reader = BufReader::new(file);
    let mut stmt = conn
        .prepare(
            r#"
            INSERT OR IGNORE INTO cities
                (id, name, ascii_name, latitude, longitude, country_code, country_name, population, timezone)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, (SELECT name FROM countries WHERE code = ?6), ?7, ?8)
            "#,
        )
        .map_err(|e| format!("Failed preparing statement: {}", e))?;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed reading cities line: {}", e))?;
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 18 {
            let id: i64 = parts[0].parse().unwrap_or(0);
            let name = parts[1];
            let ascii_name = parts[2];
            let lat: f64 = parts[4].parse().unwrap_or(0.0);
            let lon: f64 = parts[5].parse().unwrap_or(0.0);
            let country_code = parts[8];
            let population: i64 = parts[14].parse().unwrap_or(0);
            let timezone = parts[17];

            stmt.execute(rusqlite::params![
                id,
                name,
                ascii_name,
                lat,
                lon,
                country_code,
                population,
                timezone
            ])
            .map_err(|e| format!("Failed inserting city row: {}", e))?;
        }
    }

    Ok(())
}
