# Phase 3: Thumbnail Generation & Timeline UI

## Overview

This phase implements on-demand thumbnail generation following the freedesktop.org spec, and builds the core timeline UI with virtual scrolling for smooth performance with hundreds of thousands of photos.

**Estimated Time:** 4-5 days  
**Difficulty:** Intermediate-Advanced  
**Prerequisites:** Phase 2 complete

---

## UI Design Guidelines

> **IMPORTANT:** When implementing any UI components in this phase, you MUST read and follow the design principles in `SKILL.md`. This file contains critical guidelines for:
> - Typography and spacing standards
> - Color usage and contrast requirements
> - Animation and interaction patterns
> - Component design principles
> - Accessibility requirements
>
> **Before writing ANY UI code, read SKILL.md thoroughly.** The goal is a delightful, polished user experience - not just functional code.

---

## Goals

- [ ] Implement thumbnail generation service
- [ ] Follow freedesktop.org thumbnail spec (with modifications)
- [ ] LRU cache with configurable max size
- [ ] Build virtual scrolling photo grid
- [ ] Create timeline view with day/month grouping
- [ ] Implement smooth 60fps scrolling
- [ ] Add photo detail view (full-size preview)
- [ ] Keyboard navigation support

---

## Design Decisions

### Thumbnail Strategy

Following our earlier discussion, we use a hybrid approach:

1. **Extract embedded EXIF thumbnails first** (free, ~160x120px)
2. **Generate larger thumbnails on-demand** as user scrolls
3. **LRU cache** with configurable max size (default 2GB)
4. **Store on drive** in `.photovault/thumbnails/` for portability

### Thumbnail Sizes

| Size | Dimensions | Use Case |
|------|------------|----------|
| Small | 128x128 | Grid view at zoom-out |
| Medium | 256x256 | Default grid view |
| Large | 512x512 | Detail hover preview |

---

## New Files

```
src/
├── services/
│   └── thumbnail.rs        # Thumbnail generation service
├── components/
│   ├── photo_grid.rs       # Virtual scrolling photo grid
│   ├── photo_card.rs       # Individual photo card
│   └── day_header.rs       # Day separator in timeline
└── views/
    ├── timeline.rs         # Updated timeline view
    └── photo_detail.rs     # Full-size photo viewer
```

---

## Step 1: Thumbnail Service

### File: `src/services/thumbnail.rs`

```rust
//! Thumbnail generation and caching service
//!
//! Follows a modified freedesktop.org thumbnail spec:
//! - Thumbnails stored on the drive itself for portability
//! - Named by SHA256 hash of file path
//! - Stored as JPEG for smaller size
//! - Three sizes: 128, 256, 512

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use tokio::sync::Semaphore;

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
    pub size: ThumbnailSize,
    pub last_accessed: Instant,
    pub file_size: u64,
}

/// Thumbnail generation result
#[derive(Debug, Clone)]
pub enum ThumbnailResult {
    /// Thumbnail exists at path
    Ready(PathBuf),
    /// Thumbnail is being generated
    Pending,
    /// Generation failed
    Failed(String),
}

/// Thumbnail service with LRU cache management
pub struct ThumbnailService {
    /// Root path of the drive
    drive_root: PathBuf,
    
    /// Thumbnail cache directory
    cache_dir: PathBuf,
    
    /// In-memory cache of thumbnail paths (for quick lookup)
    cache: Arc<RwLock<HashMap<(String, ThumbnailSize), ThumbnailEntry>>>,
    
    /// Maximum cache size in bytes
    max_cache_bytes: u64,
    
    /// Current cache size in bytes
    current_cache_bytes: Arc<RwLock<u64>>,
    
    /// Semaphore to limit concurrent generation
    generation_semaphore: Arc<Semaphore>,
    
    /// Set of paths currently being generated
    generating: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl ThumbnailService {
    /// Create a new thumbnail service for a drive
    pub fn new<P: AsRef<Path>>(drive_root: P, max_cache_gb: f64) -> std::io::Result<Self> {
        let drive_root = drive_root.as_ref().to_path_buf();
        let cache_dir = drive_root.join(".photovault").join("thumbnails");
        
        // Create cache directories
        for size in [ThumbnailSize::Small, ThumbnailSize::Medium, ThumbnailSize::Large] {
            std::fs::create_dir_all(cache_dir.join(size.dir_name()))?;
        }
        
        let max_cache_bytes = (max_cache_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        
        Ok(Self {
            drive_root,
            cache_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_bytes,
            current_cache_bytes: Arc::new(RwLock::new(0)),
            generation_semaphore: Arc::new(Semaphore::new(4)), // 4 concurrent generations
            generating: Arc::new(RwLock::new(std::collections::HashSet::new())),
        })
    }

    /// Get or generate a thumbnail for a photo
    /// 
    /// Returns immediately with the thumbnail path if cached,
    /// or spawns generation and returns Pending.
    pub fn get_thumbnail(
        &self,
        photo_path: &str,
        file_hash: &str,
        size: ThumbnailSize,
    ) -> ThumbnailResult {
        let cache_key = (file_hash.to_string(), size);
        
        // Check in-memory cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                if entry.path.exists() {
                    return ThumbnailResult::Ready(entry.path.clone());
                }
            }
        }
        
        // Check if file exists on disk
        let thumb_path = self.thumbnail_path(file_hash, size);
        if thumb_path.exists() {
            // Add to in-memory cache
            self.add_to_cache(file_hash, size, &thumb_path);
            return ThumbnailResult::Ready(thumb_path);
        }
        
        // Check if already generating
        {
            let generating = self.generating.read().unwrap();
            if generating.contains(&cache_key.0) {
                return ThumbnailResult::Pending;
            }
        }
        
        // Mark as generating
        {
            let mut generating = self.generating.write().unwrap();
            generating.insert(cache_key.0.clone());
        }
        
        ThumbnailResult::Pending
    }

    /// Generate a thumbnail synchronously
    /// 
    /// Call this from a background task after get_thumbnail returns Pending.
    pub fn generate_thumbnail(
        &self,
        photo_path: &Path,
        file_hash: &str,
        size: ThumbnailSize,
    ) -> Result<PathBuf, String> {
        let _permit = self.generation_semaphore
            .try_acquire()
            .map_err(|_| "Too many concurrent thumbnail generations")?;
        
        let thumb_path = self.thumbnail_path(file_hash, size);
        
        // Load image
        let img = image::open(photo_path)
            .map_err(|e| format!("Failed to open image: {}", e))?;
        
        // Generate thumbnail
        let thumb = self.create_thumbnail(&img, size);
        
        // Save as JPEG
        thumb.save_with_format(&thumb_path, ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to save thumbnail: {}", e))?;
        
        // Get file size
        let file_size = std::fs::metadata(&thumb_path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        // Add to cache
        self.add_to_cache(file_hash, size, &thumb_path);
        
        // Update cache size
        {
            let mut current = self.current_cache_bytes.write().unwrap();
            *current += file_size;
        }
        
        // Evict if over limit
        self.evict_if_needed();
        
        // Remove from generating set
        {
            let mut generating = self.generating.write().unwrap();
            generating.remove(file_hash);
        }
        
        Ok(thumb_path)
    }

    /// Try to extract embedded EXIF thumbnail
    pub fn extract_exif_thumbnail(
        &self,
        photo_path: &Path,
        file_hash: &str,
    ) -> Option<PathBuf> {
        // Read EXIF data
        let file = std::fs::File::open(photo_path).ok()?;
        let mut bufreader = std::io::BufReader::new(file);
        
        let exif = kamadak_exif::Reader::new()
            .read_from_container(&mut bufreader)
            .ok()?;
        
        // Look for thumbnail
        for field in exif.fields() {
            if field.tag == kamadak_exif::Tag::JPEGInterchangeFormat {
                // Found thumbnail offset - this is complex, skip for now
                // In practice, the image crate handles this better
            }
        }
        
        None // Embedded thumbnail extraction is complex, skip for MVP
    }

    /// Create a thumbnail from an image
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
        
        // Use Lanczos3 for high quality downscaling
        img.resize(new_width, new_height, FilterType::Lanczos3)
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
        let file_size = std::fs::metadata(path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        let entry = ThumbnailEntry {
            path: path.to_path_buf(),
            size,
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
        
        // Get all entries sorted by last accessed time
        let mut entries: Vec<_> = {
            let cache = self.cache.read().unwrap();
            cache.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        
        entries.sort_by(|a, b| a.1.last_accessed.cmp(&b.1.last_accessed));
        
        // Remove oldest entries until under limit
        let target = self.max_cache_bytes * 80 / 100; // Evict to 80% of max
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
        
        for size in [ThumbnailSize::Small, ThumbnailSize::Medium, ThumbnailSize::Large] {
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
                                size,
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
}
```

---

## Step 2: Photo Grid Component

### File: `src/components/photo_grid.rs`

```rust
//! Virtual scrolling photo grid component
//!
//! Renders only visible photos for smooth performance with large libraries.
//! Uses a uniform grid layout with responsive column count.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::{self, Event};
use iced::mouse;
use iced::{Element, Length, Point, Rectangle, Size, Vector};

use crate::models::Photo;
use crate::app::Message;
use crate::theme::colors::{Backgrounds, Border, Text};

/// Configuration for the photo grid
#[derive(Debug, Clone)]
pub struct PhotoGridConfig {
    /// Size of each thumbnail in pixels
    pub thumbnail_size: f32,
    /// Gap between thumbnails
    pub gap: f32,
    /// Padding around the grid
    pub padding: f32,
}

impl Default for PhotoGridConfig {
    fn default() -> Self {
        Self {
            thumbnail_size: 160.0,
            gap: 8.0,
            padding: 16.0,
        }
    }
}

/// A photo grid item (photo or day header)
#[derive(Debug, Clone)]
pub enum GridItem {
    Photo(Photo),
    DayHeader { date: String, location: Option<String>, count: usize },
}

/// Virtual scrolling photo grid widget
pub struct PhotoGrid<'a> {
    /// All items to display
    items: &'a [GridItem],
    /// Configuration
    config: PhotoGridConfig,
    /// Current scroll offset
    scroll_offset: f32,
    /// Total height of content
    content_height: f32,
    /// Callback for photo selection
    on_select: Option<Box<dyn Fn(i64) -> Message + 'a>>,
}

impl<'a> PhotoGrid<'a> {
    pub fn new(items: &'a [GridItem]) -> Self {
        Self {
            items,
            config: PhotoGridConfig::default(),
            scroll_offset: 0.0,
            content_height: 0.0,
            on_select: None,
        }
    }

    pub fn thumbnail_size(mut self, size: f32) -> Self {
        self.config.thumbnail_size = size;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = gap;
        self
    }

    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: Fn(i64) -> Message + 'a,
    {
        self.on_select = Some(Box::new(callback));
        self
    }

    /// Calculate number of columns for a given width
    fn columns_for_width(&self, width: f32) -> usize {
        let available = width - self.config.padding * 2.0;
        let cell_width = self.config.thumbnail_size + self.config.gap;
        (available / cell_width).floor().max(1.0) as usize
    }

    /// Calculate total content height
    fn calculate_content_height(&self, columns: usize) -> f32 {
        let mut height = self.config.padding;
        let mut col = 0;

        for item in self.items {
            match item {
                GridItem::DayHeader { .. } => {
                    // Day headers take full width
                    if col > 0 {
                        height += self.config.thumbnail_size + self.config.gap;
                        col = 0;
                    }
                    height += 60.0 + self.config.gap; // Header height
                }
                GridItem::Photo(_) => {
                    col += 1;
                    if col >= columns {
                        height += self.config.thumbnail_size + self.config.gap;
                        col = 0;
                    }
                }
            }
        }

        // Add remaining row if partially filled
        if col > 0 {
            height += self.config.thumbnail_size + self.config.gap;
        }

        height + self.config.padding
    }

    /// Get visible range of items for the current scroll position
    fn visible_range(&self, viewport_height: f32, columns: usize) -> (usize, usize) {
        let row_height = self.config.thumbnail_size + self.config.gap;
        
        // Estimate start row
        let start_row = (self.scroll_offset / row_height).floor() as usize;
        let visible_rows = (viewport_height / row_height).ceil() as usize + 2; // +2 for buffer
        
        // Convert to item indices (rough estimate - headers complicate this)
        let start_idx = start_row.saturating_mul(columns);
        let end_idx = (start_idx + visible_rows * columns).min(self.items.len());
        
        (start_idx, end_idx)
    }
}

// For a simpler implementation, we'll use iced's scrollable + column
// The full virtual scrolling widget is complex; here's a simplified approach:

use iced::widget::{column, container, image, row, scrollable, text, Column, Row, Space};

/// Render a simplified photo grid using standard widgets
/// 
/// Note: For 100k+ photos, a custom virtual scrolling widget is needed.
/// This simplified version works for smaller collections.
pub fn photo_grid_simple<'a>(
    photos: &'a [Photo],
    thumbnail_size: f32,
    columns: usize,
    on_select: impl Fn(i64) -> Message + 'a + Clone,
) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    let mut current_row: Vec<Element<'a, Message>> = Vec::new();

    for photo in photos {
        // Create photo card
        let card = photo_card(photo, thumbnail_size, on_select.clone());
        current_row.push(card);

        if current_row.len() >= columns {
            rows.push(
                Row::with_children(current_row)
                    .spacing(8.0)
                    .into()
            );
            current_row = Vec::new();
        }
    }

    // Add remaining photos
    if !current_row.is_empty() {
        // Pad with empty space
        while current_row.len() < columns {
            current_row.push(Space::with_width(thumbnail_size).into());
        }
        rows.push(
            Row::with_children(current_row)
                .spacing(8.0)
                .into()
        );
    }

    scrollable(
        Column::with_children(rows)
            .spacing(8.0)
            .padding(16.0)
    )
    .height(Length::Fill)
    .into()
}

/// Render a single photo card
fn photo_card<'a>(
    photo: &'a Photo,
    size: f32,
    on_select: impl Fn(i64) -> Message + 'a,
) -> Element<'a, Message> {
    use iced::widget::button;

    let photo_id = photo.id;
    
    // Placeholder - in real implementation, load actual thumbnail
    let content = container(
        text(&photo.file_name)
            .size(10)
            .color(Text::SECONDARY)
    )
    .width(size)
    .height(size)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(|_theme| container::Style {
        background: Some(Backgrounds::ELEVATED.into()),
        border: iced::Border {
            color: Border::SUBTLE,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    });

    button(content)
        .padding(0)
        .style(|_theme, status| {
            let border_color = match status {
                button::Status::Hovered => Border::VISIBLE,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: None,
                border: iced::Border {
                    color: border_color,
                    width: 2.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(on_select(photo_id))
        .into()
}

/// Render a day header
pub fn day_header<'a>(
    date: &str,
    location: Option<&str>,
    count: usize,
) -> Element<'a, Message> {
    let date_text = text(date)
        .size(16)
        .color(Text::PRIMARY);

    let mut header_row = Row::new()
        .push(date_text)
        .push(Space::with_width(Length::Fill));

    if let Some(loc) = location {
        header_row = header_row.push(
            text(loc)
                .size(14)
                .color(Text::SECONDARY)
        );
        header_row = header_row.push(Space::with_width(16));
    }

    header_row = header_row.push(
        text(format!("{} photos", count))
            .size(12)
            .color(Text::TERTIARY)
    );

    container(header_row)
        .width(Length::Fill)
        .padding([16, 0])
        .style(|_theme| container::Style {
            border: iced::Border {
                color: Border::SUBTLE,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
```

---

## Step 3: Timeline View (Updated)

### File: `src/views/timeline.rs`

```rust
//! Timeline view - main photo browsing interface
//!
//! Displays photos organized by date with day headers.
//! Uses virtual scrolling for performance with large libraries.

use chrono::{DateTime, Datelike, Utc};
use iced::widget::{column, container, row, scrollable, text, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::photo_grid::{day_header, photo_grid_simple};
use crate::models::Photo;
use crate::theme::colors::{Backgrounds, Text};

/// Group photos by date
#[derive(Debug, Clone)]
pub struct DateGroup {
    pub date: String,
    pub display_date: String,
    pub location: Option<String>,
    pub photos: Vec<Photo>,
}

/// Timeline view component
pub struct TimelineView;

impl TimelineView {
    /// Render the timeline view with photos
    pub fn view_with_photos(photos: &[Photo]) -> Element<'static, Message> {
        if photos.is_empty() {
            return Self::empty_view();
        }

        // Group photos by date
        let groups = Self::group_by_date(photos);

        // Build the timeline
        let mut timeline_items: Vec<Element<'static, Message>> = Vec::new();

        for group in groups {
            // Add day header
            timeline_items.push(day_header(
                &group.display_date,
                group.location.as_deref(),
                group.photos.len(),
            ));

            // Add photo grid for this day
            // Clone photos for the closure
            let day_photos: Vec<Photo> = group.photos.clone();
            
            timeline_items.push(
                photo_grid_simple(
                    &day_photos,
                    160.0,
                    6, // columns - should be calculated from window width
                    |id| Message::SelectPhoto(id),
                )
            );
        }

        let content = Column::with_children(timeline_items)
            .spacing(0)
            .width(Length::Fill);

        scrollable(content)
            .height(Length::Fill)
            .into()
    }

    /// Render empty timeline
    pub fn view() -> Element<'static, Message> {
        Self::empty_view()
    }

    /// Empty state view
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Timeline")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Your photos will appear here after indexing.")
                .size(14)
                .color(Text::SECONDARY),
            Space::with_height(32),
            text("Photos are organized by date, newest first.")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Group photos by date
    fn group_by_date(photos: &[Photo]) -> Vec<DateGroup> {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, DateGroup> = BTreeMap::new();

        for photo in photos {
            let date_key = photo
                .date_taken
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            let display_date = photo
                .date_taken
                .map(|d| Self::format_display_date(&d))
                .unwrap_or_else(|| "Unknown Date".to_string());

            let group = groups.entry(date_key.clone()).or_insert_with(|| DateGroup {
                date: date_key,
                display_date,
                location: photo.location_string(),
                photos: Vec::new(),
            });

            // Update location if this photo has one and group doesn't
            if group.location.is_none() && photo.has_location() {
                group.location = photo.location_string();
            }

            group.photos.push(photo.clone());
        }

        // Convert to vec and reverse (newest first)
        let mut result: Vec<_> = groups.into_values().collect();
        result.reverse();
        result
    }

    /// Format a date for display
    fn format_display_date(date: &DateTime<Utc>) -> String {
        let now = Utc::now();
        let today = now.date_naive();
        let photo_date = date.date_naive();

        if photo_date == today {
            "Today".to_string()
        } else if photo_date == today.pred_opt().unwrap_or(today) {
            "Yesterday".to_string()
        } else if photo_date.year() == today.year() {
            date.format("%B %d").to_string() // "March 15"
        } else {
            date.format("%B %d, %Y").to_string() // "March 15, 2019"
        }
    }
}
```

---

## Step 4: Photo Detail View

### File: `src/views/photo_detail.rs`

```rust
//! Photo detail view - full-size photo viewer
//!
//! Modal overlay for viewing photos at full resolution.
//! Supports keyboard navigation between photos.

use iced::widget::{button, column, container, image, row, text, Space};
use iced::keyboard::{self, Key};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::theme::colors::{Backgrounds, Border, Text, Accent};

/// Photo detail view component
pub struct PhotoDetailView;

impl PhotoDetailView {
    /// Render the photo detail view
    pub fn view(photo: &Photo, has_prev: bool, has_next: bool) -> Element<'static, Message> {
        // Close button (top right)
        let close_btn = button(
            text("×").size(24).color(Text::PRIMARY)
        )
        .padding(Padding::from([8, 12]))
        .style(|_theme, status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                _ => None,
            };
            button::Style {
                background,
                text_color: Text::PRIMARY,
                border: iced::Border::default(),
                ..Default::default()
            }
        })
        .on_press(Message::ClosePhotoDetail);

        // Navigation buttons
        let prev_btn = Self::nav_button("←", Message::PreviousPhoto, has_prev);
        let next_btn = Self::nav_button("→", Message::NextPhoto, has_next);

        // Photo metadata
        let metadata = Self::metadata_bar(photo);

        // Main image area (placeholder - actual image loading is complex)
        let image_area = container(
            column![
                text(&photo.file_name)
                    .size(16)
                    .color(Text::PRIMARY),
                Space::with_height(8),
                text(format!("{}x{}", 
                    photo.width.unwrap_or(0), 
                    photo.height.unwrap_or(0)
                ))
                .size(14)
                .color(Text::SECONDARY),
            ]
            .align_x(Alignment::Center)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Color::BLACK.into()),
            ..Default::default()
        });

        // Layout: header, image with nav buttons, metadata
        let header = row![
            Space::with_width(Length::Fill),
            close_btn,
        ]
        .padding(Padding::from([8, 16]));

        let image_with_nav = row![
            prev_btn,
            image_area,
            next_btn,
        ]
        .align_y(Alignment::Center);

        let content = column![
            header,
            image_with_nav,
            metadata,
        ];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.scale_alpha(0.95).into()),
                ..Default::default()
            })
            .into()
    }

    /// Navigation button
    fn nav_button(label: &str, message: Message, enabled: bool) -> Element<'static, Message> {
        let btn = button(
            text(label).size(32).color(if enabled {
                Text::PRIMARY
            } else {
                Text::TERTIARY
            })
        )
        .padding(Padding::from([20, 16]))
        .style(move |_theme, status| {
            let background = if !enabled {
                None
            } else {
                match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                }
            };
            button::Style {
                background,
                border: iced::Border::default(),
                ..Default::default()
            }
        });

        if enabled {
            btn.on_press(message).into()
        } else {
            btn.into()
        }
    }

    /// Metadata bar at bottom
    fn metadata_bar(photo: &Photo) -> Element<'static, Message> {
        let mut items: Vec<Element<'static, Message>> = Vec::new();

        // Date
        if let Some(date) = &photo.date_taken {
            items.push(
                Self::metadata_item("Date", &date.format("%B %d, %Y %H:%M").to_string())
            );
        }

        // Location
        if let Some(location) = photo.location_string() {
            items.push(Self::metadata_item("Location", &location));
        }

        // Camera
        if let Some(ref make) = photo.camera_make {
            let camera = match &photo.camera_model {
                Some(model) => format!("{} {}", make, model),
                None => make.clone(),
            };
            items.push(Self::metadata_item("Camera", &camera));
        }

        // Dimensions
        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            items.push(Self::metadata_item("Size", &format!("{}×{}", w, h)));
        }

        let metadata_row = iced::widget::Row::with_children(items)
            .spacing(32)
            .align_y(Alignment::Center);

        container(metadata_row)
            .width(Length::Fill)
            .padding(Padding::from([16, 32]))
            .style(|_theme| container::Style {
                background: Some(Backgrounds::SECONDARY.into()),
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Single metadata item
    fn metadata_item<'a>(label: &str, value: &str) -> Element<'a, Message> {
        column![
            text(label)
                .size(11)
                .color(Text::TERTIARY),
            text(value)
                .size(13)
                .color(Text::PRIMARY),
        ]
        .spacing(2)
        .into()
    }
}

/// Color extension for alpha scaling
trait ColorExt {
    fn scale_alpha(self, factor: f32) -> iced::Color;
}

impl ColorExt for iced::Color {
    fn scale_alpha(self, factor: f32) -> iced::Color {
        iced::Color { a: self.a * factor, ..self }
    }
}
```

---

## Step 5: Update Application Messages

Add new messages to `src/app.rs`:

```rust
/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing messages ...

    /// Select a photo to view in detail
    SelectPhoto(i64),

    /// Close photo detail view
    ClosePhotoDetail,

    /// Navigate to previous photo
    PreviousPhoto,

    /// Navigate to next photo
    NextPhoto,

    /// Thumbnail ready for a photo
    ThumbnailReady { photo_id: i64, path: PathBuf },

    /// Request thumbnail generation
    RequestThumbnail { photo_id: i64, file_hash: String },
}
```

---

## Step 6: Update Services Module

### File: `src/services/mod.rs`

```rust
//! Application services

pub mod drive_detector;
pub mod scanner;
pub mod exif_extractor;
pub mod thumbnail;

pub use drive_detector::{DriveDetector, DriveInfo};
pub use scanner::{Scanner, DiscoveredFile, ScanProgress};
pub use exif_extractor::{ExifExtractor, ImageMetadata};
pub use thumbnail::{ThumbnailService, ThumbnailSize, ThumbnailResult};
```

---

## UI Design Notes

### Timeline View Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  Timeline                           [Zoom: ████░] │
│             │─────────────────────────────────────────────────── │
│  Timeline   │                                                    │
│  People     │  ═══ Today ══════════════════════════ Home ═══════│
│  Search     │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ │
│             │  │     │ │     │ │     │ │     │ │     │ │     │ │
│  ─────────  │  │ 📷  │ │ 📷  │ │ 📷  │ │ 📷  │ │ 📷  │ │ 📷  │ │
│             │  │     │ │     │ │     │ │     │ │     │ │     │ │
│  Settings   │  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ │
│             │                                                    │
│             │  ═══ Yesterday ═════════════════════ Office ══════│
│             │  ┌─────┐ ┌─────┐ ┌─────┐                          │
│             │  │     │ │     │ │     │                          │
│             │  │ 📷  │ │ 📷  │ │ 📷  │                          │
│             │  │     │ │     │ │     │                          │
│             │  └─────┘ └─────┘ └─────┘                          │
│             │                                                    │
└─────────────────────────────────────────────────────────────────┘
```

### Photo Detail View

```
┌─────────────────────────────────────────────────────────────────┐
│                                                              [×] │
│                                                                  │
│                                                                  │
│   ◀                    ┌───────────────────┐                 ▶  │
│                        │                   │                     │
│                        │                   │                     │
│                        │    [Full Photo]   │                     │
│                        │                   │                     │
│                        │                   │                     │
│                        └───────────────────┘                     │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│ Date           Location         Camera              Size        │
│ Mar 15, 2019   Tokyo, Japan     iPhone 12 Pro       4032×3024   │
└──────────────────────────────────────────────────────────────────┘
```

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `←` / `→` | Previous/Next photo (in detail view) |
| `Escape` | Close detail view |
| `Home` | Jump to newest photos |
| `End` | Jump to oldest photos |
| `Page Up/Down` | Scroll by page |
| `+` / `-` | Zoom thumbnail size |

---

## Verification Checklist

- [ ] Thumbnails generated on-demand as user scrolls
- [ ] Thumbnails cached on disk in `.photovault/thumbnails/`
- [ ] LRU eviction works when cache exceeds limit
- [ ] Timeline shows photos grouped by date
- [ ] Day headers show date, location, and count
- [ ] Clicking photo opens detail view
- [ ] Detail view shows metadata (date, location, camera)
- [ ] Arrow keys navigate between photos
- [ ] Escape closes detail view
- [ ] Scrolling is smooth (60fps target)
- [ ] Column count adapts to window width

---

## Performance Notes

### Virtual Scrolling

For 100k+ photos, true virtual scrolling is essential:
- Only render visible items (~50-100 at a time)
- Recycle DOM elements during scroll
- Pre-load thumbnails for items just outside viewport

The simplified implementation in this phase works for up to ~10k photos.
For larger libraries, implement a custom Iced widget with virtual scrolling.

### Thumbnail Generation

- Generate thumbnails in background thread
- Use image crate's fast Lanczos3 downscaling
- Consider using `turbojpeg` for faster JPEG encoding
- Batch requests to avoid overwhelming disk IO

---

## Next Phase Preview

**Phase 4: Face Detection & Clustering** will add:
- ONNX runtime integration
- SCRFD face detection model
- ArcFace embedding generation
- DBSCAN clustering for grouping faces
- People view with face clusters

---

## Expected Results & Behavior

> **IMPORTANT:** Do not proceed to Phase 4 until a human has verified ALL of the following behaviors. Each item must be manually tested and confirmed.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **Timeline View** | Photos displayed in chronological grid, grouped by day |
| **Day Headers** | Each day group has a header showing date (e.g., "March 15, 2019") |
| **Photo Thumbnails** | Clear, properly-sized thumbnails in grid layout (not stretched/squished) |
| **Thumbnail Loading** | Placeholders shown while thumbnails generate, replaced smoothly |
| **Photo Detail View** | Full-size photo displayed with metadata panel (date, camera, location) |
| **Scrolling Performance** | Smooth 60fps scrolling through timeline, no visible jank |
| **Empty Timeline** | Graceful message when no photos are indexed |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **Scroll timeline** | Photos load progressively, smooth scrolling at 60fps |
| **Click photo thumbnail** | Opens photo detail view with full-size image |
| **Press Escape in detail view** | Returns to timeline at same scroll position |
| **Press Left/Right arrows in detail** | Navigates to previous/next photo |
| **Resize window** | Grid reflows to fit available width, thumbnails adjust |
| **Scroll past many photos (1000+)** | Virtual scrolling keeps memory stable, no slowdown |

### Technical Verification

```bash
# Check thumbnails directory exists
ls -la /path/to/drive/.photovault/thumbnails/

# Count generated thumbnails
find /path/to/drive/.photovault/thumbnails/ -name "*.jpg" | wc -l

# Check thumbnail sizes (should see 128, 256, 512 variants)
ls -la /path/to/drive/.photovault/thumbnails/ | head -20

# Verify LRU cache size stays under limit
du -sh /path/to/drive/.photovault/thumbnails/
```

**Expected:** Thumbnails directory contains generated files. Total size stays under configured cache limit (default 2GB). Multiple size variants exist per photo.

### Performance Verification

| Metric | Expected |
|--------|----------|
| **Thumbnail generation** | < 100ms per thumbnail |
| **Timeline initial load** | < 1 second for first screen of photos |
| **Scroll framerate** | Consistent 60fps with 10k+ photos |
| **Memory usage** | Under 300MB with LRU cache active |
| **Cache hit rate** | > 90% for repeated views of same photos |

### Sign-off Checklist

Before proceeding to Phase 4, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **Timeline displays:** Photos shown in chronological grid with day groupings
- [ ] **Thumbnails generate:** On-demand generation works for all supported formats
- [ ] **Three sizes created:** 128px, 256px, and 512px thumbnails generated
- [ ] **LRU cache works:** Old thumbnails evicted when cache limit reached
- [ ] **Thumbnails stored correctly:** Files in `.photovault/thumbnails/` directory
- [ ] **Photo detail works:** Click opens full-size view with metadata
- [ ] **Keyboard navigation:** Arrow keys and Escape work in detail view
- [ ] **Smooth scrolling:** 60fps maintained with large photo libraries
- [ ] **No console errors:** Clean operation during browsing
- [ ] **SKILL.md followed:** Timeline UI matches design guidelines (spacing, typography, colors)

**Signature:** ___________________ **Date:** _______________

---

## Proceed to Phase 4

Only after ALL items above are verified, proceed to:

📁 `plan/PHASE_4_FACE_DETECTION.md`
