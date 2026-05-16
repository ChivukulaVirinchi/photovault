//! Shared helpers for engine-side integration tests.
//!
//! Each integration test file (`tests/<name>.rs`) compiles as its own
//! binary; cargo's idiomatic way to share helpers between them is a
//! `mod common;` line at the top of each file pointing at this
//! directory. The pattern is documented at
//! <https://doc.rust-lang.org/book/ch11-03-test-organization.html#submodules-in-integration-tests>.
//!
//! ## What lives here
//!
//! - [`make_jpeg`] / [`make_png`]: build a small, valid image byte
//!   buffer in memory (and write it to disk via [`write_jpeg`]).
//! - [`make_minimal_nef`]: build a TIFF byte string with an embedded
//!   JPEG preview — used to test the RAW pipeline without shipping
//!   actual camera RAWs.
//! - [`make_library`]: create a tempdir + run schema migrations so a
//!   test can call `Database::open_for_drive` against it.
//! - [`seed_photos`]: insert N synthetic photo rows for downstream
//!   tests (timeline, search, trash, geocoding, etc.).
//!
//! ## Conventions
//!
//! - Every helper returns either bytes or a `TempDir` whose lifetime
//!   controls cleanup — never leak filesystem state.
//! - All randomness is seeded so test failures are reproducible.
//! - No network. No system clock — fixed 2024-01-01 anchor.

#![allow(dead_code)] // helpers may be unused in any given test binary

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use image::{ImageBuffer, Rgb};
use tempfile::TempDir;

use smriti::db::photo_repo::{PhotoInsert, PhotoRepo};
use smriti::db::Database;

// ---------- image generators ----------

/// Produce a JPEG of approximately the given dimensions that
/// compresses to AT LEAST 10 KB on disk — the scanner enforces a
/// 10 KB minimum-file-size floor to avoid indexing icons / sprites
/// / favicons. We fill the buffer with deterministic pseudo-noise
/// (linear-congruential, seeded by `colour`) so the JPEG encoder
/// can't crush it down to a few hundred bytes the way it does with
/// solid colours. The visual appearance doesn't matter for any of
/// our tests; only the bytes / file size do.
///
/// Default callers can use width=256, height=256 to comfortably
/// clear the floor; on most platforms that produces a ~30-60 KB
/// file. Wider variations exist for callers that explicitly want
/// larger images.
pub fn make_jpeg(width: u32, height: u32, colour: [u8; 3]) -> Vec<u8> {
    let seed: u32 = u32::from(colour[0]).wrapping_mul(7919)
        ^ u32::from(colour[1]).wrapping_mul(7901)
        ^ u32::from(colour[2]).wrapping_mul(7883);
    let mut rng_state: u32 = seed.wrapping_add(1);
    let img = ImageBuffer::from_fn(width, height, |_x, _y| {
        // Pixel = colour ± deterministic noise. Linear-congruential RNG
        // — not cryptographic, just predictable across runs.
        rng_state = rng_state.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let n = ((rng_state >> 16) & 0x7F) as u8; // 0..127
        let r = colour[0].saturating_add(n / 2);
        let g = colour[1].saturating_add(n);
        let b = colour[2].saturating_add(n / 3);
        Rgb([r, g, b])
    });
    let mut buf = Cursor::new(Vec::with_capacity((width * height * 3) as usize));
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .expect("encode jpeg");
    buf.into_inner()
}

/// PNG sibling of [`make_jpeg`]. Useful for testing that the scanner
/// + thumbnail pipeline handle every supported format symmetrically.
///
/// PNG is lossless so we have to fill every pixel with genuinely
/// incompressible bytes — a per-pixel-independent LCG that mutates
/// all three channels by a wide range. With solid colour or the
/// jpeg-style additive noise, deflate compresses a 256×256 PNG down
/// to ~1.5 KB and the scanner's 10 KB filter discards it.
pub fn make_png(width: u32, height: u32, colour: [u8; 3]) -> Vec<u8> {
    let mut rng_r: u32 = u32::from(colour[0]).wrapping_add(0xDEAD_BEEF);
    let mut rng_g: u32 = u32::from(colour[1]).wrapping_add(0xCAFE_F00D);
    let mut rng_b: u32 = u32::from(colour[2]).wrapping_add(0xBAAD_F00D);
    let img = ImageBuffer::from_fn(width, height, |_x, _y| {
        // Three independent LCGs, full byte range. Looks like static —
        // exactly what we want for incompressibility.
        rng_r = rng_r.wrapping_mul(1_103_515_245).wrapping_add(12345);
        rng_g = rng_g.wrapping_mul(214_013).wrapping_add(2_531_011);
        rng_b = rng_b.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        Rgb([
            (rng_r >> 16) as u8,
            (rng_g >> 16) as u8,
            (rng_b >> 16) as u8,
        ])
    });
    let mut buf = Cursor::new(Vec::with_capacity((width * height * 3) as usize));
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode png");
    buf.into_inner()
}

/// Build a minimal little-endian TIFF with a single embedded JPEG
/// preview at a known offset. Mirrors the helper inside
/// `src/services/raw_preview.rs::tests::make_tiff` but exposed for
/// any integration test that needs to exercise the RAW pipeline
/// without shipping a real camera RAW.
///
/// `preview` is the bytes of the JPEG to embed — pass the output of
/// [`make_jpeg`] when you just need "any JPEG".
pub fn make_minimal_nef(preview: &[u8]) -> Vec<u8> {
    // Layout:
    //   [0..8]   header (II*\0, ifd0_offset = 8)
    //   [8..38]  IFD0: 2 entries (JPEGInterchangeFormat + Length) + NextIFD=0
    //   [38..]   the embedded JPEG bytes
    let preview_offset: u32 = 38;
    let preview_length: u32 = preview.len() as u32;

    let mut out = Vec::with_capacity(38 + preview.len());
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset

    // IFD0
    out.extend_from_slice(&2u16.to_le_bytes()); // entry count
                                                // Entry: JPEGInterchangeFormat (0x0201), type LONG, count 1, value = offset
    out.extend_from_slice(&0x0201u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&preview_offset.to_le_bytes());
    // Entry: JPEGInterchangeFormatLength (0x0202), type LONG, count 1, value = length
    out.extend_from_slice(&0x0202u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&preview_length.to_le_bytes());
    // NextIFD = 0 (chain ends)
    out.extend_from_slice(&0u32.to_le_bytes());

    // The embedded JPEG bytes
    out.extend_from_slice(preview);
    out
}

// ---------- on-disk fixtures ----------

/// Drop a JPEG at `dir/name` and return the full path. Convenience
/// wrapper for tests that need photos on disk to scan.
pub fn write_jpeg(dir: &Path, name: &str, width: u32, height: u32, colour: [u8; 3]) -> PathBuf {
    let path = dir.join(name);
    let bytes = make_jpeg(width, height, colour);
    fs::write(&path, &bytes).expect("write jpeg");
    path
}

/// Drop a synthetic NEF at `dir/name` and return the path. The NEF's
/// embedded JPEG is 64×48 with the given colour.
pub fn write_nef(dir: &Path, name: &str, colour: [u8; 3]) -> PathBuf {
    let path = dir.join(name);
    let preview = make_jpeg(64, 48, colour);
    let bytes = make_minimal_nef(&preview);
    fs::write(&path, &bytes).expect("write nef");
    path
}

// ---------- library fixtures ----------

/// A fresh-on-disk test library: tempdir with `.photovault/` already
/// created, schema applied, migrations run. Returns the TempDir
/// (keeps the directory alive while the test holds it) and the
/// `Database` handle ready to use.
///
/// The tempdir prefix is deliberately NOT dot-prefixed: the scanner
/// treats any directory whose name starts with `.` as hidden and
/// refuses to descend into it (mirrors the production behaviour of
/// "skip dotfiles unless asked"). tempfile's default `.tmpXXXX`
/// pattern would make the entire test library invisible to a
/// `start_scan` call.
pub fn make_library() -> (TempDir, Database) {
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .expect("tempdir");
    let database = Database::open_for_drive(dir.path()).expect("open_for_drive on fresh tempdir");
    smriti::db::create_schema(&database.conn).expect("create_schema");
    smriti::db::migrations::run_migrations(&database.conn).expect("run_migrations");
    (dir, database)
}

/// Insert `count` deterministic photo rows into the library. Each
/// photo:
///   - is named `IMG_<i>.jpg` with stable file_hash `hash-<i>`
///   - has a `date_taken` walking back from 2024-01-01 by one day
///   - has GPS at Vizag (17.68, 83.20) for the first half and no GPS
///     for the second half — so search / geocoding tests have
///     coverage of both branches.
///
/// Returns the count actually inserted (matches `count` on success).
pub fn seed_photos(db: &Database, count: usize) -> usize {
    let repo = PhotoRepo::new(&db.conn);
    let inserts: Vec<PhotoInsert> = (0..count)
        .map(|i| {
            let has_gps = i < count / 2;
            let date = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap()
                - chrono::Duration::days(i as i64);
            PhotoInsert {
                relative_path: format!("subdir/IMG_{:04}.jpg", i),
                file_name: format!("IMG_{:04}.jpg", i),
                file_hash: format!("hash-{:04}", i),
                file_size: 1024 * (1 + i as i64),
                file_mtime: Some(date.timestamp()),
                date_taken: Some(date.to_rfc3339()),
                date_taken_source: Some("exif".into()),
                gps_latitude: has_gps.then_some(17.68),
                gps_longitude: has_gps.then_some(83.20),
                location_city: None,
                location_country: None,
                camera_make: Some("Nikon".into()),
                camera_model: Some("Z 7II".into()),
                iso: Some(400),
                aperture: Some("f/2.8".into()),
                shutter_speed: Some("1/125".into()),
                focal_length: Some("50mm".into()),
                lens_model: None,
                flash: None,
                gps_altitude: None,
                width: Some(6048),
                height: Some(4024),
                orientation: 1,
            }
        })
        .collect();
    repo.insert_batch(&inserts).expect("seed insert_batch")
}
