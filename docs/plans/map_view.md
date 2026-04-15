# Implementation Plan — Map View Feature

## Context

PhotoVault stores a `latitude` / `longitude` column for each photo (populated
during EXIF scan), and a reverse-geocoding service exists to resolve those
coordinates to place names against an offline GeoNames database. Today,
nothing surfaces this spatial data. Users can't see "where have I been",
can't navigate by location, and the place name for a single photo is only
visible as text in the info panel.

This plan introduces a **Map View** — an interactive, online-first map that
renders every GPS-tagged photo as a pin on a real cartographic basemap. It
ships in two surfaces:

1. **Full-screen Map view** — sidebar entry. Pan + zoom a world map.
   All geotagged photos shown as pins. Pins cluster at low zoom levels
   (one bubble per region with count), split naturally as you zoom in.
   Clicking a pin opens a thumbnail popover → filmstrip of photos there.

2. **Mini-map in Photo Detail** — a 240×160 embedded widget in the
   existing photo info panel. Shows one tile at zoom 13 centered on the
   photo's coords, with a single pin. Reverse-geocoded place name
   appears as a text label beneath the map. Non-interactive.

Both surfaces share a single underlying `MapWidget` component, a shared
`tile_cache` service, and a shared `map_math` module. This is a
deliberate architectural choice — one widget, two configurations.

### The online-first decision

Google Photos' desktop app has no map view at all. Windows Photos shows a
static thumbnail in File Info and punts to an external Maps app for
anything more. PhotoVault can comfortably do better than both by
streaming OSM-compatible tiles on demand and caching them to disk
forever after first fetch.

The offline-first promise is preserved in practice: after the first view
of any given region, every tile at every zoom level for that region
stays in the local cache and renders without network access. New regions
(the user opens a different part of the world) need internet once.

### Locked decisions (do not revisit during implementation)

- **Tile provider**: CartoDB Positron
  (`https://{a,b,c}.basemaps.cartocdn.com/light_all/{z}/{x}/{y}.png`).
  Clean minimalist style, excellent pin contrast, free, no API key,
  attribution required.
- **Attribution string** (must appear visibly on full-screen map,
  small text, bottom-right): *"© OpenStreetMap contributors © CARTO"*.
- **Zoom range**: 2 (world) to 18 (building). Initial zoom for
  full-screen map = fit-to-bbox of all pins (or zoom 2 if no pins).
  Mini-map zoom = fixed at 13.
- **Tile size**: 256×256 (CartoDB standard; slippy-map convention).
- **Concurrent fetches**: max 2. Implemented via tokio semaphore.
- **User-Agent header**: `"PhotoVault/0.1 (https://github.com/photovault)"`.
  CartoDB and OSM both require non-empty User-Agent.
- **Cache location**: `<drive>/.photovault/tile_cache/{z}/{x}/{y}.png`.
  Per-drive (not global) so removing the drive removes its cache.
- **Cache size cap**: 500 MB default, user-configurable in Settings.
  LRU eviction via file mtime; sweep runs on app startup and every 10 min.
- **Offline fallback**: when a tile isn't cached and network fetch fails,
  render the nearest cached parent tile (zoom - 1, parent (x/2, y/2))
  scaled up 2× and drawn clipped to the missing tile's position. If no
  ancestor exists in cache either, draw a neutral grey square labeled "◯".
- **Pin clustering threshold**: two pins cluster if their projected
  screen-pixel distance < 40px. Simple O(n²) pass is fine; we'll never
  have more than ~50k pins on screen.
- **Pin style**: filled circle, 8px radius, CartoDB-accent blue
  (`#1976d2`) with white stroke. Cluster bubble: larger (14-24px, scaled
  by count), same colors, count rendered as white text inside.
- **Interaction**: drag to pan, scroll wheel to zoom (snaps to integer
  zoom), click pin to open popover with thumbnail grid. Keyboard: `+`/`-`
  to zoom, arrow keys to pan, `Esc` to close popover.
- **No turn-by-turn, no search-within-map, no drawing tools.** This is
  a photo-location browser, not a navigation app.

## Implementation order

Phases run in strict sequence. Each phase must `cargo build` clean (zero
new warnings) and — where tests exist — `cargo test --lib` must pass
before moving on.

| Phase | What | Depends on |
|-------|------|------------|
| 0 | Cargo deps + cache dir scaffolding | — |
| 1 | `map_math.rs` (pure functions, Web Mercator) | 0 |
| 2 | `tile_cache.rs` (fetch + disk + LRU) | 0, 1 |
| 3 | State + messages + handler skeleton | 2 |
| 4 | `map_widget.rs` — the custom iced widget | 3 |
| 5 | Full-screen `View::Map` | 4 |
| 6 | Pin clustering on full-screen view | 5 |
| 7 | Mini-map embedded in PhotoDetail info panel | 4 |
| 8 | Settings: cache limit + clear cache button | 2, 4 |
| 9 | Polish: attribution, error states, reverse geocode line | 5, 7 |

Target test count after all phases: **56 existing (Memories) + 8 new
(map_math) + 4 new (tile_cache) = 68 total**.

---

## Phase 0 — Dependencies and cache directory

### 0.1 — Add HTTP client dependency

**File**: `Cargo.toml`

Add under `[dependencies]`, grouped near `tokio`  :

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream"] }
```

Rationale: `rustls-tls` avoids a system OpenSSL dep (important for
Windows portability). `stream` enables chunked body reads for tiles.
`default-features = false` disables bundled `native-tls` we don't need.

Verify: `cargo build` — downloads reqwest + rustls. Should succeed with
zero warnings.

### 0.2 — Add the tile cache directory convention

**File**: `src/db/paths.rs` (or wherever drive paths are resolved;
search for the function that returns the `.photovault` subdirectory)

If there is a helper like `photovault_dir(drive: &Path) -> PathBuf`,
add a sibling:

```rust
pub fn tile_cache_dir(drive: &Path) -> PathBuf {
    photovault_dir(drive).join("tile_cache")
}
```

If no such helper exists, define both in `src/services/tile_cache.rs`
(Phase 2) directly. Don't over-engineer — one function, one place.

### 0.3 — Sanity check

```bash
cargo build   # must pass with zero warnings
cargo test --lib   # existing 56 tests must still pass
```

Commit message for end of Phase 0:
`Map view: add reqwest dependency for tile fetching (Phase 0)`

---

## Phase 1 — Map math (pure, heavily tested)

**New file**: `src/services/map_math.rs`

All tile math lives here. Pure functions, no I/O, no async. Everything
is `f64` for precision; cast to `f32` at the canvas boundary.

### 1.1 — Constants and types

```rust
//! Web Mercator projection + slippy tile math.
//!
//! Tiles follow the OSM/Google/Bing convention: tile (0,0) at zoom 0
//! covers the whole world. At zoom `z`, the world is a 2^z × 2^z grid
//! of 256×256 tiles.

pub const TILE_SIZE: f64 = 256.0;
pub const MIN_ZOOM: u8 = 2;
pub const MAX_ZOOM: u8 = 18;

/// A tile address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

/// Geographic coordinate (degrees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

/// Viewport in CSS-pixel space, used for projecting pins.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub center: LatLng,
    pub zoom: u8,
}
```

### 1.2 — Forward projection (LatLng → fractional tile coords)

```rust
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
```

### 1.3 — Inverse projection (tile → LatLng)

```rust
pub fn tile_x_to_lng(x: f64, z: u8) -> f64 {
    let n = 2f64.powi(z as i32);
    x / n * 360.0 - 180.0
}

pub fn tile_y_to_lat(y: f64, z: u8) -> f64 {
    let n = 2f64.powi(z as i32);
    let rad = (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan();
    rad.to_degrees()
}
```

### 1.4 — Viewport helpers

```rust
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
    ((v.width as f64 / 2.0 + dx) as f32, (v.height as f64 / 2.0 + dy) as f32)
}

/// Inverse: a viewport pixel → LatLng.
pub fn viewport_pixel_to_latlng(v: &Viewport, px: f32, py: f32) -> LatLng {
    let (cx, cy) = viewport_center_tile(v);
    let dx = (px as f64 - v.width as f64 / 2.0) / TILE_SIZE;
    let dy = (py as f64 - v.height as f64 / 2.0) / TILE_SIZE;
    LatLng {
        lng: tile_x_to_lng(cx + dx, v.zoom),
        lat: tile_y_to_lat(cy + dy, v.zoom),
    }
}

/// Return every tile that intersects the viewport. Returned list is
/// small (typically 4-16 tiles for a 1200×800 viewport).
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
        if ty < 0 || ty >= n { continue; }
        for tx in min_x..=max_x {
            // Wrap longitude at the antimeridian so panning past ±180 works.
            let wrapped_x = ((tx % n) + n) % n;
            out.push(TileId { z: v.zoom, x: wrapped_x as u32, y: ty as u32 });
        }
    }
    out
}

/// Compute the bounding box of a set of pins, then return a (center,
/// zoom) that fits all pins with a small padding margin.
/// Returns (LatLng::default, MIN_ZOOM) if pins is empty.
pub fn fit_bounds(pins: &[LatLng], viewport_w: f32, viewport_h: f32) -> (LatLng, u8) {
    if pins.is_empty() {
        return (LatLng { lat: 20.0, lng: 0.0 }, MIN_ZOOM);
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
    // Find the largest zoom where the bbox fits in 80% of the viewport.
    for z in (MIN_ZOOM..=MAX_ZOOM).rev() {
        let v = Viewport { width: viewport_w, height: viewport_h, center, zoom: z };
        let (tl_px, tl_py) = latlng_to_viewport_pixel(&v, LatLng { lat: max_lat, lng: min_lng });
        let (br_px, br_py) = latlng_to_viewport_pixel(&v, LatLng { lat: min_lat, lng: max_lng });
        let bbox_w = (br_px - tl_px).abs();
        let bbox_h = (br_py - tl_py).abs();
        if bbox_w <= viewport_w * 0.8 && bbox_h <= viewport_h * 0.8 {
            return (center, z);
        }
    }
    (center, MIN_ZOOM)
}

/// For a missing tile, return the nearest cached ancestor ID.
/// Walks up zoom levels (parent = (x/2, y/2, z-1)) until z == MIN_ZOOM.
pub fn parent_tile(t: TileId) -> Option<TileId> {
    if t.z == 0 { return None; }
    Some(TileId { z: t.z - 1, x: t.x / 2, y: t.y / 2 })
}
```

### 1.5 — Unit tests (all in the same file)

```rust
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
            center: LatLng { lat: 12.9716, lng: 77.5946 },
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
            center: LatLng { lat: 48.8566, lng: 2.3522 },
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
            center: LatLng { lat: 48.85, lng: 2.35 },
            zoom: 2,
        };
        let tiles = visible_tiles(&v);
        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|t| t.z == 2));
        assert!(tiles.iter().all(|t| t.x < 4 && t.y < 4));
    }

    #[test]
    fn fit_bounds_single_point_zooms_in() {
        let pins = vec![LatLng { lat: 12.97, lng: 77.59 }];
        let (c, z) = fit_bounds(&pins, 1000.0, 800.0);
        assert_eq!(z, 13);
        assert!((c.lat - 12.97).abs() < 1e-6);
    }

    #[test]
    fn fit_bounds_world_wide_returns_min_zoom() {
        let pins = vec![
            LatLng { lat: -60.0, lng: -170.0 },
            LatLng { lat: 60.0, lng: 170.0 },
        ];
        let (_c, z) = fit_bounds(&pins, 800.0, 600.0);
        assert!(z <= 3);
    }
}
```

### 1.6 — Register module

**File**: `src/services/mod.rs`

Add: `pub mod map_math;`
Re-export nothing; callers use the full path `map_math::...`.

### 1.7 — Verify

```bash
cargo test --lib services::map_math
# Expect: 8 passed
```

Commit: `Map view: add map_math module with Web Mercator projection (Phase 1)`

---

## Phase 2 — Tile cache service

**New file**: `src/services/tile_cache.rs`

Responsibilities:
1. Resolve a `TileId` to an on-disk path.
2. Read cached tile bytes if present.
3. Fetch uncached tiles over HTTP (CartoDB Positron).
4. Enforce concurrency limit (2 parallel requests).
5. LRU eviction when cache exceeds the user's size cap.

### 2.1 — Module skeleton

```rust
//! On-demand tile fetching + disk cache.
//!
//! Cache layout: `<drive>/.photovault/tile_cache/{z}/{x}/{y}.png`.
//! Fetched over HTTPS from CartoDB Positron (free, no API key).
//! Attribution MUST be displayed: "© OpenStreetMap contributors © CARTO".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::Semaphore;

use super::map_math::TileId;

pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 500 * 1024 * 1024; // 500 MB
pub const USER_AGENT: &str = "PhotoVault/0.1 (https://github.com/photovault)";
const MAX_CONCURRENT_FETCHES: usize = 2;
const FETCH_TIMEOUT_SECS: u64 = 8;
const CARTO_SUBDOMAINS: [&str; 3] = ["a", "b", "c"];

/// Long-lived handle passed to the widget. Cloneable, cheap.
#[derive(Clone)]
pub struct TileCache {
    root: PathBuf,
    client: reqwest::Client,
    sem: Arc<Semaphore>,
    limit_bytes: u64,
}

impl TileCache {
    pub fn new(drive: &Path, limit_bytes: u64) -> Self {
        let root = drive.join(".photovault").join("tile_cache");
        let _ = std::fs::create_dir_all(&root);
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
            .build()
            .expect("reqwest client");
        Self {
            root,
            client,
            sem: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
            limit_bytes,
        }
    }

    pub fn tile_path(&self, t: TileId) -> PathBuf {
        self.root
            .join(t.z.to_string())
            .join(t.x.to_string())
            .join(format!("{}.png", t.y))
    }

    /// Cheap synchronous check — does the tile exist on disk?
    pub fn has(&self, t: TileId) -> bool {
        self.tile_path(t).exists()
    }

    /// Return the on-disk path if cached, else fetch it, write it, return
    /// the path. Bumps mtime on every call (for LRU).
    pub async fn get_or_fetch(&self, t: TileId) -> Result<PathBuf, String> {
        let path = self.tile_path(t);
        if path.exists() {
            // Touch mtime for LRU.
            let _ = std::fs::File::open(&path).and_then(|f| f.set_modified(SystemTime::now()));
            return Ok(path);
        }
        let _permit = self.sem.acquire().await.map_err(|e| e.to_string())?;
        let sub = CARTO_SUBDOMAINS[(t.x as usize + t.y as usize) % CARTO_SUBDOMAINS.len()];
        let url = format!(
            "https://{}.basemaps.cartocdn.com/light_all/{}/{}/{}.png",
            sub, t.z, t.x, t.y
        );
        let bytes = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("fetch {}: {}", url, e))?
            .error_for_status()
            .map_err(|e| format!("status {}: {}", url, e))?
            .bytes()
            .await
            .map_err(|e| format!("read {}: {}", url, e))?;

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, &bytes).map_err(|e| format!("write {:?}: {}", path, e))?;
        Ok(path)
    }

    /// Walk the cache, compute total bytes, evict oldest (by mtime) until
    /// under the size cap. Safe to call periodically.
    pub fn evict_if_over_limit(&self) -> Result<u64, String> {
        let mut entries: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let mut total: u64 = 0;
        walk_cache(&self.root, &mut entries, &mut total)?;
        if total <= self.limit_bytes {
            return Ok(total);
        }
        entries.sort_by_key(|e| e.1); // oldest first
        let mut freed = 0u64;
        for (path, _, size) in entries {
            if total - freed <= self.limit_bytes { break; }
            let _ = std::fs::remove_file(&path);
            freed += size;
        }
        Ok(total - freed)
    }

    /// Delete every tile. Used by the "Clear map cache" button.
    pub fn clear(&self) -> Result<(), String> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Sum of all tile bytes on disk. Cheap — O(n files).
    pub fn current_size_bytes(&self) -> u64 {
        let mut total = 0u64;
        let mut entries = Vec::new();
        let _ = walk_cache(&self.root, &mut entries, &mut total);
        total
    }

    pub fn set_limit_bytes(&mut self, limit: u64) {
        self.limit_bytes = limit;
    }
}

fn walk_cache(
    dir: &Path,
    out: &mut Vec<(PathBuf, SystemTime, u64)>,
    total: &mut u64,
) -> Result<(), String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()); };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_cache(&p, out, total)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("png") {
            if let Ok(md) = entry.metadata() {
                let size = md.len();
                *total += size;
                out.push((p, md.modified().unwrap_or(SystemTime::UNIX_EPOCH), size));
            }
        }
    }
    Ok(())
}
```

### 2.2 — Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn tile_path_layout() {
        let td = TempDir::new().unwrap();
        let cache = TileCache::new(td.path(), DEFAULT_CACHE_LIMIT_BYTES);
        let p = cache.tile_path(TileId { z: 5, x: 7, y: 11 });
        assert!(p.ends_with("tile_cache/5/7/11.png"));
    }

    #[test]
    fn has_false_on_empty() {
        let td = TempDir::new().unwrap();
        let cache = TileCache::new(td.path(), DEFAULT_CACHE_LIMIT_BYTES);
        assert!(!cache.has(TileId { z: 0, x: 0, y: 0 }));
    }

    #[test]
    fn clear_removes_all() {
        let td = TempDir::new().unwrap();
        let cache = TileCache::new(td.path(), DEFAULT_CACHE_LIMIT_BYTES);
        // Write a dummy tile manually.
        let p = cache.tile_path(TileId { z: 0, x: 0, y: 0 });
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"PNG").unwrap();
        assert!(cache.has(TileId { z: 0, x: 0, y: 0 }));
        cache.clear().unwrap();
        assert!(!cache.has(TileId { z: 0, x: 0, y: 0 }));
    }

    #[test]
    fn eviction_respects_limit() {
        let td = TempDir::new().unwrap();
        let cache = TileCache::new(td.path(), 100); // 100-byte limit
        for i in 0..10 {
            let p = cache.tile_path(TileId { z: 1, x: i, y: 0 });
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![0u8; 50]).unwrap();
        }
        // Total = 500 bytes. After eviction, ≤ 100.
        let remaining = cache.evict_if_over_limit().unwrap();
        assert!(remaining <= 100, "remaining {} > 100", remaining);
    }
}
```

Add `tempfile = "3"` under `[dev-dependencies]` in `Cargo.toml` if not
already present.

### 2.3 — Register module

**File**: `src/services/mod.rs`

Add: `pub mod tile_cache;`
Re-export: `pub use tile_cache::{TileCache, DEFAULT_CACHE_LIMIT_BYTES};`

### 2.4 — Verify

```bash
cargo test --lib services::tile_cache
# Expect: 4 passed (or 3 if the eviction test is skipped in CI)
```

Commit: `Map view: add tile_cache service with LRU eviction (Phase 2)`

---

## Phase 3 — State, messages, handler skeleton

### 3.1 — View variants

**File**: `src/app/state/mod.rs`

Add to the `View` enum next to `Memories`:

```rust
pub enum View {
    // ...
    Memories,
    MemoryDetail,
    Map,          // NEW
    // ...
}
```

### 3.2 — PhotoVault fields

**File**: `src/app/state/mod.rs`

Add near the bottom of the struct:

```rust
// --- Map view state ---
pub(crate) tile_cache: Option<crate::services::TileCache>,
pub(crate) map_center: crate::services::map_math::LatLng,
pub(crate) map_zoom: u8,
pub(crate) map_drag_origin: Option<(f32, f32)>,
pub(crate) map_pins_cache: Vec<(i64, crate::services::map_math::LatLng)>,
pub(crate) map_cache_limit_bytes: u64,
pub(crate) selected_cluster_photos: Vec<i64>,  // photos at the clicked pin
pub(crate) popover_position: Option<(f32, f32)>,
```

Initialize in `PhotoVault::new()` (the block that sets other defaults):

```rust
tile_cache: None,  // populated on select_drive
map_center: crate::services::map_math::LatLng { lat: 20.0, lng: 0.0 },
map_zoom: 2,
map_drag_origin: None,
map_pins_cache: Vec::new(),
map_cache_limit_bytes: crate::services::DEFAULT_CACHE_LIMIT_BYTES,
selected_cluster_photos: Vec::new(),
popover_position: None,
```

### 3.3 — Config field

**File**: `src/config/mod.rs`

Add to the config struct:

```rust
#[serde(default = "default_map_cache_limit_mb")]
pub map_cache_limit_mb: u32,

fn default_map_cache_limit_mb() -> u32 { 500 }
```

In `Default::default()` also set `map_cache_limit_mb: 500`.

Wire in `PhotoVault::new()`:

```rust
app.map_cache_limit_bytes = (config.map_cache_limit_mb as u64) * 1024 * 1024;
```

### 3.4 — Message variants

**File**: `src/app/messages.rs`

Add at the end of the `Message` enum:

```rust
// --- Map view ---
MapPan { dx: f32, dy: f32 },               // dragging
MapPanStart { x: f32, y: f32 },
MapPanEnd,
MapZoomAt { x: f32, y: f32, delta: i8 },   // +1 = in, -1 = out
MapResetView,                              // fit-to-pins
MapPinsLoaded(Vec<(i64, crate::services::map_math::LatLng)>),
MapTileFetched(crate::services::map_math::TileId),
MapTileFetchFailed(crate::services::map_math::TileId, String),
MapPinClicked { photo_ids: Vec<i64>, viewport_x: f32, viewport_y: f32 },
MapClosePopover,
MapOpenClusterFilmstrip,                   // from popover → filmstrip
SetMapCacheLimit(u32),                     // MB, from Settings
ClearMapCache,
MapCacheCleared,
```

### 3.5 — Handler skeleton

**New file**: `src/app/handlers/map.rs`

```rust
//! Map view handlers.

use iced::Task;

use crate::db::Database;
use crate::services::{map_math, TileCache};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

/// Called from scanning::select_drive after DB is ready.
pub(crate) fn init_tile_cache(app: &mut PhotoVault) {
    let Some(drive) = app.selected_drive.clone() else { return; };
    let cache = TileCache::new(&drive, app.map_cache_limit_bytes);
    app.tile_cache = Some(cache.clone());
    // Periodic eviction in background.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            let _ = cache.evict_if_over_limit();
        }
    });
}

/// Load pins for all non-trashed photos with GPS.
pub(crate) fn load_pins(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive) = app.selected_drive.clone() else { return Task::none(); };
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let Ok(db) = Database::open_for_drive(&drive) else { return Vec::new(); };
                let mut stmt = match db.conn.prepare(
                    "SELECT id, latitude, longitude FROM photos
                     WHERE is_trashed = FALSE
                       AND latitude IS NOT NULL
                       AND longitude IS NOT NULL"
                ) { Ok(s) => s, Err(_) => return Vec::new() };
                let rows: Vec<_> = stmt
                    .query_map([], |r| {
                        let id: i64 = r.get(0)?;
                        let lat: f64 = r.get(1)?;
                        let lng: f64 = r.get(2)?;
                        Ok((id, map_math::LatLng { lat, lng }))
                    })
                    .ok()
                    .into_iter()
                    .flat_map(|it| it.flatten())
                    .collect();
                rows
            })
            .await
            .unwrap_or_default()
        },
        Message::MapPinsLoaded,
    )
}

pub(crate) fn pins_loaded(
    app: &mut PhotoVault,
    pins: Vec<(i64, map_math::LatLng)>,
) -> Task<Message> {
    app.map_pins_cache = pins;
    // Fit the view to pins if this is the user's first entry.
    let positions: Vec<_> = app.map_pins_cache.iter().map(|(_, ll)| *ll).collect();
    let (center, zoom) = map_math::fit_bounds(
        &positions,
        app.window_width as f32,
        app.window_height as f32,
    );
    app.map_center = center;
    app.map_zoom = zoom;
    Task::none()
}

pub(crate) fn pan_start(app: &mut PhotoVault, x: f32, y: f32) -> Task<Message> {
    app.map_drag_origin = Some((x, y));
    Task::none()
}

pub(crate) fn pan(app: &mut PhotoVault, dx: f32, dy: f32) -> Task<Message> {
    // Convert pixel delta to tile-space delta → lat/lng delta.
    let v = map_math::Viewport {
        width: app.window_width as f32,
        height: app.window_height as f32,
        center: app.map_center,
        zoom: app.map_zoom,
    };
    let new_center_px = (v.width / 2.0 - dx, v.height / 2.0 - dy);
    let new_center = map_math::viewport_pixel_to_latlng(&v, new_center_px.0, new_center_px.1);
    app.map_center = new_center;
    Task::none()
}

pub(crate) fn pan_end(app: &mut PhotoVault) -> Task<Message> {
    app.map_drag_origin = None;
    Task::none()
}

pub(crate) fn zoom_at(app: &mut PhotoVault, x: f32, y: f32, delta: i8) -> Task<Message> {
    let old_zoom = app.map_zoom;
    let new_zoom = ((old_zoom as i16) + delta as i16)
        .clamp(map_math::MIN_ZOOM as i16, map_math::MAX_ZOOM as i16) as u8;
    if new_zoom == old_zoom { return Task::none(); }
    // Keep the cursor's geo-position stable across the zoom.
    let v_old = map_math::Viewport {
        width: app.window_width as f32,
        height: app.window_height as f32,
        center: app.map_center,
        zoom: old_zoom,
    };
    let cursor_geo = map_math::viewport_pixel_to_latlng(&v_old, x, y);
    // After zoom, we want cursor_geo to project to (x, y). Solve for
    // new center: offset the center by the same geo-delta.
    let v_new_probe = map_math::Viewport {
        width: app.window_width as f32,
        height: app.window_height as f32,
        center: cursor_geo,
        zoom: new_zoom,
    };
    let (px, py) = map_math::latlng_to_viewport_pixel(&v_new_probe, cursor_geo);
    let dx = x - px;
    let dy = y - py;
    // Center pixel where cursor should land:
    let target_center_px_x = v_new_probe.width / 2.0 - dx;
    let target_center_px_y = v_new_probe.height / 2.0 - dy;
    let new_center = map_math::viewport_pixel_to_latlng(
        &v_new_probe,
        target_center_px_x,
        target_center_px_y,
    );
    app.map_center = new_center;
    app.map_zoom = new_zoom;
    Task::none()
}

pub(crate) fn reset_view(app: &mut PhotoVault) -> Task<Message> {
    let positions: Vec<_> = app.map_pins_cache.iter().map(|(_, ll)| *ll).collect();
    let (center, zoom) = map_math::fit_bounds(
        &positions,
        app.window_width as f32,
        app.window_height as f32,
    );
    app.map_center = center;
    app.map_zoom = zoom;
    Task::none()
}

pub(crate) fn pin_clicked(
    app: &mut PhotoVault,
    photo_ids: Vec<i64>,
    vx: f32,
    vy: f32,
) -> Task<Message> {
    app.selected_cluster_photos = photo_ids;
    app.popover_position = Some((vx, vy));
    Task::none()
}

pub(crate) fn close_popover(app: &mut PhotoVault) -> Task<Message> {
    app.selected_cluster_photos.clear();
    app.popover_position = None;
    Task::none()
}

pub(crate) fn open_cluster_filmstrip(app: &mut PhotoVault) -> Task<Message> {
    if app.selected_cluster_photos.is_empty() { return Task::none(); }
    // Reuse PhotoDetail with a custom nav list = the cluster's photos.
    let first = app.selected_cluster_photos[0];
    if let Some(idx) = app.photos.iter().position(|p| p.id == first) {
        app.previous_view = Some(View::Map);
        app.selected_photo_index = Some(idx);
        app.current_view = View::PhotoDetail;
        return app.load_photo_detail_for_index(idx);
    }
    Task::none()
}

pub(crate) fn set_cache_limit(app: &mut PhotoVault, mb: u32) -> Task<Message> {
    app.map_cache_limit_bytes = (mb as u64) * 1024 * 1024;
    app.config.map_cache_limit_mb = mb;
    let _ = app.config.save();
    if let Some(ref mut c) = app.tile_cache {
        c.set_limit_bytes(app.map_cache_limit_bytes);
        let _ = c.evict_if_over_limit();
    }
    Task::none()
}

pub(crate) fn clear_cache(app: &mut PhotoVault) -> Task<Message> {
    let cache = app.tile_cache.clone();
    Task::perform(
        async move {
            if let Some(c) = cache {
                let _ = c.clear();
            }
        },
        |_| Message::MapCacheCleared,
    )
}

pub(crate) fn cache_cleared(_app: &mut PhotoVault) -> Task<Message> {
    tracing::info!("Map tile cache cleared");
    Task::none()
}

// The two Tile{Fetched,FetchFailed} messages are emitted by the widget's
// subscription/task. They just trigger a re-render; no state change needed
// because the widget reads the cache directly.
pub(crate) fn tile_event(_app: &mut PhotoVault) -> Task<Message> {
    Task::none()
}
```

### 3.6 — Handler dispatch

**File**: `src/app/handlers/mod.rs`

Add `mod map;` and in the big `match message` block:

```rust
Message::MapPan { dx, dy } => map::pan(app, dx, dy),
Message::MapPanStart { x, y } => map::pan_start(app, x, y),
Message::MapPanEnd => map::pan_end(app),
Message::MapZoomAt { x, y, delta } => map::zoom_at(app, x, y, delta),
Message::MapResetView => map::reset_view(app),
Message::MapPinsLoaded(p) => map::pins_loaded(app, p),
Message::MapTileFetched(_) => map::tile_event(app),
Message::MapTileFetchFailed(_, _) => map::tile_event(app),
Message::MapPinClicked { photo_ids, viewport_x, viewport_y } =>
    map::pin_clicked(app, photo_ids, viewport_x, viewport_y),
Message::MapClosePopover => map::close_popover(app),
Message::MapOpenClusterFilmstrip => map::open_cluster_filmstrip(app),
SetMapCacheLimit(mb) => map::set_cache_limit(app, mb),
ClearMapCache => map::clear_cache(app),
MapCacheCleared => map::cache_cleared(app),
```

### 3.7 — Initialize tile cache on drive select

**File**: `src/app/handlers/scanning.rs`

Inside `select_drive`, after DB opens and migrations run, before
returning:

```rust
super::map::init_tile_cache(app);
let pin_task = super::map::load_pins(app);
// Merge with any existing task via Task::batch or chain.
```

### 3.8 — NavigateTo dispatch

**File**: same, `navigate_to` function:

```rust
} else if view == View::Map {
    app.current_view = view;
    if app.map_pins_cache.is_empty() {
        return super::map::load_pins(app);
    }
    return Task::none();
}
```

### 3.9 — Verify

```bash
cargo build   # zero warnings
cargo test --lib   # 68 tests (56 + 8 map_math + 4 tile_cache) must pass
```

Commit: `Map view: state, messages, handler skeleton (Phase 3)`

---

## Phase 4 — The Map widget

This is the meatiest phase. Build a custom iced widget that renders
tiles + pins and handles pan/zoom events.

### 4.1 — Implementation strategy decision

iced 0.13's `canvas::Frame` does NOT support `draw_image`. We therefore
compose the widget as a **layered view**:

- **Bottom layer**: an iced `Stack` (via nested columns/rows with
  absolute positioning helper) of `iced::widget::image(handle)` elements,
  one per visible tile, each offset by a computed pixel position.
- **Top layer**: a `canvas::Canvas` that draws pins + click hit-targets
  on a transparent background over the tile grid.
- **Interaction**: wrap the whole thing in a `mouse_area` to capture
  drag, wheel, and click events.

iced 0.13 does expose `iced::widget::stack!` (added in 0.13.0) that
renders children on top of each other, each taking the full available
size. Use that for the layering.

### 4.2 — Widget config + constructor

**New file**: `src/components/map_widget.rs`

```rust
//! Interactive map widget. Renders cached tiles + photo pins.
//! Used full-screen by View::Map and as a mini-map by PhotoDetail.

use std::collections::HashSet;
use std::path::PathBuf;

use iced::advanced::image::Handle;
use iced::mouse::{self, Cursor};
use iced::widget::{canvas, container, image as iced_image, mouse_area, stack, Space};
use iced::{Color, Element, Length, Point, Rectangle, Size};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::map_math::{self, LatLng, TileId, Viewport};
use crate::services::TileCache;

pub struct MapWidgetConfig<'a> {
    pub cache: &'a TileCache,
    pub center: LatLng,
    pub zoom: u8,
    pub pins: &'a [(i64, LatLng)],
    pub viewport_size: Size,
    pub interactive: bool,
    pub show_attribution: bool,
    pub theme: AppTheme,
}

pub fn map_widget(cfg: MapWidgetConfig<'_>) -> Element<'static, Message> {
    let v = Viewport {
        width: cfg.viewport_size.width,
        height: cfg.viewport_size.height,
        center: cfg.center,
        zoom: cfg.zoom,
    };

    // Gather visible tiles; for each, decide what to render.
    let visible = map_math::visible_tiles(&v);

    // Kick off fetches for missing tiles (non-blocking).
    for tid in &visible {
        if !cfg.cache.has(*tid) {
            spawn_fetch(cfg.cache.clone(), *tid);
        }
    }

    // Build the tile layer.
    let tile_layer = build_tile_layer(&v, &visible, cfg.cache);
    // Build the pin+cluster layer as a canvas.
    let pin_layer = build_pin_layer(&v, cfg.pins, cfg.theme);

    let mut layers = stack![tile_layer, pin_layer];

    if cfg.show_attribution {
        layers = layers.push(attribution_layer(cfg.theme));
    }

    let body: Element<'static, Message> = container(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if cfg.interactive {
        mouse_area(body)
            .on_press(Message::MapPanStart { x: 0.0, y: 0.0 }) // overridden by closure below in v2
            .into()
    } else {
        body
    }
}
```

### 4.3 — Tile layer

```rust
fn build_tile_layer(
    v: &Viewport,
    visible: &[TileId],
    cache: &TileCache,
) -> Element<'static, Message> {
    let (cx, cy) = map_math::viewport_center_tile(v);
    let mut positioned: Vec<(f32, f32, Element<'static, Message>)> = Vec::new();

    for tid in visible {
        let dx = (tid.x as f64 - cx) * map_math::TILE_SIZE;
        let dy = (tid.y as f64 - cy) * map_math::TILE_SIZE;
        let screen_x = v.width as f64 / 2.0 + dx;
        let screen_y = v.height as f64 / 2.0 + dy;

        let path_opt = if cache.has(*tid) {
            Some(cache.tile_path(*tid))
        } else {
            ancestor_cached(cache, *tid)
        };

        let tile_element: Element<'static, Message> = match path_opt {
            Some(p) => iced_image(Handle::from_path(p))
                .width(Length::Fixed(map_math::TILE_SIZE as f32))
                .height(Length::Fixed(map_math::TILE_SIZE as f32))
                .into(),
            None => Space::new(
                Length::Fixed(map_math::TILE_SIZE as f32),
                Length::Fixed(map_math::TILE_SIZE as f32),
            ).into(),
        };

        positioned.push((screen_x as f32, screen_y as f32, tile_element));
    }

    // Build a composed element. Since iced doesn't ship absolute positioning,
    // use a hand-rolled `PositionedLayer` via a custom container or the
    // `advanced::layout` API. Simpler: for each tile, wrap it in a
    // `container` with precise padding so its top-left lands at (screen_x, screen_y).
    let mut stack_el = stack![];
    for (sx, sy, el) in positioned {
        let padded = container(el)
            .padding([sy.max(0.0) as u16, 0, 0, sx.max(0.0) as u16]);
        stack_el = stack_el.push(padded);
    }
    stack_el.into()
}

fn ancestor_cached(cache: &TileCache, t: TileId) -> Option<PathBuf> {
    let mut cur = t;
    while let Some(parent) = map_math::parent_tile(cur) {
        if cache.has(parent) {
            // Return the parent path; the widget draws it scaled up.
            return Some(cache.tile_path(parent));
        }
        cur = parent;
    }
    None
}

fn spawn_fetch(cache: TileCache, tid: TileId) {
    tokio::spawn(async move {
        let _ = cache.get_or_fetch(tid).await;
        // We don't dispatch a message — the next frame will observe the
        // tile via cache.has(tid). A cheap periodic redraw (below) handles
        // showing the newly arrived tile.
    });
}
```

**Known limitation of this approach**: iced's padding-based positioning
is clunky and can't produce negative offsets, so tiles that should hang
off the top/left edge are clipped. This is acceptable for v1 (the
viewport fills the window; edge tiles are rarely needed) but we note
this as a tech debt in Phase 9.

A cleaner production approach is to implement `advanced::Widget` with a
custom `layout` method. Worth doing in a follow-up — for v1 the
padding hack is good enough.

### 4.4 — Pin layer (canvas)

```rust
use iced::widget::canvas::{Path, Stroke, Text};

struct PinLayer {
    viewport: Viewport,
    pins: Vec<(i64, LatLng)>,
    theme: AppTheme,
}

impl<Message> canvas::Program<Message> for PinLayer {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let clusters = cluster_pins(&self.viewport, &self.pins);
        for c in clusters {
            let center = iced::Point::new(c.x, c.y);
            let radius = cluster_radius(c.count);
            let color = Color::from_rgb(0.098, 0.463, 0.824); // #1976d2
            frame.fill(&Path::circle(center, radius), color);
            frame.stroke(
                &Path::circle(center, radius),
                Stroke::default().with_width(2.0).with_color(Color::WHITE),
            );
            if c.count > 1 {
                frame.fill_text(Text {
                    content: c.count.to_string(),
                    position: center,
                    color: Color::WHITE,
                    size: iced::Pixels(11.0),
                    horizontal_alignment: iced::alignment::Horizontal::Center,
                    vertical_alignment: iced::alignment::Vertical::Center,
                    ..Default::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }
}

fn build_pin_layer(
    v: &Viewport,
    pins: &[(i64, LatLng)],
    theme: AppTheme,
) -> Element<'static, Message> {
    let program = PinLayer {
        viewport: *v,
        pins: pins.to_vec(),
        theme,
    };
    canvas(program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
```

### 4.5 — Cluster helper

```rust
#[derive(Debug, Clone)]
struct Cluster {
    x: f32,
    y: f32,
    count: usize,
    photo_ids: Vec<i64>,
}

const CLUSTER_DIST_PX: f32 = 40.0;

fn cluster_pins(v: &Viewport, pins: &[(i64, LatLng)]) -> Vec<Cluster> {
    let mut projected: Vec<(i64, f32, f32)> = pins.iter()
        .map(|(id, ll)| {
            let (px, py) = map_math::latlng_to_viewport_pixel(v, *ll);
            (*id, px, py)
        })
        // Drop pins that are clearly off-screen.
        .filter(|(_, px, py)| *px >= -40.0 && *py >= -40.0
            && *px <= v.width + 40.0 && *py <= v.height + 40.0)
        .collect();

    let mut out: Vec<Cluster> = Vec::new();
    let mut used = vec![false; projected.len()];
    for i in 0..projected.len() {
        if used[i] { continue; }
        let (id_i, px_i, py_i) = projected[i];
        let mut cluster = Cluster {
            x: px_i, y: py_i, count: 1, photo_ids: vec![id_i],
        };
        used[i] = true;
        for j in (i + 1)..projected.len() {
            if used[j] { continue; }
            let (id_j, px_j, py_j) = projected[j];
            if (px_i - px_j).hypot(py_i - py_j) < CLUSTER_DIST_PX {
                used[j] = true;
                cluster.count += 1;
                cluster.photo_ids.push(id_j);
                // Merge as running average.
                cluster.x = (cluster.x * (cluster.count - 1) as f32 + px_j) / cluster.count as f32;
                cluster.y = (cluster.y * (cluster.count - 1) as f32 + py_j) / cluster.count as f32;
            }
        }
        out.push(cluster);
    }
    out
}

fn cluster_radius(count: usize) -> f32 {
    match count {
        1 => 8.0,
        2..=10 => 12.0,
        11..=100 => 16.0,
        _ => 20.0,
    }
}
```

### 4.6 — Attribution layer

```rust
fn attribution_layer(theme: AppTheme) -> Element<'static, Message> {
    use iced::widget::text;
    let p = crate::theme::colors::palette(theme);
    container(
        text("© OpenStreetMap contributors © CARTO")
            .size(10)
            .color(p.text_tertiary),
    )
    .padding([2, 6])
    .align_bottom(Length::Fill)
    .align_right(Length::Fill)
    .into()
}
```

### 4.7 — Register module

**File**: `src/components/mod.rs`

Add: `pub mod map_widget;`

### 4.8 — Verify

```bash
cargo build   # zero warnings
```

Visually not testable yet — no view uses it. That's Phase 5.

Commit: `Map view: MapWidget component (Phase 4)`

---

## Phase 5 — Full-screen Map view

### 5.1 — The view function

**New file**: `src/views/map.rs`

```rust
//! Full-screen map view.

use iced::widget::{button, column, container, row, text, mouse_area, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::{Message, PhotoVault};
use crate::components::map_widget::{map_widget, MapWidgetConfig};
use crate::theme::colors;

pub fn map_view(app: &PhotoVault) -> Element<'_, Message> {
    let p = colors::palette(app.config.theme);

    let Some(ref cache) = app.tile_cache else {
        return empty_state(app);
    };

    let size = iced::Size::new(app.window_width as f32, app.window_height as f32 - 80.0);

    let widget = map_widget(MapWidgetConfig {
        cache,
        center: app.map_center,
        zoom: app.map_zoom,
        pins: &app.map_pins_cache,
        viewport_size: size,
        interactive: true,
        show_attribution: true,
        theme: app.config.theme,
    });

    let header = container(
        row![
            text("Map").size(22).color(p.text_primary),
            Space::with_width(Length::Fill),
            button(text("Reset view").size(12)).on_press(Message::MapResetView),
        ]
        .align_y(Alignment::Center)
        .spacing(16),
    )
    .padding(Padding { top: 16.0, right: 24.0, bottom: 8.0, left: 24.0 });

    // Interaction wrapper that translates mouse_area events into MapPan/MapZoomAt.
    let interactive = mouse_area(widget)
        .on_move(|_p: iced::Point| Message::MapClosePopover)
        .on_scroll(|_| Message::MapZoomAt { x: 0.0, y: 0.0, delta: 1 });
    // NOTE: iced's mouse_area in 0.13 doesn't expose wheel delta directly;
    // we'll need subscription-based handling in the final implementation.
    // For v1, the toolbar has +/- buttons (below) and keyboard shortcuts.

    let zoom_controls = container(
        column![
            button(text("+").size(20)).on_press(Message::MapZoomAt { x: size.width / 2.0, y: size.height / 2.0, delta: 1 }),
            Space::with_height(4),
            button(text("−").size(20)).on_press(Message::MapZoomAt { x: size.width / 2.0, y: size.height / 2.0, delta: -1 }),
        ],
    )
    .padding(Padding { top: 80.0, right: 16.0, bottom: 0.0, left: 0.0 })
    .align_right(Length::Fill);

    let map_area: Element<'_, Message> = iced::widget::stack![interactive, zoom_controls].into();

    // Optional popover on top when a pin is clicked.
    let body: Element<'_, Message> = if app.selected_cluster_photos.is_empty() {
        map_area
    } else {
        iced::widget::stack![map_area, popover(app)].into()
    };

    container(column![header, body])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn empty_state(app: &PhotoVault) -> Element<'_, Message> {
    let p = colors::palette(app.config.theme);
    let body = column![
        Space::with_height(48),
        text("Map").size(24).color(p.text_primary),
        Space::with_height(12),
        text("Select a drive to see your photos on a map.")
            .size(14)
            .color(p.text_secondary),
    ]
    .align_x(Alignment::Center);
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .into()
}

fn popover(app: &PhotoVault) -> Element<'_, Message> {
    let p = colors::palette(app.config.theme);
    let (x, y) = app.popover_position.unwrap_or((0.0, 0.0));
    let count = app.selected_cluster_photos.len();

    // Show up to 6 thumbnails in a 3×2 grid; "View all" if > 6.
    let thumbs: Element<'_, Message> = if count == 1 {
        // Single photo: inline thumbnail preview.
        single_photo_preview(app, app.selected_cluster_photos[0])
    } else {
        cluster_grid_preview(app)
    };

    let box_el = container(
        column![
            row![
                text(format!("{} photo{}", count, if count == 1 { "" } else { "s" }))
                    .size(14)
                    .color(p.text_primary),
                Space::with_width(Length::Fill),
                button(text("×").size(14)).on_press(Message::MapClosePopover),
            ].align_y(Alignment::Center),
            Space::with_height(8),
            thumbs,
            Space::with_height(8),
            button(text("Open filmstrip").size(12))
                .on_press(Message::MapOpenClusterFilmstrip),
        ]
        .padding(12)
        .width(Length::Fixed(280.0)),
    )
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(p.bg_elevated.into()),
        border: iced::Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    });

    container(box_el)
        .padding(Padding {
            top: y.max(0.0),
            left: (x - 140.0).max(0.0),
            right: 0.0,
            bottom: 0.0,
        })
        .into()
}

fn single_photo_preview(app: &PhotoVault, photo_id: i64) -> Element<'_, Message> {
    let p = colors::palette(app.config.theme);
    let photo = app.photos.iter().find(|p| p.id == photo_id);
    match photo.and_then(|ph| ph.thumbnail_path.as_ref()) {
        Some(path) => {
            let abs = app.selected_drive.as_ref()
                .map(|d| d.join(path).to_string_lossy().to_string())
                .unwrap_or_default();
            iced::widget::image(abs)
                .width(Length::Fixed(256.0))
                .height(Length::Fixed(192.0))
                .content_fit(iced::ContentFit::Cover)
                .into()
        }
        None => container(text("(no preview)").color(p.text_tertiary))
            .width(Length::Fixed(256.0))
            .height(Length::Fixed(192.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
    }
}

fn cluster_grid_preview(app: &PhotoVault) -> Element<'_, Message> {
    let mut grid = iced::widget::column![].spacing(4);
    let mut current_row = iced::widget::row![].spacing(4);
    let mut in_row = 0;
    for (i, id) in app.selected_cluster_photos.iter().take(6).enumerate() {
        let photo = app.photos.iter().find(|p| p.id == *id);
        let el: Element<'_, Message> = match photo.and_then(|ph| ph.thumbnail_path.as_ref()) {
            Some(path) => {
                let abs = app.selected_drive.as_ref()
                    .map(|d| d.join(path).to_string_lossy().to_string())
                    .unwrap_or_default();
                iced::widget::image(abs)
                    .width(Length::Fixed(80.0))
                    .height(Length::Fixed(80.0))
                    .content_fit(iced::ContentFit::Cover)
                    .into()
            }
            None => Space::new(Length::Fixed(80.0), Length::Fixed(80.0)).into(),
        };
        current_row = current_row.push(el);
        in_row += 1;
        if in_row == 3 {
            grid = grid.push(current_row);
            current_row = iced::widget::row![].spacing(4);
            in_row = 0;
        }
        let _ = i;
    }
    if in_row > 0 {
        grid = grid.push(current_row);
    }
    grid.into()
}
```

### 5.2 — Register module

**File**: `src/views/mod.rs`

Add: `pub mod map;`

### 5.3 — Dispatch in top-level view

**File**: `src/app/views.rs`

Add a match arm in `view()`:

```rust
View::Map => crate::views::map::map_view(app),
```

### 5.4 — Sidebar entry

**File**: `src/components/sidebar.rs`

Inside the sidebar button list, add between Timeline and Memories:

```rust
Self::nav_button("Map", View::Map, current_view, app_theme),
```

### 5.5 — Keyboard shortcuts

**File**: `src/app/handlers/timeline.rs`, in `key_pressed` — add an arm
for `View::Map`:

```rust
} else if app.current_view == View::Map {
    use iced::keyboard::key::Named;
    match key {
        keyboard::Key::Named(Named::ArrowLeft) =>
            return super::handle(app, Message::MapPan { dx: 50.0, dy: 0.0 }),
        keyboard::Key::Named(Named::ArrowRight) =>
            return super::handle(app, Message::MapPan { dx: -50.0, dy: 0.0 }),
        keyboard::Key::Named(Named::ArrowUp) =>
            return super::handle(app, Message::MapPan { dx: 0.0, dy: 50.0 }),
        keyboard::Key::Named(Named::ArrowDown) =>
            return super::handle(app, Message::MapPan { dx: 0.0, dy: -50.0 }),
        keyboard::Key::Named(Named::Escape) => {
            if !app.selected_cluster_photos.is_empty() {
                return super::handle(app, Message::MapClosePopover);
            }
        }
        keyboard::Key::Character(ref ch) => {
            let lower = ch.to_lowercase();
            if lower == "+" || lower == "=" {
                return super::handle(app, Message::MapZoomAt {
                    x: app.window_width as f32 / 2.0,
                    y: app.window_height as f32 / 2.0,
                    delta: 1,
                });
            }
            if lower == "-" || lower == "_" {
                return super::handle(app, Message::MapZoomAt {
                    x: app.window_width as f32 / 2.0,
                    y: app.window_height as f32 / 2.0,
                    delta: -1,
                });
            }
        }
        _ => {}
    }
}
```

### 5.6 — Redraw on tile arrival

Tiles fetched asynchronously must trigger a UI refresh. Use a cheap
polling subscription while the view is Map:

**File**: `src/app/mod.rs`, inside `subscription()`:

```rust
if self.current_view == state::View::Map {
    subs.push(
        iced::time::every(std::time::Duration::from_millis(500))
            .map(|_| Message::MapTileFetched(
                crate::services::map_math::TileId { z: 0, x: 0, y: 0 }
            )),
    );
}
```

This is a blunt but effective redraw pump while Map is open. No message
state change; just triggers the view function to be re-invoked, which
re-checks the cache for newly arrived tiles.

### 5.7 — Verify

```bash
cargo build
cargo run
```

- Click "Map" in sidebar → world-level basemap appears after a few
  seconds of tile fetches.
- Pins visible (if test drive has geotagged photos).
- `+` / `−` keys zoom in/out. Arrow keys pan.
- Clicking "Reset view" fits to pins.

Commit: `Map view: full-screen View::Map with interactive pan and zoom (Phase 5)`

---

## Phase 6 — Pin clustering + popover wiring

Clustering is already implemented in Phase 4. Phase 6 wires up pin
clicks to the popover.

### 6.1 — Hit-testing

The canvas `Program::update` method receives mouse events with positions
relative to the canvas bounds. Implement:

```rust
impl canvas::Program<Message> for PinLayer {
    // ... existing draw ...

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(pos) = cursor.position_in(bounds) else {
            return (canvas::event::Status::Ignored, None);
        };
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let clusters = cluster_pins(&self.viewport, &self.pins);
                for c in clusters {
                    let dx = c.x - pos.x;
                    let dy = c.y - pos.y;
                    if (dx * dx + dy * dy).sqrt() <= cluster_radius(c.count) + 4.0 {
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::MapPinClicked {
                                photo_ids: c.photo_ids,
                                viewport_x: c.x,
                                viewport_y: c.y,
                            }),
                        );
                    }
                }
            }
            _ => {}
        }
        (canvas::event::Status::Ignored, None)
    }
}
```

### 6.2 — Drag handling

Pan is handled via `mouse_area` on the outer container. Drag state is
stored on `PhotoVault.map_drag_origin`:

```rust
let interactive = mouse_area(widget)
    .on_press(Message::MapPanStart { x: 0.0, y: 0.0 })
    // iced 0.13's mouse_area gives us button events but the position
    // of the click is not directly in the message. Workaround: use a
    // subscription that listens to global mouse moves while dragging,
    // firing MapPan with the delta. See 6.3.
```

### 6.3 — Mouse move subscription while dragging

**File**: `src/app/mod.rs`, in `subscription()`:

```rust
if self.current_view == state::View::Map && self.map_drag_origin.is_some() {
    subs.push(iced::event::listen_with(|event, _status, _id| match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::MapPan { dx: position.x, dy: position.y })
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(Message::MapPanEnd)
        }
        _ => None,
    }));
}
```

Update `handlers::map::pan` to take raw cursor position and compute
delta against `map_drag_origin`:

```rust
pub(crate) fn pan(app: &mut PhotoVault, cursor_x: f32, cursor_y: f32) -> Task<Message> {
    let Some((origin_x, origin_y)) = app.map_drag_origin else { return Task::none(); };
    let dx = cursor_x - origin_x;
    let dy = cursor_y - origin_y;
    // Shift center in lat/lng based on pixel delta.
    let v = map_math::Viewport {
        width: app.window_width as f32,
        height: app.window_height as f32,
        center: app.map_center,
        zoom: app.map_zoom,
    };
    let new_center_px = (v.width / 2.0 - dx, v.height / 2.0 - dy);
    app.map_center = map_math::viewport_pixel_to_latlng(&v, new_center_px.0, new_center_px.1);
    app.map_drag_origin = Some((cursor_x, cursor_y));
    Task::none()
}
```

### 6.4 — Verify

```bash
cargo run
```

- Click-and-drag the map → pans in real time.
- Click a pin → popover appears showing photo count.
- Click "Open filmstrip" → PhotoDetail opens filtered to that cluster.
- `Esc` closes the popover.

Commit: `Map view: pin clustering, click popover, drag-to-pan (Phase 6)`

---

## Phase 7 — Mini-map in Photo Detail

### 7.1 — Where it goes

**File**: `src/app/views.rs` (or wherever `PhotoDetail` is rendered) —
locate the info panel that shows EXIF. Append the mini-map below (or
above, your call) the "Location: 12.97, 77.59" line.

### 7.2 — Mini-map renderer

**File**: `src/views/photo_detail_map.rs` (NEW)

```rust
//! Mini-map embedded in the photo detail info panel.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::{Message, PhotoVault};
use crate::components::map_widget::{map_widget, MapWidgetConfig};
use crate::models::Photo;
use crate::services::map_math::LatLng;
use crate::theme::colors;

const MINI_W: f32 = 240.0;
const MINI_H: f32 = 160.0;
const MINI_ZOOM: u8 = 13;

pub fn photo_mini_map(app: &PhotoVault, photo: &Photo) -> Option<Element<'_, Message>> {
    let lat = photo.latitude?;
    let lng = photo.longitude?;
    let cache = app.tile_cache.as_ref()?;

    let pin = (photo.id, LatLng { lat, lng });

    let widget = map_widget(MapWidgetConfig {
        cache,
        center: LatLng { lat, lng },
        zoom: MINI_ZOOM,
        pins: std::slice::from_ref(&pin),
        viewport_size: iced::Size::new(MINI_W, MINI_H),
        interactive: false,
        show_attribution: false,
        theme: app.config.theme,
    });

    let p = colors::palette(app.config.theme);
    let place = reverse_geocode(app, lat, lng)
        .unwrap_or_else(|| format!("{:.4}, {:.4}", lat, lng));

    Some(
        column![
            container(widget)
                .width(Length::Fixed(MINI_W))
                .height(Length::Fixed(MINI_H))
                .style(move |_t: &iced::Theme| container::Style {
                    border: iced::Border {
                        color: p.border_subtle,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }),
            Space::with_height(6),
            text(place).size(12).color(p.text_secondary),
        ]
        .align_x(Alignment::Start)
        .into()
    )
}

fn reverse_geocode(app: &PhotoVault, lat: f64, lng: f64) -> Option<String> {
    // Delegate to the existing geocoding service; signature may differ.
    // The important piece: call the existing service and return a
    // "City, Region" string when available.
    crate::services::geocoding::reverse_lookup_label(lat, lng).ok()
}
```

### 7.3 — Register module

**File**: `src/views/mod.rs`

Add: `pub mod photo_detail_map;`

### 7.4 — Wire into photo detail info panel

**File**: the existing photo detail view. Where the EXIF/info block is
assembled, insert:

```rust
if let Some(mini) = crate::views::photo_detail_map::photo_mini_map(app, photo) {
    info_col = info_col.push(mini);
    info_col = info_col.push(Space::with_height(12));
}
```

### 7.5 — Verify

```bash
cargo run
```

- Open a GPS-tagged photo → info panel shows a small map with a pin.
- Below the map: "Bangalore, Karnataka" (or equivalent) if reverse
  geocoding is working; else lat/lng as fallback.
- Open a non-GPS photo → mini-map simply doesn't appear. No empty box.

Commit: `Map view: mini-map in photo detail info panel (Phase 7)`

---

## Phase 8 — Settings: cache limit + clear cache

### 8.1 — Settings view additions

**File**: `src/views/settings.rs`

Add a new section "Map":

```rust
let map_section = column![
    text("Map").size(16).color(p.text_primary),
    Space::with_height(8),
    // Cache size display.
    row![
        text("Tile cache size:").size(12).color(p.text_secondary),
        Space::with_width(8),
        text(format!("{:.1} MB",
            app.tile_cache.as_ref()
                .map(|c| c.current_size_bytes() as f64 / 1024.0 / 1024.0)
                .unwrap_or(0.0)
        )).size(12).color(p.text_primary),
        Space::with_width(Length::Fill),
        button(text("Clear cache").size(12)).on_press(Message::ClearMapCache),
    ].align_y(Alignment::Center),
    Space::with_height(8),
    // Limit slider or number input.
    row![
        text("Cache size limit:").size(12).color(p.text_secondary),
        Space::with_width(8),
        text_input("MB", &app.map_cache_limit_bytes_display())
            .on_input(|s: String| {
                s.parse::<u32>()
                    .map(Message::SetMapCacheLimit)
                    .unwrap_or(Message::SetMapCacheLimit(500))
            })
            .width(Length::Fixed(80.0)),
        Space::with_width(4),
        text("MB").size(12).color(p.text_secondary),
    ].align_y(Alignment::Center),
];
```

Add a helper on `PhotoVault`:

```rust
impl PhotoVault {
    pub fn map_cache_limit_bytes_display(&self) -> String {
        (self.map_cache_limit_bytes / 1024 / 1024).to_string()
    }
}
```

### 8.2 — Verify

```bash
cargo run
```

- Open Settings → see "Map" section.
- Cache size shows current usage (grows as you pan the map).
- Clear cache → size drops to 0.
- Change limit → value saved to config; persists across restart.

Commit: `Map view: Settings — cache size display, limit, clear button (Phase 8)`

---

## Phase 9 — Polish, error states, and verification

### 9.1 — Visible error state when offline and nothing cached

If every visible tile has no ancestor in cache AND network fetches are
failing, the map shows a grey field. Add a one-line overlay:

```rust
// In build_tile_layer, track whether ANY tile (or ancestor) rendered.
// If none did AND we've observed fetch failures recently, push an
// error banner at the top of the map:
container(text("Offline — tiles for this region aren't cached yet.")
    .size(12).color(Color::WHITE))
    .style(|_t| container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
        ..Default::default()
    })
    .padding([6, 12])
```

Implement this as a bool flag on `PhotoVault`, set from the
`MapTileFetchFailed` handler:

```rust
pub(crate) fn tile_fetch_failed(app: &mut PhotoVault, _tid: TileId, err: String) -> Task<Message> {
    tracing::warn!("Tile fetch failed: {}", err);
    app.map_recent_fetch_failure = true;
    Task::none()
}
```

Reset the flag on successful fetch.

### 9.2 — Ensure mini-map pre-fetches tiles on photo detail open

When a photo is opened and it has GPS, proactively fetch the single
zoom-13 tile even before the view renders, so the mini-map never flashes
empty:

**File**: the photo-detail-open handler (likely `load_photo_detail_for_index`)

```rust
if let (Some(lat), Some(lng), Some(cache)) = (photo.latitude, photo.longitude, app.tile_cache.clone()) {
    let tile = crate::services::map_math::TileId {
        z: 13,
        x: crate::services::map_math::lng_to_tile_x(lng, 13) as u32,
        y: crate::services::map_math::lat_to_tile_y(lat, 13) as u32,
    };
    tokio::spawn(async move {
        let _ = cache.get_or_fetch(tile).await;
    });
}
```

### 9.3 — Thorough manual verification checklist

- [ ] `cargo build --release` clean, zero warnings.
- [ ] `cargo test --lib` — 68 tests pass (56 + 8 map_math + 4 tile_cache).
- [ ] Launch app, select a drive with GPS-tagged photos.
- [ ] Sidebar shows "Map" between Timeline and Memories.
- [ ] Click Map → world basemap loads in < 2 seconds (tiles fetching).
- [ ] Subsequent visits are instant (cached).
- [ ] Pan with drag → smooth, no stutter.
- [ ] Zoom with `+`/`−` → integer zoom, center stays at cursor position.
- [ ] Pins visible at every zoom level.
- [ ] Low zoom (≤ 5): pins cluster with count badges.
- [ ] High zoom (≥ 11): every photo has its own pin.
- [ ] Click a cluster → popover with grid of 6 thumbnails + "Open filmstrip".
- [ ] Click single pin → popover with one larger thumbnail.
- [ ] Popover "Open filmstrip" → PhotoDetail navigation is scoped to cluster.
- [ ] `Esc` closes popover.
- [ ] "Reset view" button → fits to all pins.
- [ ] Open a GPS photo → mini-map appears in info panel.
- [ ] Mini-map shows correct city.
- [ ] Place name text appears below mini-map.
- [ ] Open non-GPS photo → no mini-map, no empty placeholder.
- [ ] Settings → Map section visible.
- [ ] Cache size updates as you pan.
- [ ] Clear cache → size → 0.
- [ ] Change limit to 50 MB → LRU kicks in on next pan.
- [ ] Disconnect network mid-use → already-cached areas still work.
- [ ] Pan to un-cached region while offline → error banner shows.

Commit: `Map view: polish, error states, verified end-to-end (Phase 9)`

---

## Critical files reference

| File | Role |
|------|------|
| `Cargo.toml` | Add reqwest dependency |
| `src/services/map_math.rs` | **NEW** — Web Mercator projection, tile math, fit-bounds, pure & tested |
| `src/services/tile_cache.rs` | **NEW** — CartoDB fetch + disk cache + LRU eviction |
| `src/services/mod.rs` | Add `pub mod map_math; pub mod tile_cache;` + re-exports |
| `src/config/mod.rs` | Add `map_cache_limit_mb` field (default 500) |
| `src/app/state/mod.rs` | Add `View::Map`, tile_cache, map_center, map_zoom, map_drag_origin, map_pins_cache, map_cache_limit_bytes, selected_cluster_photos, popover_position |
| `src/app/messages.rs` | Add 14 Message variants for map |
| `src/app/handlers/map.rs` | **NEW** — all map message handlers |
| `src/app/handlers/mod.rs` | `mod map;` + dispatch 14 variants |
| `src/app/handlers/scanning.rs` | Call `map::init_tile_cache` on select_drive; handle `View::Map` in `navigate_to` |
| `src/app/handlers/timeline.rs` | Add View::Map branch to `key_pressed` |
| `src/app/mod.rs` | Add 500ms redraw-pump subscription + mouse subscription during drag |
| `src/app/views.rs` | Add `View::Map => crate::views::map::map_view(app)` arm |
| `src/components/map_widget.rs` | **NEW** — custom MapWidget (tile layer + pin canvas) |
| `src/components/sidebar.rs` | Add "Map" nav button between Timeline and Memories |
| `src/components/mod.rs` | Add `pub mod map_widget;` |
| `src/views/map.rs` | **NEW** — full-screen map view (header + interactive widget + popover) |
| `src/views/photo_detail_map.rs` | **NEW** — 240×160 mini-map component for info panel |
| `src/views/settings.rs` | Add Map section (cache size + limit + clear) |
| `src/views/mod.rs` | Add `pub mod map; pub mod photo_detail_map;` |
| `src/[wherever photo detail info lives]` | Wire mini-map into info panel |

---

## Known limitations / future work

1. **Tile layer positioning**: the padding-hack approach can't produce
   negative offsets. At extreme pans near the world edge, the outermost
   tiles may be clipped. Fix: implement a proper `advanced::Widget`
   with custom layout. Out of scope for v1.
2. **Smooth zoom**: we snap to integer zoom levels. Google Maps style
   fractional zoom (smooth pinch-zoom between 10.3 and 10.4) would
   require fractional-zoom tile scaling and is intentionally not in v1.
3. **Scroll wheel zoom at cursor**: iced 0.13's `mouse_area` doesn't
   expose wheel delta; v1 uses `+`/`−` buttons and keys. A proper
   widget implementation could hook `iced::event::listen_with` for
   scroll events. Worth revisiting.
4. **Raster-only**: no vector tiles (MVT/PMTiles). Positron looks good
   as a raster; we can revisit for bigger offline-first promise.
5. **No geocoding search box**: "find my photos in Delhi" requires
   forward geocoding. Fits naturally into the planned Unified Search
   feature — don't build it here.
6. **Heatmap mode**: the density heatmap variant that Google Photos
   mobile shows is nice but not essential. Easy to add later as a mode
   toggle — same pins, different canvas rendering.

---

## Rollback strategy

Each phase is self-contained:

- **Phase 0 failure**: revert `Cargo.toml`. Zero code impact.
- **Phase 1-2 failure**: delete new files, revert `services/mod.rs`.
- **Phase 3 failure**: revert state/messages/handlers changes.
- **Phase 4 failure**: delete `map_widget.rs`, revert `components/mod.rs`.
- **Phase 5+ failure**: revert the specific file, binary still compiles.

If the feature ships and misbehaves in production: remove the "Map" entry
from the sidebar (single line in `sidebar.rs`). The rest of the app is
unaffected; tile cache simply stops growing.

---

## Out of scope for this plan

- Vector tiles (PMTiles/MVT) — future consideration
- Heat-map rendering mode — easy follow-up, not essential
- Forward geocoding ("find photos near Delhi") — Unified Search plan
- Route lines / trip connectors — Auto Albums plan
- Offline tile bundle shipping with installer — rejected in favor of
  on-demand fetch
- Custom map styles beyond Positron — easy URL swap when requested
- Mapbox / Google Maps integration — rejected (needs API key, contradicts
  offline-first + keyless-setup principle)
- 3D terrain / satellite imagery — not needed for photo location
  browsing, huge engineering effort
