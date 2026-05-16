//! Workflow test: scanning a directory of photos produces correctly
//! populated rows in the `photos` table.
//!
//! This is the foundational user journey: "I point Smriti at my
//! photos folder, click Scan, photos appear in Timeline". Breaking
//! it breaks everything downstream — thumbnails, EXIF, faces,
//! search, the lot. Worth its own end-to-end test.
//!
//! Each test:
//! 1. Builds a tempdir with synthetic photos (no real images shipped
//!    in the repo).
//! 2. Opens a Database against the tempdir and runs the streaming
//!    scanner.
//! 3. Awaits the scan task to completion.
//! 4. Asserts on the resulting rows.

mod common;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use smriti::db::photo_repo::PhotoRepo;
use smriti::services::scanner;

/// Build a tempdir + open a Database against it. Returns the dir
/// (held alive by the caller) and the Database wrapped in
/// Arc<Mutex<>> ready to hand to the scanner.
fn make_scannable_library() -> (TempDir, Arc<Mutex<smriti::db::Database>>) {
    let (dir, db) = common::make_library();
    (dir, Arc::new(Mutex::new(db)))
}

/// Run the scanner and collect the final report. The caller must
/// supply the drive root and the same Database the library was
/// opened against. Cancel flag is held false for the duration.
async fn run_scan_and_wait(
    drive_root: std::path::PathBuf,
    db: Arc<Mutex<smriti::db::Database>>,
) -> scanner::ScanReport {
    let cancel = Arc::new(AtomicBool::new(false));
    let (rx, handle) = scanner::start_scan(drive_root, db, cancel, /* hidden = */ false);
    // Drain progress messages so the bounded channel doesn't back
    // up and stall the scanner — we don't assert on individual
    // events here, just need the final report.
    tokio::spawn(async move {
        while rx.recv().await.is_ok() {}
    });
    handle.await.expect("scan task join")
}

// ---------- basic correctness ----------

#[tokio::test(flavor = "multi_thread")]
async fn scan_inserts_one_row_per_image() {
    // Arrange: three JPEGs side-by-side on disk.
    let (dir, db) = make_scannable_library();
    // 256×256 with noise is comfortably above the scanner's 10 KB
    // minimum-file-size floor (≈30-60 KB on disk).
    common::write_jpeg(dir.path(), "alpha.jpg", 256, 256, [255, 0, 0]);
    common::write_jpeg(dir.path(), "beta.jpg", 256, 256, [0, 255, 0]);
    common::write_jpeg(dir.path(), "gamma.jpg", 256, 256, [0, 0, 255]);

    // Act: scan.
    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;

    // Assert: three rows, no errors.
    assert_eq!(report.errors, Vec::<String>::new(), "scan should not produce errors");
    assert_eq!(report.files_inserted, 3, "one row per JPEG");

    let db_guard = db.lock().await;
    let repo = PhotoRepo::new(&db_guard.conn);
    let count = repo.count().unwrap();
    assert_eq!(count, 3, "DB row count matches files-on-disk count");
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_walks_into_subdirectories() {
    // Arrange: photos nested two levels deep.
    let (dir, db) = make_scannable_library();
    let sub = dir.path().join("2024").join("january");
    std::fs::create_dir_all(&sub).unwrap();
    common::write_jpeg(&sub, "IMG_0001.jpg", 256, 256, [120, 120, 120]);
    common::write_jpeg(&sub, "IMG_0002.jpg", 256, 256, [200, 200, 200]);

    // Act.
    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;

    // Assert: both nested photos picked up.
    assert_eq!(report.files_inserted, 2);

    let db_guard = db.lock().await;
    let repo = PhotoRepo::new(&db_guard.conn);
    let count = repo.count().unwrap();
    assert_eq!(count, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_skips_unsupported_extensions() {
    // Arrange: mix of supported and unsupported file types.
    let (dir, db) = make_scannable_library();
    common::write_jpeg(dir.path(), "photo.jpg", 256, 256, [100, 100, 100]);
    std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
    std::fs::write(dir.path().join("video.mp4"), b"\0\0\0\0").unwrap();
    std::fs::write(dir.path().join("archive.zip"), b"PK\x03\x04").unwrap();

    // Act.
    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;

    // Assert: only the JPEG made it in.
    assert_eq!(report.files_inserted, 1, "scanner ignored non-image files");
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_skips_photovault_metadata_dir() {
    // Arrange: a JPEG inside `.photovault/` — the on-drive metadata
    // dir — must NOT be re-indexed even though it has a valid
    // extension. Otherwise the thumbnail cache would re-enter the
    // scan as new photos on every run.
    let (dir, db) = make_scannable_library();
    common::write_jpeg(dir.path(), "real.jpg", 256, 256, [1, 2, 3]);
    let meta = dir.path().join(".photovault").join("thumbnails");
    std::fs::create_dir_all(&meta).unwrap();
    common::write_jpeg(&meta, "thumb.jpg", 256, 256, [9, 9, 9]);

    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    assert_eq!(
        report.files_inserted, 1,
        "the .photovault/ metadata dir must be skipped"
    );
}

// ---------- multi-format coverage ----------

#[tokio::test(flavor = "multi_thread")]
async fn scan_picks_up_multi_format_library() {
    // Arrange: a library mixing JPEG, PNG, and a synthetic NEF
    // (RAW). Each must be recognised as a photo by the scanner.
    // The actual decode happens later in the thumbnail pass; here
    // we only verify the scanner ROW gets created.
    let (dir, db) = make_scannable_library();
    common::write_jpeg(dir.path(), "shot.jpg", 256, 256, [200, 50, 50]);
    let png_bytes = common::make_png(256, 256, [50, 200, 50]);
    std::fs::write(dir.path().join("shot.png"), png_bytes).unwrap();
    // NEF wraps a 256×256 JPEG via the helper.
    let preview = common::make_jpeg(256, 256, [50, 50, 200]);
    let nef = common::make_minimal_nef(&preview);
    std::fs::write(dir.path().join("shot.nef"), nef).unwrap();

    // Act.
    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(report.files_inserted, 3, "JPEG + PNG + NEF all indexed");

    // Assert: each row has the correct file_name + extension preserved.
    let db_guard = db.lock().await;
    let mut stmt = db_guard
        .conn
        .prepare("SELECT file_name FROM photos ORDER BY file_name")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(names, vec!["shot.jpg", "shot.nef", "shot.png"]);
}

// ---------- re-scan idempotency ----------

#[tokio::test(flavor = "multi_thread")]
async fn re_scan_does_not_create_duplicate_rows() {
    // Arrange: scan once, then scan again with no changes.
    let (dir, db) = make_scannable_library();
    common::write_jpeg(dir.path(), "stable.jpg", 256, 256, [100, 100, 100]);
    let _first = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    let second = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;

    // Assert: the second scan touches the same row, doesn't create
    // a new one. Behaviour relies on `INSERT OR IGNORE` keyed on
    // `file_path`; regression here would explode library size on
    // every reopen.
    let db_guard = db.lock().await;
    let repo = PhotoRepo::new(&db_guard.conn);
    assert_eq!(repo.count().unwrap(), 1);
    assert!(
        second.errors.is_empty(),
        "second scan should be clean: {:?}",
        second.errors
    );
}
