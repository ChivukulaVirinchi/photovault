//! Library Health diagnostics.
//!
//! Surfaces data-quality counters the user can act on:
//!   - photos missing thumbnails (file not on disk)
//!   - photos whose date came from mtime / file_meta (likely wrong)
//!   - photos with no date_taken at all
//!   - HEIC files in the library and whether the binary can decode them
//!   - photos that face-processing finished without finding any faces
//!
//! All numbers come from a single SQL pass plus filesystem stats for
//! the thumbnail check; cheap enough to refresh on every view open
//! without caching.

use std::path::Path;

use rusqlite::{Connection, Result as SqliteResult};

#[derive(Debug, Clone, Default)]
pub struct LibraryHealth {
    pub total_photos: i64,
    /// Photos whose `thumbnail_path` is set in DB but the file is
    /// missing on disk, OR thumbnail_path is NULL.
    pub missing_thumbnails: i64,
    /// `date_taken_source IN ('mtime', 'file_meta')` — fallback
    /// chain used; the year may be off.
    pub inaccurate_dates: i64,
    /// `date_taken IS NULL` — couldn't extract a date at all.
    pub missing_dates: i64,
    /// Total `.heic` / `.heif` photos in the library.
    pub heic_count: i64,
    /// True when this binary was compiled with the `heic` Cargo
    /// feature (libheif). When false and `heic_count > 0`, the user
    /// has photos PhotoVault can't decode.
    pub heic_decoder_available: bool,
    /// Photos marked `faces_processed = TRUE` but have zero rows in
    /// `faces`. Heuristic: face pipeline finished but found nothing
    /// (could be a bug or genuinely face-less landscape photos).
    pub face_processed_no_faces: i64,
}

/// Compute all library-health counters in one pass. Designed to run
/// inside `spawn_blocking`.
pub fn compute(conn: &Connection, drive_root: &Path) -> SqliteResult<LibraryHealth> {
    let total_photos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
        [],
        |r| r.get(0),
    )?;

    let inaccurate_dates: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photos
         WHERE is_trashed = FALSE
           AND date_taken_source IN ('mtime', 'file_meta')",
        [],
        |r| r.get(0),
    )?;

    let missing_dates: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE AND date_taken IS NULL",
        [],
        |r| r.get(0),
    )?;

    let heic_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photos
         WHERE is_trashed = FALSE
           AND (LOWER(file_path) LIKE '%.heic' OR LOWER(file_path) LIKE '%.heif')",
        [],
        |r| r.get(0),
    )?;

    let face_processed_no_faces: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photos p
         WHERE is_trashed = FALSE
           AND faces_processed = TRUE
           AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.photo_id = p.id)",
        [],
        |r| r.get(0),
    )?;

    // Missing-thumbnail count: NULL thumbnail_path counts directly;
    // for non-NULL paths we have to stat each file. To bound the
    // cost we sample only photos whose thumbnail_path is non-NULL —
    // a thumbnail file gone missing is unusual (manual delete /
    // disk corruption), so we don't care about strict accuracy.
    let null_thumbs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photos
         WHERE is_trashed = FALSE AND thumbnail_path IS NULL",
        [],
        |r| r.get(0),
    )?;
    let mut stat_misses = 0i64;
    {
        let mut stmt = conn.prepare(
            "SELECT thumbnail_path FROM photos
             WHERE is_trashed = FALSE AND thumbnail_path IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for row in rows.flatten() {
            let p = drive_root.join(&row);
            if !p.exists() {
                stat_misses += 1;
            }
        }
    }

    Ok(LibraryHealth {
        total_photos,
        missing_thumbnails: null_thumbs + stat_misses,
        inaccurate_dates,
        missing_dates,
        heic_count,
        heic_decoder_available: cfg!(feature = "heic"),
        face_processed_no_faces,
    })
}
