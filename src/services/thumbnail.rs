//! Thumbnail generation and caching service
//!
//! Follows a modified freedesktop.org thumbnail spec:
//! - Thumbnails stored on the drive itself for portability
//! - Named by SHA256 hash of file path
//! - Stored as JPEG for smaller size
//! - Three sizes: 128, 256, 512

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use image::imageops::FilterType;
use image::{codecs::jpeg::JpegEncoder, DynamicImage, GenericImageView};

/// Thumbnail size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    Small,  // 128x128
    Medium, // 256x256
    Large,  // 512x512
}

impl ThumbnailSize {
    pub fn pixels(&self) -> u32 {
        match self {
            ThumbnailSize::Small => 128,
            ThumbnailSize::Medium => 256,
            ThumbnailSize::Large => 512,
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

/// Cached thumbnail entry
#[derive(Debug, Clone)]
pub struct ThumbnailEntry {
    pub path: PathBuf,
    pub last_accessed: Instant,
    pub file_size: u64,
}

/// Concurrency limiter using std::sync primitives (safe for spawn_blocking)
struct ConcurrencyLimiter {
    max: usize,
    current: Mutex<usize>,
}

impl ConcurrencyLimiter {
    fn new(max: usize) -> Self {
        Self {
            max,
            current: Mutex::new(0),
        }
    }

    /// Try to acquire a permit. Returns true if acquired.
    fn try_acquire(&self) -> bool {
        let mut current = self.current.lock().unwrap();
        if *current < self.max {
            *current += 1;
            true
        } else {
            false
        }
    }

    /// Release a permit.
    fn release(&self) {
        let mut current = self.current.lock().unwrap();
        *current = current.saturating_sub(1);
    }
}

/// Thumbnail service with LRU cache management
pub struct ThumbnailService {
    /// Root path of the drive
    _drive_root: PathBuf,

    /// Thumbnail cache directory
    cache_dir: PathBuf,

    /// In-memory cache of thumbnail paths (for quick lookup)
    cache: Arc<RwLock<HashMap<(String, ThumbnailSize), ThumbnailEntry>>>,

    /// Maximum cache size in bytes
    max_cache_bytes: u64,

    /// Current cache size in bytes
    current_cache_bytes: Arc<RwLock<u64>>,

    /// Concurrency limiter for generation (std::sync for blocking context)
    generation_limiter: Arc<ConcurrencyLimiter>,

    /// Set of hashes currently being generated
    generating: Arc<RwLock<std::collections::HashSet<String>>>,
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

        Ok(Self {
            _drive_root: drive_root,
            cache_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_bytes,
            current_cache_bytes: Arc::new(RwLock::new(0)),
            generation_limiter: Arc::new(ConcurrencyLimiter::new(4)), // Leave headroom for UI responsiveness
            generating: Arc::new(RwLock::new(std::collections::HashSet::new())),
        })
    }

    /// Generate a thumbnail synchronously
    ///
    /// Call this from a background task (spawn_blocking) after get_thumbnail returns Pending.
    pub fn generate_thumbnail(
        &self,
        photo_path: &Path,
        file_hash: &str,
        size: ThumbnailSize,
    ) -> Result<PathBuf, String> {
        // Acquire concurrency permit (std::sync for blocking context)
        if !self.generation_limiter.try_acquire() {
            return Err("Too many concurrent thumbnail generations".to_string());
        }

        let result = self.generate_thumbnail_inner(photo_path, file_hash, size);

        // Always release permit
        self.generation_limiter.release();

        // Remove from generating set
        {
            let mut generating = self.generating.write().unwrap();
            generating.remove(file_hash);
        }

        result
    }

    /// Inner thumbnail generation (separated for clean permit release)
    fn generate_thumbnail_inner(
        &self,
        photo_path: &Path,
        file_hash: &str,
        size: ThumbnailSize,
    ) -> Result<PathBuf, String> {
        let thumb_path = self.thumbnail_path(file_hash, size);

        if thumb_path.exists() {
            self.add_to_cache(file_hash, size, &thumb_path);
            return Ok(thumb_path);
        }

        // Create the hash subdirectory if it doesn't exist
        if let Some(parent) = thumb_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create thumbnail subdirectory: {}", e))?;
        }

        // Load image using image::ImageReader (image 0.25 API)
        let img = image::ImageReader::open(photo_path)
            .map_err(|e| format!("Failed to open image: {}", e))?
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        // Generate thumbnail
        let thumb = self.create_thumbnail(&img, size);

        // Save as JPEG with faster/lower-quality profile for quicker generation
        let quality = match size {
            ThumbnailSize::Small => 55,
            ThumbnailSize::Medium => 65,
            ThumbnailSize::Large => 78,
        };
        let mut out = std::fs::File::create(&thumb_path)
            .map_err(|e| format!("Failed to create thumbnail file: {}", e))?;
        let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
        encoder
            .encode_image(&thumb)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;

        // Get file size
        let file_size = std::fs::metadata(&thumb_path).map(|m| m.len()).unwrap_or(0);

        // Add to cache
        self.add_to_cache(file_hash, size, &thumb_path);

        // Update cache size
        {
            let mut current = self.current_cache_bytes.write().unwrap();
            *current += file_size;
        }

        // Evict if over limit
        self.evict_if_needed();

        Ok(thumb_path)
    }

    /// Create a thumbnail from an image, maintaining aspect ratio
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

        match size {
            // Fast-path for timeline responsiveness.
            ThumbnailSize::Small | ThumbnailSize::Medium => img.thumbnail(new_width, new_height),
            ThumbnailSize::Large => img.resize(new_width, new_height, FilterType::CatmullRom),
        }
    }

    /// Get the path where a thumbnail should be stored
    fn thumbnail_path(&self, file_hash: &str, size: ThumbnailSize) -> PathBuf {
        // Use first 2 chars of hash as subdirectory (like git)
        let subdir = &file_hash[..2.min(file_hash.len())];

        self.cache_dir
            .join(size.dir_name())
            .join(subdir)
            .join(format!("{}.jpg", file_hash))
    }

    /// Add a thumbnail to the in-memory cache
    fn add_to_cache(&self, file_hash: &str, size: ThumbnailSize, path: &Path) {
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let entry = ThumbnailEntry {
            path: path.to_path_buf(),
            last_accessed: Instant::now(),
            file_size,
        };

        let mut cache = self.cache.write().unwrap();
        cache.insert((file_hash.to_string(), size), entry);
    }

    /// Evict old thumbnails if cache is over limit
    fn evict_if_needed(&self) {
        let current = *self.current_cache_bytes.read().unwrap();

        if current <= self.max_cache_bytes {
            return;
        }

        // Get all entries sorted by last accessed time (oldest first)
        let mut entries: Vec<_> = {
            let cache = self.cache.read().unwrap();
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        entries.sort_by(|a, b| a.1.last_accessed.cmp(&b.1.last_accessed));

        // Remove oldest entries until under 80% of max
        let target = self.max_cache_bytes * 80 / 100;
        let mut freed = 0u64;
        let mut to_remove = Vec::new();

        for (key, entry) in entries {
            if current - freed <= target {
                break;
            }

            // Delete file
            if std::fs::remove_file(&entry.path).is_ok() {
                freed += entry.file_size;
                to_remove.push(key);
            }
        }

        // Remove from cache
        {
            let mut cache = self.cache.write().unwrap();
            for key in to_remove {
                cache.remove(&key);
            }
        }

        // Update current size
        {
            let mut current = self.current_cache_bytes.write().unwrap();
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
            let size_dir = self.cache_dir.join(size.dir_name());

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
                                last_accessed: Instant::now(),
                                file_size,
                            };

                            let mut cache = self.cache.write().unwrap();
                            cache.insert((hash, size), entry);
                        }
                    }
                }
            }
        }

        *self.current_cache_bytes.write().unwrap() = total_size;

        tracing::info!(
            "Loaded {} existing thumbnails ({} MB)",
            self.cache.read().unwrap().len(),
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
        assert_eq!(ThumbnailSize::Small.pixels(), 128);
        assert_eq!(ThumbnailSize::Medium.pixels(), 256);
        assert_eq!(ThumbnailSize::Large.pixels(), 512);
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
    fn test_concurrency_limiter() {
        let limiter = ConcurrencyLimiter::new(2);
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire()); // Should fail, at max
        limiter.release();
        assert!(limiter.try_acquire()); // Should succeed again
    }
}
