//! Workflow test: reverse-geocoding GPS coords → city names.
//!
//! Hermetic: builds a tiny throwaway GeoNames SQLite in a tempdir
//! with five cities (Visakhapatnam PPLA2 + Rasapudipalem PPL +
//! Gajuwaka PPL + Paris PPLC + Tokyo PPLA), then exercises the
//! geocoder against it. No dependency on the production
//! `data/geonames.db` — every test is self-contained and runs in
//! milliseconds.
//!
//! Pins two service behaviours:
//!   - admin-seat feature codes (PPLA/PPLA2/PPLC) beat plain PPL
//!     even when PPL is nearer or has higher population. This was
//!     the recent Visakhapatnam-vs-Rasapudipalem regression.
//!   - coordinates with no city within MAX_CITY_DISTANCE_KM (100 km)
//!     return None — we don't tag rural photos with a city 200 km
//!     away.

use rusqlite::Connection;
use tempfile::TempDir;

use smriti::services::geocoding::GeocodingService;

/// Build a tiny throwaway geonames-shaped SQLite at the returned
/// path. Schema matches what `smriti::db::geonames::build_geonames_db`
/// produces (v2 — includes `feature_code`). Returns the tempdir
/// (held alive by the caller) and the path the geocoder reads from.
fn make_test_geonames() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("smriti-geonames-")
        .tempdir()
        .unwrap();
    let db_path = dir.path().join("geonames.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE cities (
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
        CREATE INDEX idx_cities_coords ON cities(latitude, longitude);
        CREATE INDEX idx_cities_feature ON cities(feature_code);
        "#,
    )
    .unwrap();

    // (name, ascii_name, lat, lng, country_code, country_name,
    //  population, feature_code)
    type CityRow = (
        &'static str,
        &'static str,
        f64,
        f64,
        &'static str,
        &'static str,
        i64,
        &'static str,
    );
    let rows: &[CityRow] = &[
        // Vizag-area trio — the regression scenario. Rasapudipalem
        // has the highest population (bogus upstream tag) and is
        // closest to some coordinates, but Visakhapatnam is PPLA2
        // (district seat) and must win.
        (
            "Visakhapatnam",
            "Visakhapatnam",
            17.6868,
            83.2185,
            "IN",
            "India",
            1_063_178,
            "PPLA2",
        ),
        (
            "Rasapudipalem",
            "Rasapudipalem",
            17.7330,
            83.3162,
            "IN",
            "India",
            1_728_128,
            "PPL",
        ),
        (
            "Gajuwaka", "Gajuwaka", 17.7000, 83.2166, "IN", "India", 258_944, "PPL",
        ),
        // National capitals, comfortably above the population floor.
        (
            "Paris", "Paris", 48.8566, 2.3522, "FR", "France", 2_148_271, "PPLC",
        ),
        (
            "Tokyo", "Tokyo", 35.6762, 139.6503, "JP", "Japan", 8_336_599, "PPLA",
        ),
        (
            "Dateline City",
            "Dateline City",
            0.1,
            -179.7,
            "FJ",
            "Fiji",
            200_000,
            "PPL",
        ),
    ];

    let mut stmt = conn
        .prepare(
            r#"
            INSERT INTO cities
                (name, ascii_name, latitude, longitude, country_code,
                 country_name, population, feature_code, timezone)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
            "#,
        )
        .unwrap();
    for (name, ascii, lat, lng, cc, country, pop, fcode) in rows {
        stmt.execute(rusqlite::params![
            name, ascii, lat, lng, cc, country, pop, fcode
        ])
        .unwrap();
    }

    (dir, db_path)
}

#[test]
fn vizag_coords_prefer_visakhapatnam_over_neighbourhoods() {
    let (_dir, db) = make_test_geonames();
    let svc = GeocodingService::new(&db).unwrap();

    // Coords at Visakhapatnam city centre. All three vizag-area
    // cities are within 5 km but only Visakhapatnam is PPLA2 —
    // feature-code ranking must promote it.
    let r = svc
        .reverse_geocode(17.6868, 83.2185)
        .expect("vizag should geocode");
    assert_eq!(
        r.city, "Visakhapatnam",
        "PPLA2 beats PPL even when PPL is closer"
    );
    assert_eq!(r.country, "India");
}

#[test]
fn rasapudipalem_coords_still_pick_visakhapatnam_via_admin_seat() {
    // Coords AT Rasapudipalem (17.733, 83.316). Naively-nearest
    // would pick Rasapudipalem (distance 0). But Visakhapatnam is
    // PPLA2 (priority 2) vs PPL (priority 6); the priority tiebreak
    // wins regardless of distance.
    let (_dir, db) = make_test_geonames();
    let svc = GeocodingService::new(&db).unwrap();
    let r = svc
        .reverse_geocode(17.7330, 83.3162)
        .expect("should geocode");
    assert_eq!(
        r.city, "Visakhapatnam",
        "admin-seat priority takes precedence over distance"
    );
}

#[test]
fn paris_coords_resolve_to_paris() {
    let (_dir, db) = make_test_geonames();
    let svc = GeocodingService::new(&db).unwrap();
    let r = svc
        .reverse_geocode(48.8566, 2.3522)
        .expect("paris should geocode");
    assert_eq!(r.city, "Paris");
    assert_eq!(r.country, "France");
}

#[test]
fn coords_with_no_city_in_range_return_none() {
    let (_dir, db) = make_test_geonames();
    let svc = GeocodingService::new(&db).unwrap();
    // Middle of the Pacific Ocean — far from every city in our
    // synthetic DB.
    assert!(
        svc.reverse_geocode(0.0, -150.0).is_none(),
        "open ocean should not resolve to anything"
    );
}

#[test]
fn dateline_crossing_coords_can_match_nearby_city() {
    let (_dir, db) = make_test_geonames();
    let svc = GeocodingService::new(&db).unwrap();
    let r = svc
        .reverse_geocode(0.1, 179.8)
        .expect("city just across the dateline should geocode");
    assert_eq!(r.city, "Dateline City");
}

#[test]
fn invalid_coords_return_none_without_querying_the_db() {
    let (_dir, db) = make_test_geonames();
    let svc = GeocodingService::new(&db).unwrap();
    assert!(svc.reverse_geocode(95.0, 0.0).is_none(), "lat > 90");
    assert!(svc.reverse_geocode(0.0, -200.0).is_none(), "lng < -180");
    // Explicit guard against EXIF rows that came in as (0, 0)
    // sentinel coords — without it they'd resolve to "Atlantic
    // Ocean".
    assert!(svc.reverse_geocode(0.0, 0.0).is_none(), "(0,0) sentinel");
}
