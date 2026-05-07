//! Integration tests for thumbnail generation pipeline.
//!
//! Creates test images, generates thumbnails at each size,
//! and verifies output quality and dimensions.

use image::{DynamicImage, ImageBuffer, Rgb};
use smriti::services::thumbnail::{ThumbnailService, ThumbnailSize};
use std::path::Path;
use tempfile::tempdir;

fn create_test_jpeg(path: &Path, width: u32, height: u32) {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        // Create a gradient pattern so thumbnails have actual content
        Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    let dynamic = DynamicImage::ImageRgb8(img);
    dynamic.save(path).unwrap();
}

#[test]
fn test_thumbnail_generation_all_sizes() {
    let temp = tempdir().unwrap();
    let drive_root = temp.path();

    // Create a test image
    let image_path = drive_root.join("test_photo.jpg");
    create_test_jpeg(&image_path, 4000, 3000);

    let service = ThumbnailService::new(drive_root, 1.0).unwrap();

    for size in [
        ThumbnailSize::Small,
        ThumbnailSize::Medium,
        ThumbnailSize::Large,
    ] {
        let result = service.generate_thumbnail(&image_path, "testhash123", 1, size);
        assert!(
            result.is_ok(),
            "Generation failed for {:?}: {:?}",
            size,
            result.err()
        );

        let thumb_path = result.unwrap();
        assert!(
            thumb_path.exists(),
            "Thumbnail file should exist for {:?}",
            size
        );

        // Check file size is reasonable (>5KB for a real thumbnail)
        let file_size = std::fs::metadata(&thumb_path).unwrap().len();
        assert!(
            file_size > 5000,
            "Thumbnail {:?} too small: {} bytes",
            size,
            file_size
        );

        // Decode and check dimensions
        let thumb_img = image::open(&thumb_path).unwrap();
        let (w, h) = (thumb_img.width(), thumb_img.height());
        let max_dim = size.pixels();
        assert!(
            w <= max_dim && h <= max_dim,
            "Thumbnail {:?} exceeds max dim {}: {}x{}",
            size,
            max_dim,
            w,
            h
        );
        assert!(
            w >= max_dim / 2 || h >= max_dim / 2,
            "Thumbnail {:?} too small: {}x{} (expected at least {} on one side)",
            size,
            w,
            h,
            max_dim / 2
        );
    }
}

#[test]
fn test_thumbnail_caching() {
    let temp = tempdir().unwrap();
    let image_path = temp.path().join("photo.jpg");
    create_test_jpeg(&image_path, 2000, 1500);

    let service = ThumbnailService::new(temp.path(), 1.0).unwrap();

    // Generate first time
    let path1 = service
        .generate_thumbnail(&image_path, "cachehash", 1, ThumbnailSize::Small)
        .unwrap();
    let mtime1 = std::fs::metadata(&path1).unwrap().modified().unwrap();

    // Generate again - should return cached (same path, not regenerated)
    let path2 = service
        .generate_thumbnail(&image_path, "cachehash", 1, ThumbnailSize::Small)
        .unwrap();
    let mtime2 = std::fs::metadata(&path2).unwrap().modified().unwrap();

    assert_eq!(path1, path2);
    assert_eq!(mtime1, mtime2, "Cached thumbnail should not be regenerated");
}

#[test]
fn test_thumbnail_path_versioning() {
    let temp = tempdir().unwrap();
    let service = ThumbnailService::new(temp.path(), 1.0).unwrap();

    // Create thumbnail directory structure to verify v2 path
    let image_path = temp.path().join("photo.jpg");
    create_test_jpeg(&image_path, 800, 600);

    let result = service
        .generate_thumbnail(&image_path, "versionhash", 1, ThumbnailSize::Small)
        .unwrap();
    let path_str = result.to_string_lossy().to_string();

    // Should contain "v2" in path
    assert!(
        path_str.contains("/v2/") || path_str.contains("\\v2\\"),
        "Thumbnail path should include v2 version: {}",
        path_str
    );
}

#[test]
fn test_thumbnail_sizes_correct() {
    assert_eq!(ThumbnailSize::Small.pixels(), 260);
    assert_eq!(ThumbnailSize::Medium.pixels(), 430);
    assert_eq!(ThumbnailSize::Large.pixels(), 860);
}

#[test]
fn test_thumbnail_with_small_image() {
    let temp = tempdir().unwrap();
    let image_path = temp.path().join("small.jpg");
    // Image smaller than thumbnail size
    create_test_jpeg(&image_path, 100, 80);

    let service = ThumbnailService::new(temp.path(), 1.0).unwrap();
    let result = service.generate_thumbnail(&image_path, "smallhash", 1, ThumbnailSize::Small);

    // Should still succeed (upscale or keep original size)
    assert!(result.is_ok());
    let thumb_path = result.unwrap();
    assert!(thumb_path.exists());
}

#[test]
fn test_corrupt_file_handling() {
    let temp = tempdir().unwrap();
    let bad_path = temp.path().join("corrupt.jpg");
    std::fs::write(&bad_path, b"not a real jpeg file").unwrap();

    let service = ThumbnailService::new(temp.path(), 1.0).unwrap();
    let result = service.generate_thumbnail(&bad_path, "badhash", 1, ThumbnailSize::Small);

    // Should return an error, not panic
    assert!(result.is_err());
}

#[test]
fn test_nonexistent_file_handling() {
    let temp = tempdir().unwrap();
    let missing_path = temp.path().join("does_not_exist.jpg");

    let service = ThumbnailService::new(temp.path(), 1.0).unwrap();
    let result = service.generate_thumbnail(&missing_path, "missinghash", 1, ThumbnailSize::Small);

    // Should return an error, not panic
    assert!(result.is_err());
}

#[test]
fn test_load_existing_thumbnails() {
    let temp = tempdir().unwrap();
    let service = ThumbnailService::new(temp.path(), 1.0).unwrap();

    // Generate a thumbnail
    let image_path = temp.path().join("photo.jpg");
    create_test_jpeg(&image_path, 1000, 750);
    service
        .generate_thumbnail(&image_path, "existhash", 1, ThumbnailSize::Small)
        .unwrap();

    // Create a new service and load existing thumbnails
    let service2 = ThumbnailService::new(temp.path(), 1.0).unwrap();
    service2.load_existing_thumbnails().unwrap();

    // The cached thumbnail should be found without regeneration
    let result = service2
        .generate_thumbnail(&image_path, "existhash", 1, ThumbnailSize::Small)
        .unwrap();
    assert!(result.exists());
}
