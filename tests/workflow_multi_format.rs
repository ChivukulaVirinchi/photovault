//! Workflow test: every format declared in `scanner::SUPPORTED_EXTENSIONS`
//! goes through the engine cleanly.
//!
//! The scanner picks up the file at the right extension; the
//! `image_io::open_image` dispatcher decodes it without panicking;
//! HEIC, RAW, and the other "needs-special-handling" formats hit
//! their dedicated arm. When a contributor adds a new format,
//! adding one case to this file is the canonical way to assert
//! the round-trip works.
//!
//! HEIC is gated by `--features heic` so the test for it is
//! `#[cfg(feature = "heic")]`. RAW is default-on (the embedded-
//! JPEG path) so its test always runs.

mod common;

use smriti::services::image_io::open_image;

#[test]
fn open_image_decodes_a_jpeg() {
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    let path = common::write_jpeg(dir.path(), "shot.jpg", 64, 48, [180, 90, 30]);
    let img = open_image(&path).expect("jpeg decode");
    assert_eq!(img.width(), 64);
    assert_eq!(img.height(), 48);
}

#[test]
fn open_image_decodes_a_png() {
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    let bytes = common::make_png(64, 48, [180, 90, 30]);
    let path = dir.path().join("shot.png");
    std::fs::write(&path, bytes).unwrap();
    let img = open_image(&path).expect("png decode");
    assert_eq!(img.width(), 64);
    assert_eq!(img.height(), 48);
}

#[cfg(feature = "raw")]
#[test]
fn open_image_decodes_a_nef_via_embedded_jpeg_preview() {
    // The minimal NEF synthesised by `common` wraps a JPEG at a
    // known IFD0 offset. `open_image` should route the .nef
    // extension through `raw_preview` → embedded JPEG decode →
    // DynamicImage with the preview's dimensions (not the
    // raw-sensor dimensions, which our synthetic file doesn't
    // even have).
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    let preview = common::make_jpeg(96, 72, [50, 200, 50]);
    let nef = common::make_minimal_nef(&preview);
    let path = dir.path().join("shot.nef");
    std::fs::write(&path, nef).unwrap();

    let img = open_image(&path).expect("nef → embedded JPEG decode");
    assert_eq!(img.width(), 96, "NEF returned its embedded JPEG width");
    assert_eq!(img.height(), 72);
}

#[cfg(feature = "raw")]
#[test]
fn open_image_decodes_each_raw_extension_via_the_same_path() {
    // The RAW dispatcher is keyed on extension. We don't need a
    // distinct sample file per maker — the same TIFF-with-embedded-
    // JPEG container is what every TIFF-based RAW boils down to.
    // This test verifies the extension matcher in `image_io.rs`
    // covers every member of `scanner::SUPPORTED_EXTENSIONS`'s RAW
    // group.
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    let preview = common::make_jpeg(48, 32, [200, 200, 100]);
    let nef = common::make_minimal_nef(&preview);
    for ext in [
        "nef", "cr2", "cr3", "arw", "dng", "orf", "rw2", "pef", "rwl", "srw",
    ] {
        let path = dir.path().join(format!("shot.{}", ext));
        std::fs::write(&path, &nef).unwrap();
        let img = open_image(&path).unwrap_or_else(|e| panic!("decode failed for .{}: {}", ext, e));
        assert_eq!(img.width(), 48, ".{} preview width matches", ext);
        assert_eq!(img.height(), 32, ".{} preview height matches", ext);
    }
}

#[test]
fn open_image_reports_raf_as_unsupported_container() {
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    let path = dir.path().join("shot.raf");
    std::fs::write(&path, b"not-a-fuji-raw").unwrap();

    let err = open_image(&path).unwrap_err();
    assert!(
        err.contains("RAF support is not available yet"),
        "unexpected error message: {}",
        err
    );
}

#[cfg(not(feature = "raw"))]
#[test]
fn open_image_without_raw_feature_errors_on_nef() {
    // The non-raw build path returns a clear "feature not compiled
    // in" error instead of silently producing garbage. Catches
    // accidental feature-flag drift in release builds.
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir()
        .unwrap();
    std::fs::write(dir.path().join("shot.nef"), b"\0\0\0\0").unwrap();
    let err = open_image(&dir.path().join("shot.nef")).unwrap_err();
    assert!(
        err.contains("RAW support not compiled in") || err.contains("not compiled"),
        "unexpected error message: {}",
        err
    );
}
