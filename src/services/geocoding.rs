//! Offline reverse geocoding service.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, Result as SqliteResult};

/// A geocoding result.
#[derive(Debug, Clone)]
pub struct GeocodingResult {
    pub city: String,
    pub country: String,
}

/// Offline geocoding service using GeoNames data.
pub struct GeocodingService {
    conn: Connection,
}

impl GeocodingService {
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.execute_batch(
            r#"
            PRAGMA query_only = ON;
            PRAGMA cache_size = -10000;
            PRAGMA mmap_size = 268435456;
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Hard cutoff (km) for the city match. Without this, sparse
    /// regions in cities1000 can route a photo's coordinates to a city
    /// hundreds of km away (e.g., a small admin region in Tibet
    /// becoming the "nearest" match for photos taken in Goa). 100 km is
    /// the empirical sweet spot — covers photos shot from rural areas
    /// near a city while still rejecting cross-country mis-attributions.
    const MAX_CITY_DISTANCE_KM: f64 = 100.0;

    pub fn reverse_geocode(&self, lat: f64, lon: f64) -> Option<GeocodingResult> {
        if !Self::is_valid_coordinate(lat, lon) {
            return None;
        }

        // ±1° (~110 km) covers the cutoff at most latitudes; expand
        // once to ±2° to catch edge cases where the photo sits exactly
        // between cells. Any further-out match exceeds the haversine
        // cutoff and gets filtered out anyway.
        self.search_bounding_box(lat, lon, 1.0)
            .or_else(|| self.search_bounding_box(lat, lon, 2.0))
    }

    /// Priority bucket for a GeoNames `feature_code`. Lower number =
    /// better match. We strongly prefer admin-seat codes (state/
    /// district / sub-district HQs) over plain "PPL" entries, because
    /// upstream GeoNames data sometimes tags suburbs / neighbourhoods
    /// with metro-wide populations — e.g. Rasapudipalem (PPL) is a
    /// neighbourhood of Visakhapatnam (PPLA2) but carries population
    /// 1.7 M in `cities1000.txt`. Ranking by feature_code first picks
    /// the user-recognisable name regardless.
    fn feature_priority(code: &str) -> u8 {
        match code {
            "PPLC" => 0,  // country capital
            "PPLA" => 1,  // state capital
            "PPLA2" => 2, // district seat
            "PPLA3" => 3,
            "PPLA4" => 4,
            "PPLG" => 5, // seat of government
            "PPL" => 6,  // generic populated place
            _ => 7,      // PPLX / PPLL / PPLS / etc.
        }
    }

    fn search_bounding_box(&self, lat: f64, lon: f64, radius_deg: f64) -> Option<GeocodingResult> {
        let min_lat = (lat - radius_deg).max(-90.0);
        let max_lat = (lat + radius_deg).min(90.0);
        let lon_ranges = longitude_ranges(lon, radius_deg);

        // We DO still apply a population floor, but only as a sanity
        // gate against extremely small entries. Real ranking is by
        // `feature_priority(feature_code)` + distance — see comments
        // there. 100 k stays as the floor for the same reason it was
        // chosen originally: rural photos without a major town nearby
        // should fall through to the "Approx. lat/lng" UI fallback
        // rather than getting tagged with a village no one recognises.
        let mut cities: Vec<(String, String, String, f64, f64, String)> = Vec::new();
        for (min_lon, max_lon) in lon_ranges {
            let mut stmt = self
                .conn
                .prepare(
                    r#"
                SELECT
                    ascii_name,
                    country_name,
                    country_code,
                    latitude,
                    longitude,
                    COALESCE(feature_code, '')
                FROM cities
                WHERE latitude BETWEEN ?1 AND ?2
                  AND longitude BETWEEN ?3 AND ?4
                  AND population >= 100000
                LIMIT 200
                "#,
                )
                .ok()?;

            let mut rows = stmt
                .query_map(params![min_lat, max_lat, min_lon, max_lon], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                })
                .ok()?
                .collect::<rusqlite::Result<Vec<_>>>()
                .ok()?;
            cities.append(&mut rows);
        }

        // Walk every candidate and keep the best one according to
        // (feature_priority asc, distance asc). Anything farther than
        // MAX_CITY_DISTANCE_KM is filtered upfront so it can't win.
        let mut best: Option<(GeocodingResult, u8, f64)> = None;

        for (city_name, country_name, _country_code, city_lat, city_lon, feature_code) in cities {
            let distance = Self::haversine_distance(lat, lon, city_lat, city_lon);
            if distance > Self::MAX_CITY_DISTANCE_KM {
                continue;
            }
            let priority = Self::feature_priority(&feature_code);
            let candidate = GeocodingResult {
                city: city_name,
                country: country_name,
            };
            let take = match &best {
                None => true,
                Some((_, p, d)) => (priority, distance) < (*p, *d),
            };
            if take {
                best = Some((candidate, priority, distance));
            }
        }

        best.map(|(r, _, _)| r)
    }

    fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        const EARTH_RADIUS_KM: f64 = 6371.0;

        let lat1_rad = lat1.to_radians();
        let lat2_rad = lat2.to_radians();
        let delta_lat = (lat2 - lat1).to_radians();
        let delta_lon = (lon2 - lon1).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();
        EARTH_RADIUS_KM * c
    }

    fn is_valid_coordinate(lat: f64, lon: f64) -> bool {
        if !((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)) {
            return false;
        }
        !((lat.abs() < 0.001) && (lon.abs() < 0.001))
    }
}

fn longitude_ranges(lon: f64, radius_deg: f64) -> Vec<(f64, f64)> {
    let min_lon = lon - radius_deg;
    let max_lon = lon + radius_deg;
    if min_lon < -180.0 {
        vec![(min_lon + 360.0, 180.0), (-180.0, max_lon)]
    } else if max_lon > 180.0 {
        vec![(min_lon, 180.0), (-180.0, max_lon - 360.0)]
    } else {
        vec![(min_lon, max_lon)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_distance() {
        let distance = GeocodingService::haversine_distance(35.6762, 139.6503, 40.7128, -74.0060);
        assert!((distance - 10850.0).abs() < 200.0);
    }

    #[test]
    fn test_valid_coordinates() {
        assert!(GeocodingService::is_valid_coordinate(35.6762, 139.6503));
        assert!(!GeocodingService::is_valid_coordinate(0.0, 0.0));
        assert!(!GeocodingService::is_valid_coordinate(91.0, 0.0));
    }
}
