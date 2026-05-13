//! Shared helper for upgrading cover/hero thumbnails to Medium size.
//!
//! Cards on the Albums and Memories surfaces stretch a single photo
//! across hundreds of pixels of screen real estate. The Small thumbnail
//! the scanner writes (260 px) gets noticeably soft at that scale; the
//! Medium variant (430 px) lands at native cover density without
//! becoming bloated. We generate it on demand the first time a cover
//! is requested and reuse it from cache thereafter.

use std::path::PathBuf;
use std::sync::Arc;

use smriti::services::path_util::safe_join_relative;
use smriti::services::thumbnail::{ThumbnailService, ThumbnailSize};

/// One row of input — fields needed to generate (or look up) a
/// medium-size thumbnail for a single cover/hero photo.
///
///   (caller_idx, file_path_relative, file_hash, orientation)
///
/// `caller_idx` lets the caller scatter results back into its own
/// list — albums and memories both index into a parallel `Vec` of
/// DTOs and need to know which row to update.
pub type CoverInput = (usize, String, String, i32);

/// For each input, return the relative path to its Medium-size
/// thumbnail. If the file doesn't already exist on disk, generate it
/// synchronously (capped by the ThumbnailService's internal limiter).
/// Failures fall through to None — the frontend then falls back to
/// the Small variant that's already in the DTO.
pub async fn upgrade_covers_to_medium(
    svc: Arc<ThumbnailService>,
    drive_root: PathBuf,
    inputs: Vec<CoverInput>,
) -> Vec<(usize, Option<String>)> {
    if inputs.is_empty() {
        return Vec::new();
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut out: Vec<(usize, Option<String>)> = Vec::with_capacity(inputs.len());
        for (idx, file_path, file_hash, orientation) in inputs {
            let subdir = &file_hash[..2.min(file_hash.len())];
            let rel = format!(
                ".photovault/thumbnails/medium/v2/{}/{}.jpg",
                subdir, file_hash
            );
            let Ok(abs_thumb) = safe_join_relative(&drive_root, &rel) else {
                out.push((idx, None));
                continue;
            };
            if abs_thumb.exists() {
                out.push((idx, Some(rel)));
                continue;
            }
            let Ok(abs_src) = safe_join_relative(&drive_root, &file_path) else {
                out.push((idx, None));
                continue;
            };
            match svc.generate_thumbnail(&abs_src, &file_hash, orientation, ThumbnailSize::Medium) {
                Ok(_) => out.push((idx, Some(rel))),
                Err(e) => {
                    tracing::debug!(
                        "cover medium thumb generate failed for {}: {}",
                        file_path,
                        e
                    );
                    out.push((idx, None));
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default()
}
