//! Workflow test: the engine doesn't fall over on weird inputs.
//!
//! Library scans encounter messy filesystems: corrupt files,
//! zero-byte files, symlinks, files with no extension, files with
//! upper-case extensions, the works. The pipeline must keep
//! processing remaining photos rather than aborting the whole
//! scan. These tests pin that "keep going on error" behaviour.

mod common;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::sync::Mutex;

use smriti::db::Database;
use smriti::services::image_io::open_image;
use smriti::services::scanner;

async fn run_scan_and_wait(
    drive_root: std::path::PathBuf,
    db: Arc<Mutex<Database>>,
) -> scanner::ScanReport {
    let cancel = Arc::new(AtomicBool::new(false));
    let (rx, handle) = scanner::start_scan(drive_root, db, cancel, false);
    tokio::spawn(async move {
        while rx.recv().await.is_ok() {}
    });
    handle.await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_continues_past_corrupt_files() {
    let (dir, db_raw) = common::make_library();
    let db = Arc::new(Mutex::new(db_raw));

    // One good photo, one "corrupt" file with a JPEG extension but
    // garbage contents. The scanner doesn't decode at scan time
    // (decode happens later in the thumbnail pass), so the corrupt
    // file still gets indexed — the scanner's job is just to find
    // candidates. The downstream decode is what would fail, and
    // we test that failure mode separately in
    // `decoder_returns_error_for_corrupt_bytes`.
    common::write_jpeg(dir.path(), "good.jpg", 256, 256, [10, 20, 30]);
    std::fs::write(dir.path().join("corrupt.jpg"), vec![0u8; 32 * 1024]).unwrap();

    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(
        report.files_inserted, 2,
        "both files get rows; decode failure happens later"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_handles_uppercase_extensions() {
    // Real-world cameras emit IMG_XXXX.JPG, IMG_XXXX.NEF, etc.
    // The scanner's extension comparison must be case-insensitive.
    let (dir, db_raw) = common::make_library();
    let db = Arc::new(Mutex::new(db_raw));
    common::write_jpeg(dir.path(), "IMG_0001.JPG", 256, 256, [50, 50, 50]);
    common::write_jpeg(dir.path(), "IMG_0002.Jpeg", 256, 256, [60, 60, 60]);

    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    assert_eq!(
        report.files_inserted, 2,
        "uppercase + mixed-case extensions both recognised"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_skips_files_without_extension() {
    // No extension = no way to know it's an image without
    // sniffing magic bytes. Smriti chose the extension-only path
    // for speed; this test pins that decision.
    let (dir, db_raw) = common::make_library();
    let db = Arc::new(Mutex::new(db_raw));
    common::write_jpeg(dir.path(), "ok.jpg", 256, 256, [70, 70, 70]);
    std::fs::write(dir.path().join("NOEXT"), vec![0xFF; 32 * 1024]).unwrap();

    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    assert_eq!(
        report.files_inserted, 1,
        "extensionless files are skipped"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_skips_files_below_minimum_size() {
    // The 10 KB minimum guards against indexing icons, sprites,
    // and email-attachment thumbnails. We don't want a folder of
    // 4×4 favicons to balloon the library.
    let (dir, db_raw) = common::make_library();
    let db = Arc::new(Mutex::new(db_raw));
    // Tiny noisy JPEG — even with noise, an 8×8 image compresses
    // to under 1 KB. Well below the 10 KB floor.
    let tiny = common::make_jpeg(8, 8, [120, 50, 80]);
    assert!(tiny.len() < 10 * 1024, "test premise: tiny.jpg under floor");
    std::fs::write(dir.path().join("tiny.jpg"), tiny).unwrap();
    // A medium-coloured 256×256 noisy JPEG comfortably clears the
    // 10 KB floor (≈30-60 KB).
    common::write_jpeg(dir.path(), "big.jpg", 256, 256, [120, 50, 80]);

    let report = run_scan_and_wait(dir.path().to_path_buf(), db.clone()).await;
    assert_eq!(
        report.files_inserted, 1,
        "files under 10 KB are silently skipped"
    );
}

#[test]
fn decoder_returns_error_for_corrupt_bytes() {
    // Direct test of `image_io::open_image` on garbage. The
    // function must return Err, not panic. The thumbnail /
    // face-detection pipelines rely on this contract — they
    // surface decode errors as "couldn't load photo" to the UI.
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    let path = dir.path().join("garbage.jpg");
    std::fs::write(&path, vec![0u8; 1024]).unwrap();
    let result = open_image(&path);
    assert!(result.is_err(), "garbage JPEG should not decode");
}

#[test]
fn decoder_returns_error_for_missing_file() {
    let path = std::path::Path::new("/non/existent/path/to/IMG.jpg");
    let result = open_image(path);
    assert!(result.is_err(), "missing file should produce an Err");
}
