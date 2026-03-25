//! Offline reverse geocoding service.

use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};

/// A geocoding result.
#[derive(Debug, Clone)]
pub struct GeocodingResult {
    pub city: String,
    pub country: String,
    pub country_code: String,
    pub distance_km: f64,
}

/// Offline geocoding service using GeoNames data.
pub struct GeocodingService {
    conn: Connection,
}

impl GeocodingService {
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA query_only = ON;
            PRAGMA cache_size = -10000;
            PRAGMA mmap_size = 268435456;
            "#,
        )?;
        Ok(Self { conn })
    }

    pub fn reverse_geocode(&self, lat: f64, lon: f64) -> Option<GeocodingResult> {
        if !Self::is_valid_coordinate(lat, lon) {
            return None;
        }

        self.search_bounding_box(lat, lon, 1.0)
            .or_else(|| self.search_bounding_box(lat, lon, 3.0))
    }

    fn search_bounding_box(&self, lat: f64, lon: f64, radius_deg: f64) -> Option<GeocodingResult> {
        let min_lat = lat - radius_deg;
        let max_lat = lat + radius_deg;
        let min_lon = lon - radius_deg;
        let max_lon = lon + radius_deg;

        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT
                    name,
                    country_name,
                    country_code,
                    latitude,
                    longitude
                FROM cities
                WHERE latitude BETWEEN ?1 AND ?2
                  AND longitude BETWEEN ?3 AND ?4
                ORDER BY population DESC
                LIMIT 100
                "#,
            )
            .ok()?;

        let cities: Vec<(String, String, String, f64, f64)> = stmt
            .query_map(params![min_lat, max_lat, min_lon, max_lon], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .ok()?
            .filter_map(|r| r.ok())
            .collect();

        let mut nearest: Option<(GeocodingResult, f64)> = None;

        for (city_name, country_name, country_code, city_lat, city_lon) in cities {
            let distance = Self::haversine_distance(lat, lon, city_lat, city_lon);
            match &nearest {
                None => {
                    nearest = Some((
                        GeocodingResult {
                            city: city_name,
                            country: country_name,
                            country_code,
                            distance_km: distance,
                        },
                        distance,
                    ));
                }
                Some((_, min_dist)) if distance < *min_dist => {
                    nearest = Some((
                        GeocodingResult {
                            city: city_name,
                            country: country_name,
                            country_code,
                            distance_km: distance,
                        },
                        distance,
                    ));
                }
                _ => {}
            }
        }

        nearest.map(|(r, _)| r)
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

    pub fn get_country_name(&self, code: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT name FROM countries WHERE code = ?1",
                params![code],
                |row| row.get(0),
            )
            .ok()
    }

    pub fn batch_geocode(&self, coords: &[(f64, f64)]) -> Vec<Option<GeocodingResult>> {
        coords
            .iter()
            .map(|(lat, lon)| self.reverse_geocode(*lat, *lon))
            .collect()
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
