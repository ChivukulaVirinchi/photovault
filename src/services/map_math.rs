//! Web Mercator projection + slippy tile math.
//!
//! Tiles follow the OSM/Google/Bing convention: tile (0,0) at zoom 0
//! covers the whole world. At zoom `z`, the world is a 2^z x 2^z grid
//! of 256x256 tiles.

pub const TILE_SIZE: f64 = 256.0;
pub const MIN_ZOOM: u8 = 2;
pub const MAX_ZOOM: u8 = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub center: LatLng,
    pub zoom: u8,
}

/// Longitude to fractional tile X at zoom z.
pub fn lng_to_tile_x(lng: f64, z: u8) -> f64 {
    let n = 2f64.powi(z as i32);
    (lng + 180.0) / 360.0 * n
}

/// Latitude to fractional tile Y at zoom z (Web Mercator).
/// Clamps `lat` to [-85.0511, 85.0511] — the Mercator cutoff.
pub fn lat_to_tile_y(lat: f64, z: u8) -> f64 {
    let lat = lat.clamp(-85.05112878, 85.05112878);
    let rad = lat.to_radians();
    let n = 2f64.powi(z as i32);
    (1.0 - (rad.tan() + 1.0 / rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n
}

pub fn tile_x_to_lng(x: f64, z: u8) -> f64 {
    let n = 2f64.powi(z as i32);
    x / n * 360.0 - 180.0
}

pub fn tile_y_to_lat(y: f64, z: u8) -> f64 {
    let n = 2f64.powi(z as i32);
    let rad = (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan();
    rad.to_degrees()
}

/// Given a viewport, return the (tile_x, tile_y) at the viewport's
/// center in fractional tile coordinates.
pub fn viewport_center_tile(v: &Viewport) -> (f64, f64) {
    (
        lng_to_tile_x(v.center.lng, v.zoom),
        lat_to_tile_y(v.center.lat, v.zoom),
    )
}

/// Project a LatLng to a pixel (x, y) within the viewport.
/// Origin (0,0) is the top-left of the viewport.
pub fn latlng_to_viewport_pixel(v: &Viewport, p: LatLng) -> (f32, f32) {
    let (cx, cy) = viewport_center_tile(v);
    let px = lng_to_tile_x(p.lng, v.zoom);
    let py = lat_to_tile_y(p.lat, v.zoom);
    let dx = (px - cx) * TILE_SIZE;
    let dy = (py - cy) * TILE_SIZE;
    (
        (v.width as f64 / 2.0 + dx) as f32,
        (v.height as f64 / 2.0 + dy) as f32,
    )
}

/// Inverse: a viewport pixel -> LatLng.
pub fn viewport_pixel_to_latlng(v: &Viewport, px: f32, py: f32) -> LatLng {
    let (cx, cy) = viewport_center_tile(v);
    let dx = (px as f64 - v.width as f64 / 2.0) / TILE_SIZE;
    let dy = (py as f64 - v.height as f64 / 2.0) / TILE_SIZE;
    LatLng {
        lng: tile_x_to_lng(cx + dx, v.zoom),
        lat: tile_y_to_lat(cy + dy, v.zoom),
    }
}

/// Return every tile that intersects the viewport.
pub fn visible_tiles(v: &Viewport) -> Vec<TileId> {
    let (cx, cy) = viewport_center_tile(v);
    let half_w = v.width as f64 / 2.0 / TILE_SIZE;
    let half_h = v.height as f64 / 2.0 / TILE_SIZE;
    let min_x = (cx - half_w).floor() as i64;
    let max_x = (cx + half_w).ceil() as i64;
    let min_y = (cy - half_h).floor() as i64;
    let max_y = (cy + half_h).ceil() as i64;
    let n = 1i64 << v.zoom;
    let mut out = Vec::with_capacity(((max_x - min_x + 1) * (max_y - min_y + 1)) as usize);

    for ty in min_y..=max_y {
        if ty < 0 || ty >= n {
            continue;
        }
        for tx in min_x..=max_x {
            if tx < 0 || tx >= n {
                continue;
            }
            out.push(TileId {
                z: v.zoom,
                x: tx as u32,
                y: ty as u32,
            });
        }
    }

    out
}

/// Compute the bounding box of a set of pins, then return a (center,
/// zoom) that fits all pins with a small padding margin.
/// Returns a default map center + MIN_ZOOM if pins is empty.
pub fn fit_bounds(pins: &[LatLng], viewport_w: f32, viewport_h: f32) -> (LatLng, u8) {
    if pins.is_empty() {
        return (
            LatLng {
                lat: 20.0,
                lng: 0.0,
            },
            MIN_ZOOM,
        );
    }

    if pins.len() == 1 {
        return (pins[0], 13);
    }

    let (mut min_lat, mut max_lat) = (f64::MAX, f64::MIN);
    let (mut min_lng, mut max_lng) = (f64::MAX, f64::MIN);
    for p in pins {
        min_lat = min_lat.min(p.lat);
        max_lat = max_lat.max(p.lat);
        min_lng = min_lng.min(p.lng);
        max_lng = max_lng.max(p.lng);
    }

    let center = LatLng {
        lat: (min_lat + max_lat) / 2.0,
        lng: (min_lng + max_lng) / 2.0,
    };

    for z in (MIN_ZOOM..=MAX_ZOOM).rev() {
        let v = Viewport {
            width: viewport_w,
            height: viewport_h,
            center,
            zoom: z,
        };
        let (tl_px, tl_py) = latlng_to_viewport_pixel(
            &v,
            LatLng {
                lat: max_lat,
                lng: min_lng,
            },
        );
        let (br_px, br_py) = latlng_to_viewport_pixel(
            &v,
            LatLng {
                lat: min_lat,
                lng: max_lng,
            },
        );
        let bbox_w = (br_px - tl_px).abs();
        let bbox_h = (br_py - tl_py).abs();
        if bbox_w <= viewport_w * 0.8 && bbox_h <= viewport_h * 0.8 {
            return (center, z);
        }
    }

    (center, MIN_ZOOM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom0_is_one_tile() {
        assert!((lng_to_tile_x(-180.0, 0) - 0.0).abs() < 1e-9);
        assert!((lng_to_tile_x(180.0, 0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn greenwich_is_half_world() {
        assert!((lng_to_tile_x(0.0, 1) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn inverse_roundtrip() {
        for z in [2u8, 5, 10, 18] {
            for &lat in &[-60.0, -30.0, 0.0, 30.0, 60.0] {
                for &lng in &[-170.0, -90.0, 0.0, 90.0, 170.0] {
                    let x = lng_to_tile_x(lng, z);
                    let y = lat_to_tile_y(lat, z);
                    let lng2 = tile_x_to_lng(x, z);
                    let lat2 = tile_y_to_lat(y, z);

                    assert!((lng - lng2).abs() < 1e-6, "lng z={} {} -> {}", z, lng, lng2);
                    assert!((lat - lat2).abs() < 1e-6, "lat z={} {} -> {}", z, lat, lat2);
                }
            }
        }
    }

    #[test]
    fn viewport_center_projects_to_middle() {
        let v = Viewport {
            width: 800.0,
            height: 600.0,
            center: LatLng {
                lat: 12.9716,
                lng: 77.5946,
            },
            zoom: 10,
        };

        let (px, py) = latlng_to_viewport_pixel(&v, v.center);
        assert!((px - 400.0).abs() < 1.0);
        assert!((py - 300.0).abs() < 1.0);
    }

    #[test]
    fn pixel_to_latlng_inverse() {
        let v = Viewport {
            width: 1000.0,
            height: 800.0,
            center: LatLng {
                lat: 48.8566,
                lng: 2.3522,
            },
            zoom: 12,
        };

        let ll = viewport_pixel_to_latlng(&v, 500.0, 400.0);
        assert!((ll.lat - v.center.lat).abs() < 1e-6);
        assert!((ll.lng - v.center.lng).abs() < 1e-6);
    }

    #[test]
    fn visible_tiles_zoom2_paris() {
        let v = Viewport {
            width: 800.0,
            height: 600.0,
            center: LatLng {
                lat: 48.85,
                lng: 2.35,
            },
            zoom: 2,
        };

        let tiles = visible_tiles(&v);
        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|t| t.z == 2));
        assert!(tiles.iter().all(|t| t.x < 4 && t.y < 4));
    }

    #[test]
    fn fit_bounds_single_point_zooms_in() {
        let pins = vec![LatLng {
            lat: 12.97,
            lng: 77.59,
        }];
        let (c, z) = fit_bounds(&pins, 1000.0, 800.0);
        assert_eq!(z, 13);
        assert!((c.lat - 12.97).abs() < 1e-6);
    }

    #[test]
    fn fit_bounds_world_wide_returns_min_zoomish() {
        let pins = vec![
            LatLng {
                lat: -60.0,
                lng: -170.0,
            },
            LatLng {
                lat: 60.0,
                lng: 170.0,
            },
        ];
        let (_c, z) = fit_bounds(&pins, 800.0, 600.0);
        assert!(z <= 3);
    }
}
