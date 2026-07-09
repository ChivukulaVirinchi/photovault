//! Thumbnail generation and caching service
//!
//! Follows a modified freedesktop.org thumbnail spec:
//! - Thumbnails stored on the drive itself for portability
//! - Named by SHA256 hash of file path
//! - Stored as JPEG for smaller size
//! - Three sizes: 128, 256, 512
//!
//! Performance strategy (matches production photo apps):
//! 1. Extract embedded EXIF thumbnail first (~2ms vs ~200-500ms for full decode)
//! 2. Fall back to bounded full decode when no adequate embedded preview exists
//! 3. Per-image timeout to prevent stuck queue from corrupt/huge images
//! 4. Priority-aware concurrency so visible cells beat background prewarm

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};
use std::{fs::File, io::BufReader};

use exif::{In, Reader as ExifReader, Tag};
use lru::LruCache;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader};

use crate::services::image_utils::apply_exif_orientation;

/// Maximum time allowed for a single thumbnail generation before giving up.
const THUMBNAIL_TIMEOUT: Duration = Duration::from_secs(10);

/// Thumbnail size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    Small,  // 300x300 (timeline grid)
    Medium, // 500x500 (detail preview)
    Large,  // 1000x1000 (high quality / viewer)
}

impl ThumbnailSize {
    pub fn pixels(&self) -> u32 {
        // ~15% smaller than the previous 300/500/1000 defaults. The quadratic
        // pixel reduction (~30%) translates to similar CPU/IO savings per
        // thumbnail with negligible visible-quality loss at typical grid sizes.
        match self {
            ThumbnailSize::Small => 260,
            ThumbnailSize::Medium => 430,
            ThumbnailSize::Large => 860,
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            ThumbnailSize::Small => "small",
            ThumbnailSize::Medium => "medium",
            ThumbnailSize::Large => "large",
        }
    }
}

/// Cached thumbnail entry.
///
/// Access recency is tracked by the enclosing `LruCache` — no explicit
/// `last_accessed` field. Previously the struct carried an `Instant`
/// that was set on insert but never bumped on access, so the "LRU"
/// eviction was effectively FIFO.
#[derive(Debug, Clone)]
pub struct ThumbnailEntry {
    pub path: PathBuf,
    pub file_size: u64,
}

/// Maximum number of entries the in-memory cache tracks. Serves as a
/// backstop against unbounded metadata growth — actual eviction
/// typically fires from the byte-budget check in `evict_if_needed`
/// well before the count limit is hit. 10k is plenty for any visible
/// scrolling window even on huge libraries.
const CACHE_ENTRY_CAPACITY: usize = 10_000;

/// Blocking concurrency limiter. Callers wait on a condvar until a
/// permit is free instead of failing fast. The previous `try_acquire`
/// returned an error when the queue was full — under prewarm load that
/// surfaced as "Too many concurrent thumbnail generations" for every
/// foreground IPC request, which the UI reported as a missing thumbnail.
struct ConcurrencyLimiter {
    max: usize,
    max_background: usize,
    state: Mutex<LimiterState>,
    cv: Condvar,
}

#[derive(Debug, Default)]
struct LimiterState {
    foreground_active: usize,
    background_active: usize,
    foreground_waiting: usize,
}

#[derive(Debug, Clone, Copy)]
enum GenerationPriority {
    Foreground,
    Background,
}

impl ConcurrencyLimiter {
    fn new(max: usize, max_background: usize) -> Self {
        Self {
            max,
            max_background,
            state: Mutex::new(LimiterState::default()),
            cv: Condvar::new(),
        }
    }

    /// Block until a permit is available, then claim it. Safe to call
    /// from a `spawn_blocking` worker — Tokio has hundreds of blocking
    /// threads and parking one is fine.
    fn acquire(self: &Arc<Self>, priority: GenerationPriority) -> ConcurrencyPermit {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(priority, GenerationPriority::Foreground) {
            state.foreground_waiting += 1;
        }

        loop {
            let active = state.foreground_active + state.background_active;
            let can_acquire = match priority {
                GenerationPriority::Foreground => active < self.max,
                GenerationPriority::Background => {
                    active < self.max
                        && state.background_active < self.max_background
                        && state.foreground_waiting == 0
                }
            };

            if can_acquire {
                match priority {
                    GenerationPriority::Foreground => {
                        state.foreground_waiting = state.foreground_waiting.saturating_sub(1);
                        state.foreground_active += 1;
                    }
                    GenerationPriority::Background => {
                        state.background_active += 1;
                    }
                }
                break;
            }

            state = self.cv.wait(state).unwrap_or_else(|e| e.into_inner());
        }

        ConcurrencyPermit {
            limiter: self.clone(),
            priority,
        }
    }

    /// Release a permit and wake one waiter.
    fn release(&self, priority: GenerationPriority) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            match priority {
                GenerationPriority::Foreground => {
                    state.foreground_active = state.foreground_active.saturating_sub(1);
                }
                GenerationPriority::Background => {
                    state.background_active = state.background_active.saturating_sub(1);
                }
            }
        }
        self.cv.notify_all();
    }
}

struct ConcurrencyPermit {
    limiter: Arc<ConcurrencyLimiter>,
    priority: GenerationPriority,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        self.limiter.release(self.priority);
    }
}

struct GenerationDeduper {
    active: Mutex<std::collections::HashSet<String>>,
    cv: Condvar,
}

impl GenerationDeduper {
    fn new() -> Self {
        Self {
            active: Mutex::new(std::collections::HashSet::new()),
            cv: Condvar::new(),
        }
    }

    fn enter(self: &Arc<Self>, key: String) -> GenerationGuard {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        while active.contains(&key) {
            active = self.cv.wait(active).unwrap_or_else(|e| e.into_inner());
        }
        active.insert(key.clone());
        GenerationGuard {
            deduper: self.clone(),
            key,
        }
    }

    fn leave(&self, key: &str) {
        {
            let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
            active.remove(key);
        }
        self.cv.notify_all();
    }
}

struct GenerationGuard {
    deduper: Arc<GenerationDeduper>,
    key: String,
}

impl Drop for GenerationGuard {
    fn drop(&mut self) {
        self.deduper.leave(&self.key);
    }
}

/// Thumbnail service with LRU cache management
pub struct ThumbnailService {
    /// Root path of the drive
    _drive_root: PathBuf,

    /// Thumbnail cache directory
    cache_dir: PathBuf,

    /// In-memory cache of thumbnail paths. Switched from
    /// `HashMap + last_accessed Instant` to `LruCache` so eviction is
    /// O(1) via `pop_lru()` instead of O(n) sort-by-timestamp, and so
    /// access recency is tracked correctly (the old field was set on
    /// insert and never updated).
    cache: Arc<RwLock<LruCache<(String, ThumbnailSize), ThumbnailEntry>>>,

    /// Maximum cache size in bytes
    max_cache_bytes: u64,

    /// Current cache size in bytes
    current_cache_bytes: Arc<RwLock<u64>>,

    /// Concurrency limiter for generation (std::sync for blocking context)
    generation_limiter: Arc<ConcurrencyLimiter>,

    /// Hash+size keys currently being generated. Duplicate requests wait
    /// for the first decode and then reuse its output from disk.
    generating: Arc<GenerationDeduper>,
}

impl ThumbnailService {
    /// Create a new thumbnail service for a drive
    pub fn new<P: AsRef<Path>>(drive_root: P, max_cache_gb: f64) -> std::io::Result<Self> {
        let drive_root = drive_root.as_ref().to_path_buf();
        let cache_dir = drive_root.join(".photovault").join("thumbnails");

        // Create cache directories for each size
        for size in [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
        ] {
            std::fs::create_dir_all(cache_dir.join(size.dir_name()))?;
        }

        let max_cache_bytes = (max_cache_gb * 1024.0 * 1024.0 * 1024.0) as u64;

        let capacity = NonZeroUsize::new(CACHE_ENTRY_CAPACITY)
            .expect("CACHE_ENTRY_CAPACITY is a non-zero compile-time constant");

        Ok(Self {
            _drive_root: drive_root,
            cache_dir,
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            max_cache_bytes,
            current_cache_bytes: Arc::new(RwLock::new(0)),
            // Cap concurrent decodes at 4. Each large JPEG holds ~50–80 MB
            // of decoded RGB while resizing — at 8-wide we saw OOM on
            // mid-spec laptops. 4 keeps the working set under ~320 MB.
            generation_limiter: Arc::new(ConcurrencyLimiter::new(4, 1)),
            generating: Arc::new(GenerationDeduper::new()),
        })
    }

    /// Prewarm Small thumbnails for a batch of photos. Skips any that
    /// already exist on disk (cheap stat). Runs in parallel via Rayon —
    /// the concurrency limiter inside `generate_thumbnail` caps the
    /// number of in-flight image decodes at 4 to avoid OOM.
    ///
    /// Aborts as soon as `cancel` is set so a fresh scan or app-exit
    /// can stop prewarm immediately. Returns the number of newly
    /// generated thumbnails.
    pub fn prewarm_small(
        &self,
        items: &[(PathBuf, String, i32)],
        cancel: &std::sync::atomic::AtomicBool,
    ) -> usize {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let size = ThumbnailSize::Small;
        let generated = AtomicUsize::new(0);
        items
            .par_iter()
            .for_each(|(photo_path, file_hash, orientation)| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                // Skip if a thumbnail file already exists on disk for this
                // hash+size — that's the same key the on-demand generator
                // uses, so the cost is one stat per photo.
                let cached = self.thumbnail_path(file_hash, size);
                if cached.exists() {
                    return;
                }
                match self.generate_thumbnail_background(photo_path, file_hash, *orientation, size)
                {
                    Ok(_) => {
                        generated.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => tracing::trace!("prewarm skipped {}: {}", photo_path.display(), e),
                }
            });
        generated.load(Ordering::Relaxed)
    }

    /// Generate a thumbnail synchronously with a timeout guard.
    ///
    /// Call this from a background task (spawn_blocking) after get_thumbnail returns Pending.
    pub fn generate_thumbnail(
        &self,
        photo_path: &Path,
        file_hash: &str,
        orientation: i32,
        size: ThumbnailSize,
    ) -> Result<PathBuf, String> {
        self.generate_thumbnail_with_priority(
            photo_path,
            file_hash,
            orientation,
            size,
            GenerationPriority::Foreground,
        )
    }

    pub fn generate_thumbnail_background(
        &self,
        photo_path: &Path,
        file_hash: &str,
        orientation: i32,
        size: ThumbnailSize,
    ) -> Result<PathBuf, String> {
        self.generate_thumbnail_with_priority(
            photo_path,
            file_hash,
            orientation,
            size,
            GenerationPriority::Background,
        )
    }

    fn generate_thumbnail_with_priority(
        &self,
        photo_path: &Path,
        file_hash: &str,
        orientation: i32,
        size: ThumbnailSize,
        priority: GenerationPriority,
    ) -> Result<PathBuf, String> {
        let thumb_path = self.thumbnail_path(file_hash, size);
        if self.try_existing_thumbnail(file_hash, size, &thumb_path) {
            return Ok(thumb_path);
        }

        let _permit = self.generation_limiter.acquire(priority);
        let key = format!("{}:{file_hash}", size.dir_name());
        let _generation = self.generating.enter(key);
        if self.try_existing_thumbnail(file_hash, size, &thumb_path) {
            return Ok(thumb_path);
        }

        let start = Instant::now();
        self.generate_thumbnail_inner(photo_path, file_hash, orientation, size, start)
    }

    /// Inner thumbnail generation (separated for clean permit release)
    fn generate_thumbnail_inner(
        &self,
        photo_path: &Path,
        file_hash: &str,
        orientation: i32,
        size: ThumbnailSize,
        start: Instant,
    ) -> Result<PathBuf, String> {
        // Check if existing thumbnail is adequate quality (not a tiny EXIF extract)
        let thumb_path = self.thumbnail_path(file_hash, size);
        if self.try_existing_thumbnail(file_hash, size, &thumb_path) {
            return Ok(thumb_path);
        }

        // Otherwise, regenerate it by falling through.
        if thumb_path.exists() {
            let _ = std::fs::remove_file(&thumb_path);
        }

        // Create the hash subdirectory if it doesn't exist
        if let Some(parent) = thumb_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create thumbnail subdirectory: {}", e))?;
        }

        // Check timeout before expensive decode
        if start.elapsed() > THUMBNAIL_TIMEOUT {
            return Err("Thumbnail generation timed out before decode".to_string());
        }

        // Strategy 2: Decode with downscale hint for large JPEGs
        let img = Self::decode_image_fast(photo_path, size)?;
        let img = apply_exif_orientation(img, orientation);

        // Check timeout after decode
        if start.elapsed() > THUMBNAIL_TIMEOUT {
            return Err("Thumbnail generation timed out after decode".to_string());
        }

        // Generate thumbnail
        let thumb = self.create_thumbnail(&img, size);

        // Save as JPEG. Lower quality at the small end where artifacts
        // are invisible to the eye anyway, kept high for the viewer-size
        // Large where compression shows.
        let quality = match size {
            ThumbnailSize::Small => 78,
            ThumbnailSize::Medium => 83,
            ThumbnailSize::Large => 88,
        };
        let mut out = std::fs::File::create(&thumb_path)
            .map_err(|e| format!("Failed to create thumbnail file: {}", e))?;
        let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
        encoder
            .encode_image(&thumb)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;

        self.add_to_cache(file_hash, size, &thumb_path);
        self.track_cache_size(&thumb_path);
        self.evict_if_needed();

        Ok(thumb_path)
    }

    fn try_existing_thumbnail(
        &self,
        file_hash: &str,
        size: ThumbnailSize,
        thumb_path: &Path,
    ) -> bool {
        if !thumb_path.exists() {
            return false;
        }
        let file_size = std::fs::metadata(thumb_path).map(|m| m.len()).unwrap_or(0);
        if file_size == 0 {
            return false;
        }
        // Most generated thumbnails are >5KB. For simple images (solid
        // colors, screenshots, graphics), valid JPEGs can be smaller, so
        // fall back to a cheap dimension check before treating them as stale
        // tiny EXIF extracts from older cache versions.
        if file_size <= 5000 && !Self::thumbnail_dimensions_are_adequate(thumb_path, size) {
            return false;
        }
        self.add_to_cache(file_hash, size, thumb_path);
        true
    }

    fn thumbnail_dimensions_are_adequate(thumb_path: &Path, size: ThumbnailSize) -> bool {
        ImageReader::open(thumb_path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.into_dimensions().ok())
            .map(|(width, height)| width.max(height) >= size.pixels())
            .unwrap_or(false)
    }

    /// Decode an image with fast downscaling for large files.
    ///
    /// For JPEGs, the underlying libjpeg can decode at 1/2, 1/4, or 1/8 scale
    /// natively — much faster than decoding full resolution then resizing.
    /// HEIC/HEIF route through `services::image_io::open_image` (libheif).
    fn decode_image_fast(
        photo_path: &Path,
        target_size: ThumbnailSize,
    ) -> Result<DynamicImage, String> {
        // Route HEIC/HEIF through the format-aware wrapper. The
        // `image` crate's reader doesn't recognise these formats so we
        // can't use its guessed-format path; fall back to a full decode
        // via libheif. For JPEG/PNG/etc, the existing fast path with
        // alloc limits stays in effect.
        let ext = photo_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if matches!(ext.as_deref(), Some("heic") | Some("heif")) {
            return crate::services::image_io::open_image(photo_path);
        }

        if matches!(ext.as_deref(), Some("jpg") | Some("jpeg")) {
            if let Some(img) = Self::decode_embedded_jpeg_thumbnail(photo_path, target_size) {
                return Ok(img);
            }
        }

        let reader =
            ImageReader::open(photo_path).map_err(|e| format!("Failed to open image: {}", e))?;

        // Use with_guessed_format for proper format detection
        let reader = reader
            .with_guessed_format()
            .map_err(|e| format!("Failed to guess image format: {}", e))?;

        // Set memory limits to prevent OOM on corrupt/huge images (200MB decode limit)
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(200 * 1024 * 1024);

        let mut reader = reader;
        reader.limits(limits);

        let img = reader
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        Ok(img)
    }

    fn decode_embedded_jpeg_thumbnail(
        photo_path: &Path,
        target_size: ThumbnailSize,
    ) -> Option<DynamicImage> {
        let file = File::open(photo_path).ok()?;
        let mut reader = BufReader::new(file);
        let exif = ExifReader::new().read_from_container(&mut reader).ok()?;
        let offset = exif
            .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
            .and_then(|f| f.value.get_uint(0))? as usize;
        let len = exif
            .get_field(Tag::JPEGInterchangeFormatLength, In::THUMBNAIL)
            .and_then(|f| f.value.get_uint(0))? as usize;
        let end = offset.checked_add(len)?;
        let bytes = exif.buf().get(offset..end)?;
        let img = image::load_from_memory(bytes).ok()?;
        let (width, height) = img.dimensions();
        if width.max(height) < target_size.pixels() {
            return None;
        }
        Some(img)
    }

    /// Track cache size after adding a thumbnail
    fn track_cache_size(&self, thumb_path: &Path) {
        let file_size = std::fs::metadata(thumb_path).map(|m| m.len()).unwrap_or(0);
        if let Ok(mut current) = self.current_cache_bytes.write() {
            *current += file_size;
        }
    }

    /// Create a thumbnail from an image, maintaining aspect ratio.
    ///
    /// Filter choice trades quality for speed:
    /// - Small (grid): `Triangle` is ~4× faster than `Lanczos3` and the
    ///   difference is invisible at 260 px on a typical screen.
    /// - Medium: `CatmullRom` keeps edges crisp without Lanczos3's cost.
    /// - Large (viewer): `Lanczos3` — the user is going to look at this.
    fn create_thumbnail(&self, img: &DynamicImage, size: ThumbnailSize) -> DynamicImage {
        let max_dim = size.pixels();
        let (width, height) = img.dimensions();

        // Calculate new dimensions maintaining aspect ratio
        let (new_width, new_height) = if width > height {
            let ratio = max_dim as f64 / width as f64;
            (max_dim, (height as f64 * ratio) as u32)
        } else {
            let ratio = max_dim as f64 / height as f64;
            ((width as f64 * ratio) as u32, max_dim)
        };

        let filter = match size {
            ThumbnailSize::Small => FilterType::Triangle,
            ThumbnailSize::Medium => FilterType::CatmullRom,
            ThumbnailSize::Large => FilterType::Lanczos3,
        };
        img.resize(new_width, new_height, filter)
    }

    /// Get the path where a thumbnail should be stored.
    /// Includes a version segment so quality upgrades trigger regeneration.
    fn thumbnail_path(&self, file_hash: &str, size: ThumbnailSize) -> PathBuf {
        // Use first 2 chars of hash as subdirectory (like git)
        let subdir = &file_hash[..2.min(file_hash.len())];

        self.cache_dir
            .join(size.dir_name())
            .join("v2")
            .join(subdir)
            .join(format!("{}.jpg", file_hash))
    }

    /// Add a thumbnail to the in-memory cache
    fn add_to_cache(&self, file_hash: &str, size: ThumbnailSize, path: &Path) {
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let entry = ThumbnailEntry {
            path: path.to_path_buf(),
            file_size,
        };

        if let Ok(mut cache) = self.cache.write() {
            cache.put((file_hash.to_string(), size), entry);
        }
    }

    /// Evict oldest thumbnails until we're back under 80% of the
    /// byte budget. LruCache's `pop_lru` gives O(1) access to the
    /// oldest entry, vs. the previous O(n log n) sort of every
    /// thumbnail by timestamp.
    fn evict_if_needed(&self) {
        let current = match self.current_cache_bytes.read() {
            Ok(v) => *v,
            Err(_) => return,
        };

        if current <= self.max_cache_bytes {
            return;
        }

        let target = self.max_cache_bytes * 80 / 100;
        let mut freed = 0u64;

        // Pop oldest entries one at a time until we're under target.
        // The cache lock is held only for the pop; file deletion
        // happens without it.
        loop {
            let popped = {
                let mut cache = match self.cache.write() {
                    Ok(c) => c,
                    Err(_) => break,
                };
                cache.pop_lru()
            };

            let Some((_, entry)) = popped else { break };

            if std::fs::remove_file(&entry.path).is_ok() {
                freed += entry.file_size;
            }

            if current.saturating_sub(freed) <= target {
                break;
            }
        }

        if let Ok(mut current) = self.current_cache_bytes.write() {
            *current = current.saturating_sub(freed);
        }

        tracing::info!("Evicted {} bytes from thumbnail cache", freed);
    }

    /// Scan existing thumbnails on disk and populate cache
    pub fn load_existing_thumbnails(&self) -> std::io::Result<()> {
        let mut total_size = 0u64;

        for size in [
            ThumbnailSize::Small,
            ThumbnailSize::Medium,
            ThumbnailSize::Large,
        ] {
            let size_dir = self.cache_dir.join(size.dir_name()).join("v2");

            if !size_dir.exists() {
                continue;
            }

            for subdir_entry in std::fs::read_dir(&size_dir)? {
                let subdir_entry = subdir_entry?;
                if !subdir_entry.file_type()?.is_dir() {
                    continue;
                }

                for file_entry in std::fs::read_dir(subdir_entry.path())? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();

                    if path.extension().map(|e| e == "jpg").unwrap_or(false) {
                        if let Some(stem) = path.file_stem() {
                            let hash = stem.to_string_lossy().to_string();
                            let file_size = file_entry.metadata()?.len();

                            total_size += file_size;

                            let entry = ThumbnailEntry {
                                path: path.clone(),
                                file_size,
                            };

                            if let Ok(mut cache) = self.cache.write() {
                                cache.put((hash, size), entry);
                            }
                        }
                    }
                }
            }
        }

        if let Ok(mut current) = self.current_cache_bytes.write() {
            *current = total_size;
        }

        let count = self.cache.read().map(|c| c.len()).unwrap_or(0);
        tracing::info!(
            "Loaded {} existing thumbnails ({} MB)",
            count,
            total_size / 1024 / 1024
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_thumbnail_path_generation() {
        let temp = tempdir().unwrap();
        let service = ThumbnailService::new(temp.path(), 1.0).unwrap();

        let path = service.thumbnail_path("abcdef123456", ThumbnailSize::Medium);

        assert!(path.to_string_lossy().contains("medium"));
        assert!(path.to_string_lossy().contains("ab")); // subdir
        assert!(path.to_string_lossy().contains("abcdef123456.jpg"));
    }

    #[test]
    fn test_thumbnail_sizes() {
        assert_eq!(ThumbnailSize::Small.pixels(), 260);
        assert_eq!(ThumbnailSize::Medium.pixels(), 430);
        assert_eq!(ThumbnailSize::Large.pixels(), 860);
    }

    #[test]
    fn test_cache_directories_created() {
        let temp = tempdir().unwrap();
        let _service = ThumbnailService::new(temp.path(), 1.0).unwrap();

        assert!(temp.path().join(".photovault/thumbnails/small").exists());
        assert!(temp.path().join(".photovault/thumbnails/medium").exists());
        assert!(temp.path().join(".photovault/thumbnails/large").exists());
    }

    #[test]
    fn small_but_dimensionally_valid_thumbnail_is_reused() {
        let temp = tempdir().unwrap();
        let service = ThumbnailService::new(temp.path(), 1.0).unwrap();
        let path = service.thumbnail_path("abcdef123456", ThumbnailSize::Small);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        DynamicImage::new_rgb8(ThumbnailSize::Small.pixels(), ThumbnailSize::Small.pixels())
            .save(&path)
            .unwrap();

        assert!(std::fs::metadata(&path).unwrap().len() <= 5000);
        assert!(service.try_existing_thumbnail("abcdef123456", ThumbnailSize::Small, &path));
    }

    #[test]
    fn test_generation_deduper_serializes_same_key() {
        let deduper = Arc::new(GenerationDeduper::new());
        let first = deduper.enter("medium:abc".into());
        let (tx, rx) = std::sync::mpsc::channel();
        let deduper_for_thread = deduper.clone();

        let handle = std::thread::spawn(move || {
            let _second = deduper_for_thread.enter("medium:abc".into());
            tx.send(()).unwrap();
        });

        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok());
        handle.join().unwrap();
    }

    #[test]
    fn test_concurrency_limiter_blocks_until_release() {
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;

        let limiter = Arc::new(ConcurrencyLimiter::new(1, 1));
        let permit = limiter.acquire(GenerationPriority::Foreground);

        // Second acquire should block. Spawn a thread that waits on
        // it, then release from the main thread after a short delay
        // and confirm the thread woke up.
        let l = limiter.clone();
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = started.clone();
        let handle = thread::spawn(move || {
            l.acquire(GenerationPriority::Foreground);
            s.store(true, std::sync::atomic::Ordering::Release);
        });

        thread::sleep(Duration::from_millis(20));
        assert!(!started.load(std::sync::atomic::Ordering::Acquire));

        drop(permit);
        handle.join().unwrap();
        assert!(started.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn test_background_generation_is_capped() {
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;

        let limiter = Arc::new(ConcurrencyLimiter::new(4, 1));
        let permit = limiter.acquire(GenerationPriority::Background);

        let (tx, rx) = mpsc::channel();
        let l = limiter.clone();
        let handle = thread::spawn(move || {
            let _permit = l.acquire(GenerationPriority::Background);
            tx.send(()).unwrap();
        });

        thread::sleep(Duration::from_millis(20));
        assert!(rx.try_recv().is_err());

        drop(permit);
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn test_waiting_foreground_generation_preempts_background() {
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;

        let limiter = Arc::new(ConcurrencyLimiter::new(1, 1));
        let permit = limiter.acquire(GenerationPriority::Background);

        let (tx, rx) = mpsc::channel();
        let l = limiter.clone();
        let tx_foreground = tx.clone();
        let foreground = thread::spawn(move || {
            let _permit = l.acquire(GenerationPriority::Foreground);
            tx_foreground.send("foreground").unwrap();
        });

        thread::sleep(Duration::from_millis(20));
        {
            let state = limiter.state.lock().unwrap();
            assert_eq!(state.foreground_waiting, 1);
        }

        let l = limiter.clone();
        let background = thread::spawn(move || {
            let _permit = l.acquire(GenerationPriority::Background);
            tx.send("background").unwrap();
        });

        thread::sleep(Duration::from_millis(20));
        assert!(rx.try_recv().is_err());

        drop(permit);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            "foreground"
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            "background"
        );

        foreground.join().unwrap();
        background.join().unwrap();
    }
}
