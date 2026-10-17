//! Shared helper for upgrading cover/hero thumbnails to Medium size.
//!
//! Cards on the Albums and Memories surfaces stretch a single photo
//! across hundreds of pixels of screen real estate. The Small thumbnail
//! the scanner writes (260 px) gets noticeably soft at that scale; the
//! Medium variant (430 px) lands at native cover density without
//! becoming bloated. We generate it on demand the first time a cover
//! is requested and reuse it from cache thereafter.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

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

/// For each input whose Medium thumbnail is already on disk, its relative
/// path. Missing ones are generated in the background and are not waited
/// for: the caller keeps the Small variant already in its DTO, and the next
/// visit gets the sharper one. Waiting here used to hold up Albums and
/// Memories while originals were decoded one after another.
pub async fn upgrade_covers_to_medium(
    svc: Arc<ThumbnailService>,
    drive_root: PathBuf,
    inputs: Vec<CoverInput>,
) -> Vec<(usize, Option<String>)> {
    if inputs.is_empty() {
        return Vec::new();
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut ready = Vec::with_capacity(inputs.len());
        let mut missing = Vec::new();
        for (idx, file_path, file_hash, orientation) in inputs {
            let subdir = &file_hash[..2.min(file_hash.len())];
            let rel = format!(".photovault/thumbnails/medium/v2/{subdir}/{file_hash}.jpg");
            let Ok(abs_thumb) = safe_join_relative(&drive_root, &rel) else {
                continue;
            };
            if abs_thumb.exists() {
                ready.push((idx, Some(rel)));
            } else {
                missing.push((file_path, file_hash, orientation));
            }
        }
        if !missing.is_empty() {
            generate_in_background(svc, drive_root, missing);
        }
        ready
    })
    .await
    .unwrap_or_default()
}

/// Hashes being generated right now, so repeat visits don't queue the same
/// work twice.
static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

fn generate_in_background(
    svc: Arc<ThumbnailService>,
    drive_root: PathBuf,
    missing: Vec<(String, String, i32)>,
) {
    let missing: Vec<_> = {
        let mut in_flight = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        missing
            .into_iter()
            .filter(|(_, hash, _)| in_flight.insert(hash.clone()))
            .collect()
    };
    if missing.is_empty() {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        for (file_path, file_hash, orientation) in missing {
            if let Ok(abs_src) = safe_join_relative(&drive_root, &file_path) {
                if let Err(e) =
                    svc.generate_thumbnail(&abs_src, &file_hash, orientation, ThumbnailSize::Medium)
                {
                    tracing::debug!("cover medium thumb generate failed for {file_path}: {e}");
                }
            }
            IN_FLIGHT
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&file_hash);
        }
    });
}
