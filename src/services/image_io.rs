//! Format-aware image opening.
//!
//! Wraps `image::open` so:
//!   - HEIC/HEIF (every modern iPhone photo) decodes through libheif
//!     when the `heic` Cargo feature is enabled, fails clearly
//!     otherwise.
//!   - TIFF-based RAW (NEF / CR2 / CR3 / ARW / DNG / ORF / RW2 /
//!     PEF / RWL / SRW) decodes via the camera's embedded full-res
//!     JPEG preview when the `raw` feature is on (default).
//!   - Everything else falls through to the `image` crate.
//!
//! Call this from any code path that previously did `image::open(...)`
//! — thumbnail generation, face detection, brightness sampling.

use std::path::Path;

use image::DynamicImage;

/// Open and decode an image. Returns a fully-decoded `DynamicImage`
/// regardless of the on-disk format. HEIC/HEIF route through libheif
/// when compiled in; RAW files route through `raw_preview` to extract
/// the embedded full-res JPEG preview (the camera's intended
/// rendering, much better than software-debayered output without
/// proprietary colour profiles); everything else uses the `image`
/// crate's native decoders.
pub fn open_image(path: &Path) -> Result<DynamicImage, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("heic") | Some("heif") => decode_heic(path),
        Some(e) if is_raw_extension(e) => decode_raw_via_preview(path),
        _ => image::open(path).map_err(|e| e.to_string()),
    }
}

/// Recognises every TIFF-based RAW we route through the embedded-JPEG
/// path. Kept in sync with `scanner::SUPPORTED_EXTENSIONS` and
/// `reindexer`'s extension allowlist — adding a new RAW extension
/// means touching this list too.
fn is_raw_extension(e: &str) -> bool {
    matches!(
        e,
        "nef" | "cr2" | "cr3" | "arw" | "dng" | "orf" | "rw2" | "pef" | "rwl" | "srw" | "raf"
    )
}

#[cfg(feature = "raw")]
fn decode_raw_via_preview(path: &Path) -> Result<DynamicImage, String> {
    let jpeg_bytes = crate::services::raw_preview::extract_largest_preview(path)
        .map_err(|e| format!("RAW preview lookup failed for {}: {}", path.display(), e))?
        .ok_or_else(|| {
            format!(
                "RAW has no embedded JPEG preview: {}. Full-RAW Bayer decode is not supported in this build.",
                path.display()
            )
        })?;
    image::load_from_memory(&jpeg_bytes).map_err(|e| {
        format!(
            "embedded JPEG preview in {} is invalid: {}",
            path.display(),
            e
        )
    })
}

#[cfg(not(feature = "raw"))]
fn decode_raw_via_preview(path: &Path) -> Result<DynamicImage, String> {
    Err(format!(
        "RAW support not compiled in (rebuild with --features raw to decode {})",
        path.display()
    ))
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
