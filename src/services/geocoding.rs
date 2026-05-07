//! Offline reverse geocoding service.

use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};

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

    fn search_bounding_box(&self, lat: f64, lon: f64, radius_deg: f64) -> Option<GeocodingResult> {
        let min_lat = lat - radius_deg;
        let max_lat = lat + radius_deg;
        let min_lon = lon - radius_deg;
        let max_lon = lon + radius_deg;

        // Filter to recognisable cities. Anything below ~100k pop is
        // typically a name only locals know — a 5k-person place a few km
        // from a metro still won the proximity contest and confused
        // users. 100k is roughly "you've heard of it on the news"; the
        // tradeoff is rural photos won't get a city tag at all, which
        // is the honest answer (the Approx. lat/lng fallback handles
        // those). Names come from `ascii_name` (no diacritics).
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT
                    ascii_name,
                    country_name,
                    country_code,
                    latitude,
                    longitude
                FROM cities
                WHERE latitude BETWEEN ?1 AND ?2
                  AND longitude BETWEEN ?3 AND ?4
                  AND population >= 100000
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

        for (city_name, country_name, _country_code, city_lat, city_lon) in cities {
            let distance = Self::haversine_distance(lat, lon, city_lat, city_lon);
            match &nearest {
                None => {
                    nearest = Some((
                        GeocodingResult {
                            city: city_name,
                            country: country_name,
                        },
                        distance,
                    ));
                }
                Some((_, min_dist)) if distance < *min_dist => {
                    nearest = Some((
                        GeocodingResult {
                            city: city_name,
                            country: country_name,
                        },
                        distance,
                    ));
                }
                _ => {}
            }
        }

        // Drop matches farther than the cutoff. Returning None here
        // surfaces "unknown location" upstream, which is more honest
        // than attributing to a far-away city.
        nearest.and_then(|(r, dist)| {
            if dist <= Self::MAX_CITY_DISTANCE_KM {
                Some(r)
            } else {
                None
            }
        })
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
