//! GeoNames database access helpers.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

/// Resolve GeoNames DB path.
///
/// Searches the same candidate roots as the rest of bootstrap (asset
/// install dir, $SMRITI_ASSET_DIR / legacy $PHOTOVAULT_ASSET_DIR,
/// executable dir, project root, /usr/lib/smriti, /usr/lib/photovault)
/// plus the CWD-relative `data/`.
pub fn geonames_db_path() -> PathBuf {
    let mut first_existing = None;
    // 1. Walk every bootstrap-known asset root.
    for root in candidate_geonames_roots() {
        let p = root.join("data").join("geonames.db");
        if p.exists() {
            if geonames_db_is_current(&p) {
                return p;
            }
            first_existing.get_or_insert(p);
        }
    }
    // 2. Last-resort: CWD-relative literal "data/geonames.db".
    let cwd = PathBuf::from("data").join("geonames.db");
    if cwd.exists() {
        if geonames_db_is_current(&cwd) {
            return cwd;
        }
        first_existing.get_or_insert(cwd);
    }
    if let Some(existing) = first_existing {
        return existing;
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
    if let Ok(from_env) = std::env::var("SMRITI_ASSET_DIR") {
        roots.push(PathBuf::from(from_env));
    }
    if let Ok(from_env) = std::env::var("PHOTOVAULT_ASSET_DIR") {
        roots.push(PathBuf::from(from_env));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            // Debian install layout (smriti primary, legacy photovault fallback)
            roots.push(dir.join("..").join("lib").join("smriti"));
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
    roots.push(PathBuf::from("/usr/lib/smriti"));
    roots.push(PathBuf::from("/usr/lib/photovault"));
    roots
}

/// Check if bundled GeoNames DB exists.
pub fn geonames_db_exists() -> bool {
    geonames_schema_is_current()
}

/// Schema version stamp. Bump whenever the column layout changes so
/// `geonames_schema_is_current` can detect a stale on-disk DB and
/// trigger a rebuild.
///   v2: added `feature_code` (PPLC / PPLA / PPLA2 / PPL / …). Lets us
///       prefer real cities over GeoNames "PPL" entries that inherit
///       a metro-wide population — e.g. Rasapudipalem (PPL, pop
///       1,728,128 in upstream data) shouldn't outrank Visakhapatnam
///       (PPLA2, pop 1,063,178).
pub const GEONAMES_SCHEMA_VERSION: i64 = 2;

/// Returns `true` when the on-disk geonames.db matches the current
/// schema. False if the file is missing OR built against an older
/// layout. Callers (asset setup, the geocoder) should rebuild when
/// this returns false.
pub fn geonames_schema_is_current() -> bool {
    let path = geonames_db_path();
    geonames_db_is_current(&path)
}

/// Validate a specific GeoNames database without creating or mutating it.
pub fn geonames_db_is_current(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return false;
    };
    // Probe both the v2 column and the data. A zero-byte, interrupted,
    // or schema-only database must not make asset setup report success.
    conn.query_row(
        "SELECT COUNT(*) FROM cities WHERE feature_code IS NOT NULL",
        [],
        |row| row.get::<_, i64>(0),
    )
    .is_ok_and(|count| count > 1_000)
}

pub fn build_geonames_db(project_root: &Path) -> Result<(), String> {
    let data_dir = project_root.join("data");
    let countries_path = data_dir.join("country_codes.txt");
    let cities_path = data_dir.join("cities1000.txt");
    // Building and lookup have different semantics. Always put the output
    // beside the supplied sources; setup scripts and release packaging use
    // this exact contract.
    let db_path = data_dir.join("geonames.db");

    std::fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create db dir: {}", e))?;

    // Build beside the destination and only replace the live DB after a
    // successful commit. Interrupted setup leaves the previous DB intact.
    let temp_path = data_dir.join(format!("geonames.db.tmp-{}", std::process::id()));
    if temp_path.exists() {
        std::fs::remove_file(&temp_path)
            .map_err(|e| format!("Failed removing stale GeoNames temp DB: {}", e))?;
    }

    if let Err(error) = build_geonames_db_file(&countries_path, &cities_path, &temp_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }
    if !geonames_db_is_current(&temp_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(
            "Built GeoNames database failed validation; source data may be incomplete".to_string(),
        );
    }

    let backup_path = data_dir.join(format!("geonames.db.backup-{}", std::process::id()));
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)
            .map_err(|e| format!("Failed removing stale GeoNames backup: {}", e))?;
    }
    let had_existing = db_path.exists();
    if had_existing {
        std::fs::rename(&db_path, &backup_path)
            .map_err(|e| format!("Failed backing up old GeoNames DB: {}", e))?;
    }
    if let Err(error) = std::fs::rename(&temp_path, &db_path) {
        if had_existing {
            let _ = std::fs::rename(&backup_path, &db_path);
        }
        return Err(format!("Failed installing rebuilt GeoNames DB: {}", error));
    }
    if had_existing {
        let _ = std::fs::remove_file(&backup_path);
    }

    Ok(())
}

fn build_geonames_db_file(
    countries_path: &Path,
    cities_path: &Path,
    db_path: &Path,
) -> Result<(), String> {
    let mut conn = Connection::open(db_path).map_err(|e| format!("Failed to open DB: {}", e))?;

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
            feature_code TEXT,
            timezone TEXT
        );

        CREATE TABLE IF NOT EXISTS countries (
            code TEXT PRIMARY KEY,
            name TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_cities_coords ON cities(latitude, longitude);
        CREATE INDEX IF NOT EXISTS idx_cities_feature ON cities(feature_code);
        "#,
    )
    .map_err(|e| format!("Failed creating schema: {}", e))?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed starting transaction: {}", e))?;

    let countries = std::fs::read_to_string(countries_path)
        .map_err(|e| format!("Failed reading {}: {}", countries_path.display(), e))?;
    for line in countries.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            // Windows PowerShell 5's `Set-Content -Encoding UTF8` prefixes
            // the first code with a BOM. Strip it or all Andorran city rows
            // fail their country lookup during the Windows setup path.
            let country_code = parts[0].trim_start_matches('\u{feff}');
            if country_code.starts_with('#') {
                continue;
            }
            // Accept compact `code<TAB>name` input and raw GeoNames
            // countryInfo.txt input produced by older packaging jobs.
            let country_name = if parts.len() >= 5 { parts[4] } else { parts[1] };
            tx.execute(
                "INSERT OR IGNORE INTO countries (code, name) VALUES (?1, ?2)",
                [country_code, country_name],
            )
            .map_err(|e| format!("Failed inserting country: {}", e))?;
        }
    }

    let file = File::open(cities_path)
        .map_err(|e| format!("Failed opening {}: {}", cities_path.display(), e))?;
    let reader = BufReader::new(file);
    let mut stmt = tx
        .prepare(
            r#"
            INSERT OR IGNORE INTO cities
                (id, name, ascii_name, latitude, longitude, country_code, country_name, population, feature_code, timezone)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, COALESCE((SELECT name FROM countries WHERE code = ?6), ?6), ?7, ?8, ?9)
            "#,
        )
        .map_err(|e| format!("Failed preparing statement: {}", e))?;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed reading cities line: {}", e))?;
        let parts: Vec<&str> = line.split('\t').collect();
        // cities1000.txt columns (tab-separated):
        //   0=id 1=name 2=ascii_name 3=alt_names 4=lat 5=lng
        //   6=feature_class 7=feature_code 8=country_code 9=cc2
        //   10..13=admin codes 14=population 15=elevation 16=dem
        //   17=timezone 18=mod_date
        if parts.len() >= 18 {
            let id: i64 = parts[0].parse().unwrap_or(0);
            let name = parts[1];
            let ascii_name = parts[2];
            let lat: f64 = parts[4].parse().unwrap_or(0.0);
            let lon: f64 = parts[5].parse().unwrap_or(0.0);
            let country_code = parts[8];
            let population: i64 = parts[14].parse().unwrap_or(0);
            let feature_code = parts[7];
            let timezone = parts[17];

            stmt.execute(rusqlite::params![
                id,
                name,
                ascii_name,
                lat,
                lon,
                country_code,
                population,
                feature_code,
                timezone
            ])
            .map_err(|e| format!("Failed inserting city row: {}", e))?;
        }
    }

    drop(stmt);
    tx.commit()
        .map_err(|e| format!("Failed committing GeoNames DB: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_writes_beside_sources_and_accepts_raw_country_info() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = temp.path().join("data");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::write(
            data.join("country_codes.txt"),
            "\u{feff}ZZ\tZZZ\t999\tZZ\tTest Country\n",
        )
        .expect("countries");

        let mut cities = String::new();
        for id in 1..=1_001 {
            cities.push_str(&format!(
                "{id}\tCity {id}\tCity {id}\t\t1.0\t2.0\tP\tPPL\tZZ\t\t\t\t\t\t{id}\t\t\tUTC\t2026-01-01\n"
            ));
        }
        std::fs::write(data.join("cities1000.txt"), cities).expect("cities");

        build_geonames_db(temp.path()).expect("build");

        let db = data.join("geonames.db");
        assert!(db.exists());
        assert!(!temp.path().join("geonames.db").exists());
        let conn = Connection::open(db).expect("open built db");
        let country: String = conn
            .query_row("SELECT country_name FROM cities WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("country name");
        assert_eq!(country, "Test Country");
    }
}
