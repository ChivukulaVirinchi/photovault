//! Format-aware image opening.
//!
//! Wraps `image::open` so HEIC/HEIF files (every modern iPhone photo)
//! decode through `libheif` when the `heic` Cargo feature is enabled,
//! and fail with a clear message otherwise.
//!
//! Call this from any code path that previously did `image::open(...)`
//! — thumbnail generation, face detection, brightness sampling.

use std::path::Path;

use image::DynamicImage;

/// Open and decode an image. Returns a fully-decoded `DynamicImage`
/// regardless of the on-disk format. HEIC/HEIF are routed through
/// libheif when compiled in; everything else uses the `image` crate.
pub fn open_image(path: &Path) -> Result<DynamicImage, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("heic") | Some("heif") => decode_heic(path),
        _ => image::open(path).map_err(|e| e.to_string()),
    }
}

#[cfg(feature = "heic")]
fn decode_heic(path: &Path) -> Result<DynamicImage, String> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_file(
        path.to_str()
            .ok_or_else(|| format!("non-UTF8 HEIC path: {}", path.display()))?,
    )
    .map_err(|e| format!("HeifContext::read_from_file failed: {}", e))?;
    let handle = ctx
        .primary_image_handle()
        .map_err(|e| format!("HEIC primary_image_handle: {}", e))?;
    let img = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None)
        .map_err(|e| format!("HEIC decode: {}", e))?;

    let planes = img.planes();
    let plane = planes
        .interleaved
        .ok_or_else(|| "HEIC: missing interleaved RGB plane".to_string())?;
    let (w, h) = (plane.width as u32, plane.height as u32);
    let stride = plane.stride;
    let mut buf = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for y in 0..(h as usize) {
        let row_start = y * stride;
        let row_end = row_start + (w as usize) * 3;
        buf.extend_from_slice(&plane.data[row_start..row_end]);
    }
    image::RgbImage::from_raw(w, h, buf)
        .map(DynamicImage::ImageRgb8)
        .ok_or_else(|| "HEIC: RGB buffer size mismatch".to_string())
}

#[cfg(not(feature = "heic"))]
fn decode_heic(path: &Path) -> Result<DynamicImage, String> {
    Err(format!(
        "HEIC support not compiled in (rebuild with --features heic to decode {})",
        path.display()
    ))
}
