# Phase 1 - MVP Specification

## Overview

PhotoVault is a desktop application that indexes photos from external hard drives, providing Google Photos-like features entirely offline. The database lives on the drive itself, making it fully portable.

**Core Principle:** We are a lens, not a hand. We observe and index. We never modify or move files unless explicitly asked to delete.

---

## Tech Stack

- **Language:** Rust
- **UI Framework:** Tauri v2 + React/TypeScript (or Solid/Svelte - TBD)
- **Database:** SQLite (stored on the indexed drive)
- **ML Runtime:** ONNX Runtime (`ort` crate)
- **Face Detection:** SCRFD or RetinaFace (ONNX model)
- **Face Embedding:** ArcFace (ONNX model)
- **Image Processing:** `image` crate, `kamadak-exif`

---

## Database Schema

The SQLite database is stored at `<drive_root>/.photovault/photovault.db`

```sql
-- Core photo metadata
CREATE TABLE photos (
    id INTEGER PRIMARY KEY,
    file_path TEXT NOT NULL,           -- Relative path from drive root
    file_name TEXT NOT NULL,
    file_hash TEXT NOT NULL,           -- SHA256 for duplicate detection
    file_size INTEGER NOT NULL,
    
    -- EXIF data
    date_taken DATETIME,               -- From EXIF, fallback to file mtime
    date_taken_source TEXT,            -- 'exif' | 'filename' | 'mtime'
    gps_latitude REAL,
    gps_longitude REAL,
    location_city TEXT,                -- Reverse geocoded
    location_country TEXT,             -- Reverse geocoded
    camera_make TEXT,
    camera_model TEXT,
    width INTEGER,
    height INTEGER,
    orientation INTEGER,
    
    -- Processing state
    thumbnail_generated BOOLEAN DEFAULT FALSE,
    faces_processed BOOLEAN DEFAULT FALSE,
    
    -- Soft delete
    is_trashed BOOLEAN DEFAULT FALSE,
    trashed_at DATETIME,
    
    -- Timestamps
    indexed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(file_path)
);

-- Detected faces in photos
CREATE TABLE faces (
    id INTEGER PRIMARY KEY,
    photo_id INTEGER NOT NULL,
    
    -- Bounding box (normalized 0-1 coordinates)
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_width REAL NOT NULL,
    bbox_height REAL NOT NULL,
    
    -- Face embedding (512-dimensional vector, stored as blob)
    embedding BLOB NOT NULL,
    
    -- Clustering
    cluster_id INTEGER,                -- NULL = unassigned
    confidence REAL,                   -- Detection confidence
    
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES face_clusters(id) ON DELETE SET NULL
);

-- Face clusters (a person)
CREATE TABLE face_clusters (
    id INTEGER PRIMARY KEY,
    name TEXT,                         -- NULL = unnamed, user sets this
    representative_face_id INTEGER,    -- Best face for this cluster (for display)
    face_count INTEGER DEFAULT 0,
    
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (representative_face_id) REFERENCES faces(id) ON DELETE SET NULL
);

-- Duplicate groups
CREATE TABLE duplicate_groups (
    id INTEGER PRIMARY KEY,
    group_hash TEXT,                   -- Shared hash or perceptual hash
    duplicate_type TEXT NOT NULL,      -- 'exact' | 'perceptual'
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE duplicate_group_members (
    id INTEGER PRIMARY KEY,
    group_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    is_suggested_keep BOOLEAN DEFAULT FALSE,  -- Our recommendation
    
    FOREIGN KEY (group_id) REFERENCES duplicate_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

-- Burst groups (photos taken within seconds)
CREATE TABLE burst_groups (
    id INTEGER PRIMARY KEY,
    start_time DATETIME NOT NULL,
    end_time DATETIME NOT NULL,
    photo_count INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE burst_group_members (
    id INTEGER PRIMARY KEY,
    group_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    sharpness_score REAL,             -- Higher = sharper
    blur_score REAL,                  -- Lower = less blur
    face_count INTEGER,               -- More faces might be better
    is_suggested_best BOOLEAN DEFAULT FALSE,
    
    FOREIGN KEY (group_id) REFERENCES burst_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

-- Trash staging
CREATE TABLE trash (
    id INTEGER PRIMARY KEY,
    photo_id INTEGER NOT NULL,
    original_path TEXT NOT NULL,
    trashed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX idx_photos_date ON photos(date_taken);
CREATE INDEX idx_photos_hash ON photos(file_hash);
CREATE INDEX idx_photos_location ON photos(location_country, location_city);
CREATE INDEX idx_photos_trashed ON photos(is_trashed);
CREATE INDEX idx_faces_cluster ON faces(cluster_id);
CREATE INDEX idx_faces_photo ON faces(photo_id);
```

---

## Feature Specifications

### 1. Directory Scanning

**Purpose:** Recursively discover all photos on a mounted drive.

**Supported Formats:**
- JPEG (.jpg, .jpeg)
- PNG (.png)
- HEIC (.heic, .heif)
- WebP (.webp)

**Flow:**
1. User launches app
2. App detects mounted external drives (or user selects a folder)
3. User clicks "Index This Drive"
4. Scanner walks directory tree recursively
5. For each supported file:
   - Calculate SHA256 hash
   - Extract basic file metadata (size, mtime)
   - Add to processing queue
6. Progress shown: "Scanning... Found 45,230 photos"

**Skip Patterns:**
- Hidden files/folders (starting with `.`)
- System folders (`System Volume Information`, `$RECYCLE.BIN`, `.Trash`)
- Files under 10KB (likely thumbnails/icons)

**UI Elements:**
- Drive selector dropdown (shows mounted drives)
- "Select Folder" button for manual selection
- Progress bar with count
- Cancel button
- "Scanning [current_folder]..." status text

**Performance Target:** 
- Scan 100,000 files in under 60 seconds (IO bound, not processing)

---

### 2. EXIF Extraction

**Purpose:** Extract rich metadata from each photo.

**Extracted Fields:**
| Field | EXIF Tag | Fallback |
|-------|----------|----------|
| Date Taken | DateTimeOriginal | DateTime > File mtime |
| GPS Latitude | GPSLatitude + GPSLatitudeRef | None |
| GPS Longitude | GPSLongitude + GPSLongitudeRef | None |
| Camera Make | Make | None |
| Camera Model | Model | None |
| Width | PixelXDimension | Image decode |
| Height | PixelYDimension | Image decode |
| Orientation | Orientation | 1 (normal) |

**Date Parsing Priority:**
1. EXIF DateTimeOriginal
2. EXIF DateTime
3. Filename patterns (e.g., `IMG_20190315_143022.jpg` → 2019-03-15 14:30:22)
4. File modification time

**Filename Date Patterns to Recognize:**
- `IMG_YYYYMMDD_HHMMSS`
- `YYYY-MM-DD HH.MM.SS`
- `YYYYMMDD_HHMMSS`
- `Screenshot_YYYYMMDD-HHMMSS`

**Flow:**
1. Photo added to processing queue from scanner
2. Worker thread picks up photo
3. Read EXIF data using `kamadak-exif`
4. Parse and validate each field
5. If date not in EXIF, try filename parsing
6. Store in database

**Error Handling:**
- Corrupted EXIF: Log warning, continue with available data
- Unreadable file: Mark as `processing_error`, skip
- No date found: Use file mtime, mark `date_taken_source = 'mtime'`

---

### 3. Offline Reverse Geocoding

**Purpose:** Convert GPS coordinates to human-readable locations without internet.

**Data Source:** 
- GeoNames dataset (cities with population > 1000)
- Shipped as SQLite database (~50MB) bundled with app

**Geocoding Database Schema:**
```sql
CREATE TABLE geonames (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    country_code TEXT NOT NULL,
    country_name TEXT NOT NULL,
    latitude REAL NOT NULL,
    longitude REAL NOT NULL,
    population INTEGER
);

CREATE INDEX idx_geonames_coords ON geonames(latitude, longitude);
```

**Algorithm:**
1. Photo has GPS coordinates (lat, lng)
2. Query: Find nearest city using Haversine distance
3. Return (city_name, country_name)

**Optimization:**
- Spatial index using R-tree if SQLite R-tree extension available
- Otherwise, bounding box pre-filter then distance calculation

**Flow:**
1. During EXIF processing, if GPS coordinates present
2. Query geocoding database
3. Store `location_city` and `location_country` in photos table

**Edge Cases:**
- Coordinates in ocean: Return nearest coastal city or "Unknown"
- Coordinates at (0, 0): Likely invalid GPS, store as NULL

---

### 4. Timeline View

**Purpose:** Primary interface for browsing photos chronologically.

**Hierarchy:**
```
Year (2019)
  └── Month (March)
        └── Day (15)
              └── Photos (grid)
```

**UI Layout:**
```
┌─────────────────────────────────────────────────────────────┐
│  [<] March 2019 [>]                        [Timeline] [Grid] │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ═══ March 15, 2019 ═══════════════════ Tokyo, Japan ══════ │
│                                                              │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐   │
│  │     │ │     │ │     │ │     │ │     │ │     │ │     │   │
│  │ IMG │ │ IMG │ │ IMG │ │ IMG │ │ IMG │ │ IMG │ │ IMG │   │
│  │     │ │     │ │     │ │     │ │     │ │     │ │     │   │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘   │
│                                                              │
│  ═══ March 14, 2019 ═══════════════════ Tokyo, Japan ══════ │
│                                                              │
│  ┌─────┐ ┌─────┐ ┌─────┐                                    │
│  │     │ │     │ │     │                                    │
│  │ IMG │ │ IMG │ │ IMG │                                    │
│  │     │ │     │ │     │                                    │
│  └─────┘ └─────┘ └─────┘                                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Navigation:**
- Scroll: Smooth scroll through days
- Month navigation: Arrow buttons or dropdown
- Year navigation: Click year header or dropdown
- Jump to date: Date picker

**Zoom Levels:**
1. **Year view:** Grid of months, each showing cover photo + count
2. **Month view:** Days listed with photo grids (default)
3. **Day view:** All photos from that day, larger thumbnails

**Keyboard Shortcuts:**
- `←` / `→`: Navigate between photos
- `↑` / `↓`: Navigate between days
- `Home`: Jump to oldest photo
- `End`: Jump to newest photo
- `G`: Open "Go to date" dialog

**Day Header:**
- Shows date in locale format
- Shows primary location (most common location that day)
- Shows photo count

**Performance:**
- Virtual scrolling: Only render visible thumbnails
- Thumbnail cache: Generate 300px thumbnails, cache on disk
- Lazy loading: Load full image only on click/hover

**Thumbnail Storage:**
```
<drive_root>/.photovault/
  ├── photovault.db
  └── thumbnails/
      ├── ab/
      │   └── ab3f8c...d4.jpg  (first 2 chars of hash as folder)
      └── cd/
          └── cd9a1b...e7.jpg
```

---

### 5. Face Detection

**Purpose:** Locate all faces in each photo.

**Model:** SCRFD (Sample and Computation Redistribution for Face Detection)
- ONNX format
- Input: 640x640 RGB image
- Output: Bounding boxes + confidence scores + 5-point landmarks

**Flow:**
1. Photo enters face processing queue
2. Load image, resize maintaining aspect ratio (max 640px on longest side)
3. Pad to 640x640
4. Run SCRFD inference
5. For each detected face (confidence > 0.5):
   - Store normalized bounding box (0-1 coordinates)
   - Crop face region with margin (20% padding)
   - Pass to face embedding model
6. Mark photo as `faces_processed = TRUE`

**Bounding Box Storage:**
- Stored as normalized coordinates (0-1 range relative to image dimensions)
- Allows correct rendering regardless of display size

**Confidence Threshold:**
- Detection threshold: 0.5 (adjustable in settings)
- Lower = more faces detected, more false positives
- Higher = fewer false positives, might miss some faces

**Performance:**
- Batch processing: Process multiple photos in parallel
- GPU acceleration: Use CUDA/Metal if available, CPU fallback
- Target: ~10-20 photos/second on modern CPU

---

### 6. Face Clustering

**Purpose:** Group faces of the same person together automatically.

**Model:** ArcFace (for generating face embeddings)
- ONNX format
- Input: 112x112 aligned face image
- Output: 512-dimensional embedding vector

**Embedding Generation:**
1. Take face crop from detection
2. Align face using 5-point landmarks (eyes, nose, mouth corners)
3. Resize to 112x112
4. Normalize pixel values
5. Run ArcFace inference
6. Store 512-float embedding as BLOB in database

**Clustering Algorithm:** DBSCAN or Chinese Whispers
- Distance metric: Cosine similarity on embeddings
- Threshold: 0.6 similarity = same person (tunable)

**Flow:**
1. All faces with embeddings but no cluster assignment
2. Load embeddings into memory
3. Run clustering algorithm
4. Assign cluster IDs to faces
5. For each cluster:
   - Calculate face count
   - Select representative face (highest detection confidence)
   - Create/update `face_clusters` record

**Re-clustering Triggers:**
- New photos indexed (incremental: only cluster new faces against existing)
- User merges clusters (update assignments)
- User manually corrects a face assignment

**Handling Growth:**
- Initial clustering: Full DBSCAN on all faces
- Incremental: For new faces, find nearest cluster centroid
  - If distance < threshold: Assign to cluster
  - If distance > threshold: Create new cluster or leave unassigned

---

### 7. Face Labeling + Cluster Merge

**Purpose:** Let users name clusters and fix clustering mistakes.

**UI - People View:**
```
┌─────────────────────────────────────────────────────────────┐
│  People                                          [Search]    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        │
│  │  ┌───┐  │  │  ┌───┐  │  │  ┌───┐  │  │  ┌───┐  │        │
│  │  │ o │  │  │  │ o │  │  │  │ o │  │  │  │ o │  │        │
│  │  └───┘  │  │  └───┘  │  │  └───┘  │  │  └───┘  │        │
│  │   Dad   │  │   Mom   │  │ Person 3│  │ Person 4│        │
│  │ 1,234   │  │   892   │  │   456   │  │   234   │        │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Naming Flow:**
1. User clicks on unnamed cluster ("Person 3")
2. Opens cluster detail view showing all photos with this face
3. User clicks "Add Name" button
4. Text input appears
5. User types name, presses Enter
6. Cluster updated with name

**Merge Flow:**
1. User notices two clusters are the same person
2. User selects first cluster
3. Clicks "Merge with..."
4. Selects second cluster
5. Confirmation dialog: "Merge 'Person 3' (456 photos) into 'Dad' (1,234 photos)?"
6. User confirms
7. All faces from second cluster reassigned to first
8. Second cluster deleted
9. Face count updated

**Merge Algorithm:**
```
1. source_cluster = cluster being merged away
2. target_cluster = cluster being merged into
3. UPDATE faces SET cluster_id = target_cluster.id WHERE cluster_id = source_cluster.id
4. UPDATE face_clusters SET face_count = (SELECT COUNT(*) FROM faces WHERE cluster_id = target_cluster.id)
5. DELETE FROM face_clusters WHERE id = source_cluster.id
```

**UI for Cluster Detail:**
```
┌─────────────────────────────────────────────────────────────┐
│  [<] Dad (1,234 photos)              [Rename] [Merge] [Delete] │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Showing all photos containing Dad                          │
│                                                              │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐   │
│  │     │ │     │ │     │ │     │ │     │ │     │ │     │   │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Delete Cluster:**
- Doesn't delete photos
- Unassigns all faces (sets `cluster_id = NULL`)
- Faces return to "Unnamed" pool

---

### 8. Exact Duplicate Detection

**Purpose:** Find identical files scattered across folders.

**Method:** SHA256 hash comparison

**Flow:**
1. During indexing, SHA256 hash calculated for each file
2. After indexing complete, query for duplicate hashes:
   ```sql
   SELECT file_hash, COUNT(*) as count 
   FROM photos 
   GROUP BY file_hash 
   HAVING count > 1
   ```
3. For each duplicate hash, create `duplicate_groups` entry
4. Add all photos with that hash as `duplicate_group_members`
5. Suggest keeping: Oldest file (likely original) or largest file

**UI - Duplicates View:**
```
┌─────────────────────────────────────────────────────────────┐
│  Duplicates                        Found 234 duplicate groups │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Group 1 - 3 identical files                             │ │
│  │                                                         │ │
│  │  ┌─────┐   /Photos/2019/IMG_001.jpg (2.3 MB) ★ KEEP    │ │
│  │  │     │   /Backup/old/IMG_001.jpg (2.3 MB)            │ │
│  │  └─────┘   /Copy/IMG_001.jpg (2.3 MB)                  │ │
│  │                                                         │ │
│  │  [Keep Suggested] [Keep All] [Review Individually]      │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ Group 2 - 2 identical files                             │ │
│  │  ...                                                    │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Actions:**
- **Keep Suggested:** Move all except suggested file to trash
- **Keep All:** Dismiss this group, keep all copies
- **Review Individually:** Expand to manually select which to keep

**Keep Suggestion Logic:**
1. Prefer files in folders not named "backup", "copy", "old"
2. Prefer oldest file by date taken
3. If tie, prefer shortest path (likely better organized)

---

### 9. Burst Detection

**Purpose:** Group photos taken within seconds of each other (rapid shooting).

**Algorithm:**
1. Query photos ordered by `date_taken`
2. Iterate through, grouping photos within 3-second window
3. Groups with 3+ photos = burst group

```python
# Pseudocode
current_group = []
for photo in photos_by_date:
    if current_group is empty:
        current_group.append(photo)
    elif photo.date_taken - current_group[-1].date_taken <= 3 seconds:
        current_group.append(photo)
    else:
        if len(current_group) >= 3:
            save_burst_group(current_group)
        current_group = [photo]
```

**Flow:**
1. Run after initial indexing
2. Re-run incrementally when new photos added
3. Store groups in `burst_groups` table

---

### 10. Best-Pick Suggestion

**Purpose:** Recommend the best photo from a burst to keep.

**Scoring Metrics:**

1. **Sharpness Score** (Laplacian variance)
   - Higher variance = sharper image
   - Calculate: Convert to grayscale, apply Laplacian filter, compute variance
   
2. **Blur Score** (inverse of motion blur detection)
   - FFT-based blur detection
   - Lower score = less blur

3. **Face Count**
   - More detected faces might indicate better group shot
   
4. **Face Quality** (if faces detected)
   - Eyes open detection (optional, complex)
   - Face detection confidence as proxy

**Combined Score:**
```
score = (sharpness_normalized * 0.4) + 
        (blur_score_normalized * 0.3) + 
        (face_confidence_avg * 0.2) +
        (face_count_normalized * 0.1)
```

**Flow:**
1. For each burst group
2. Calculate scores for each photo in group
3. Highest score = suggested best
4. Set `is_suggested_best = TRUE`

**UI Integration:**
- Burst groups shown with badge: "5 similar"
- Click to expand
- Best pick highlighted with star
- User can override suggestion

---

### 11. Quick Cull Workflow

**Purpose:** Rapidly review photos, decide keep/delete.

**This is a dedicated mode optimized for speed.**

**UI - Cull Mode:**
```
┌─────────────────────────────────────────────────────────────┐
│  Cull Mode - March 15, 2019                    [Exit Cull]   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│                    ┌─────────────────┐                       │
│                    │                 │                       │
│                    │                 │                       │
│                    │   [LARGE IMG]   │                       │
│                    │                 │                       │
│                    │                 │                       │
│                    └─────────────────┘                       │
│                                                              │
│                    IMG_4521.jpg                              │
│                    15 of 89 in this day                      │
│                                                              │
│  ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐    │
│  │ x │ │   │ │▶  │ │   │ │ x │ │   │ │   │ │   │ │   │    │
│  └───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘    │
│    ↑                                                        │
│  marked for deletion                                        │
│                                                              │
│  [←] Previous    [X] Trash    [→] Next    [Space] Toggle    │
│                                                              │
│  Marked: 12 for deletion                      [Finish Cull] │
└─────────────────────────────────────────────────────────────┘
```

**Keyboard Controls:**
| Key | Action |
|-----|--------|
| `←` or `A` | Previous photo |
| `→` or `D` | Next photo |
| `X` | Mark for trash |
| `Space` | Toggle trash mark |
| `U` | Undo last action |
| `Enter` | Finish cull, review marked |
| `Esc` | Exit cull mode |
| `1-5` | Rate photo (optional feature) |
| `Z` | Zoom in |

**Entry Points:**
- "Cull this day" button in timeline
- "Cull this burst" button in burst view
- "Cull duplicates" in duplicate view
- Global "Cull unreviewed" in menu

**Flow:**
1. User enters cull mode for a set of photos
2. First photo displayed large
3. Filmstrip below shows context (X marks on trashed)
4. User navigates with keyboard, marks unwanted
5. User presses Enter to finish
6. Summary shown: "12 photos marked. Move to trash?"
7. User confirms → photos move to trash staging

**State Tracking:**
- Cull session stored in memory (not DB)
- Only commits to trash on explicit confirmation

---

### 12. Trash Staging

**Purpose:** Safe deletion with recovery option.

**Trash Behavior:**
- Deleted photos NOT immediately removed from disk
- Marked as `is_trashed = TRUE` in database
- Hidden from normal views
- Recoverable until permanent delete

**Trash View UI:**
```
┌─────────────────────────────────────────────────────────────┐
│  Trash (45 photos)                            [Empty Trash]  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  These photos will be permanently deleted after 30 days     │
│  or when you empty trash manually.                          │
│                                                              │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐          │
│  │     │ │     │ │     │ │     │ │     │ │     │          │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘          │
│                                                              │
│  [Restore Selected]                   [Delete Selected Now]  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Operations:**
- **Restore:** Set `is_trashed = FALSE`, remove from trash table
- **Permanent Delete:** Actually delete file from disk, remove from all tables
- **Empty Trash:** Permanent delete all trashed photos
- **Auto-cleanup:** Option to auto-delete after 30 days (configurable)

**Permanent Delete Flow:**
1. User clicks "Delete Now" or "Empty Trash"
2. Confirmation: "Permanently delete 45 photos? This cannot be undone."
3. User confirms
4. For each photo:
   - Delete file from disk using `std::fs::remove_file`
   - Delete thumbnail
   - Delete from database (cascades to faces, etc.)
5. Show result: "45 photos permanently deleted"

**Safety:**
- Double confirmation for permanent delete
- Show file paths in confirmation dialog
- Log deletions to separate audit log file

---

### 13. Search

**Purpose:** Find photos by date, location, or person.

**Search UI:**
```
┌─────────────────────────────────────────────────────────────┐
│  🔍 [Search photos...                                    ]   │
│                                                              │
│  Recent: Tokyo | Dad | March 2019 | Beach                   │
├─────────────────────────────────────────────────────────────┤
```

**Search Types:**

1. **By Person Name**
   - Input: "Dad"
   - Query: Find cluster where name LIKE '%Dad%'
   - Return: All photos with faces in that cluster

2. **By Location**
   - Input: "Tokyo" or "Japan"
   - Query: `WHERE location_city LIKE '%Tokyo%' OR location_country LIKE '%Tokyo%'`
   - Return: Matching photos

3. **By Date**
   - Input: "March 2019" or "2019-03-15"
   - Parse natural language dates
   - Query: `WHERE date_taken BETWEEN start AND end`

4. **Combined**
   - Input: "Dad in Tokyo"
   - Parse: person=Dad, location=Tokyo
   - Query: Intersection of both result sets

**Natural Language Date Parsing:**
| Input | Interpretation |
|-------|----------------|
| "March 2019" | 2019-03-01 to 2019-03-31 |
| "2019" | Full year |
| "last summer" | June-August of previous year |
| "yesterday" | Literal yesterday |
| "March 15" | March 15 of current year |

**Search Results UI:**
```
┌─────────────────────────────────────────────────────────────┐
│  Results for "Dad in Tokyo" - 156 photos                    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  (Same grid view as timeline, grouped by date)              │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Performance:**
- SQLite FTS5 for text search on location fields
- Indexed queries for date ranges
- Person search via cluster_id lookup

---

### 14. SQLite Database on Drive

**Purpose:** Portable metadata that travels with photos.

**Location:**
```
/Volumes/MyPhotoDrive/
  ├── .photovault/
  │   ├── photovault.db        # Main database
  │   ├── photovault.db-wal    # Write-ahead log
  │   ├── photovault.db-shm    # Shared memory
  │   ├── geocoding.db         # Bundled geocoding data (copied on first use)
  │   └── thumbnails/          # Generated thumbnails
  │       ├── ab/
  │       │   └── ab3f8c...jpg
  │       └── ...
  ├── Photos/
  │   └── (user's photos)
  └── ...
```

**Why `.photovault/` folder:**
- Hidden on Unix systems (dot prefix)
- All app data in one place
- Easy to exclude from other backups if desired
- Clear what it is if discovered

**Database Pragmas:**
```sql
PRAGMA journal_mode=WAL;          -- Better concurrent read performance
PRAGMA synchronous=NORMAL;        -- Balance between safety and speed
PRAGMA cache_size=-64000;         -- 64MB cache
PRAGMA temp_store=MEMORY;         -- Temp tables in memory
PRAGMA mmap_size=268435456;       -- Memory-map up to 256MB
```

**Portability:**
1. User has Drive A indexed on Windows PC
2. User plugs Drive A into Mac laptop
3. App on Mac detects `.photovault/` folder
4. Opens database, ready immediately
5. Thumbnails already generated, instant browsing

**Multi-drive Handling:**
- Each drive has its own `.photovault/` database
- App can show "All Drives" unified view (queries multiple DBs)
- Or single-drive view

**Database Versioning:**
```sql
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```
- On open, check version, run migrations if needed

---

### 15. Incremental Re-indexing

**Purpose:** Efficiently detect changes without full rescan.

**Change Detection:**

1. **New files:**
   - Walk directory tree
   - Compare paths against database
   - New path = new file to index

2. **Deleted files:**
   - Query all paths from database
   - Check each exists on disk
   - Missing = mark for cleanup

3. **Moved files:**
   - File missing at old path
   - File with same hash found at new path
   - Update path in database

4. **Modified files:**
   - Compare file modification time with `updated_at`
   - If file newer, re-extract EXIF (might have been edited)

**Flow:**
1. User clicks "Refresh" or app auto-checks on launch
2. Quick scan: Walk directories, collect all paths + mtimes
3. Compare against database
4. Generate change list: added[], removed[], moved[], modified[]
5. Process changes:
   - Added: Full indexing pipeline
   - Removed: Mark as deleted or remove from DB
   - Moved: Update path
   - Modified: Re-extract metadata

**Optimization:**
- Store directory mtime: If directory unchanged, skip its contents
- Bloom filter for quick "does this path exist in DB?" checks

**UI:**
```
┌────────────────────────────────────────┐
│  Checking for changes...               │
│                                        │
│  Found:                                │
│    + 45 new photos                     │
│    - 3 deleted                         │
│    ~ 12 moved                          │
│                                        │
│  [Process Changes]  [Ignore for Now]   │
└────────────────────────────────────────┘
```

---

## Application Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Tauri App                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │                 React Frontend                       │    │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │    │
│  │  │Timeline │ │ People  │ │ Search  │ │  Cull   │   │    │
│  │  │  View   │ │  View   │ │  View   │ │  Mode   │   │    │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘   │    │
│  └─────────────────────────────────────────────────────┘    │
│                            │                                │
│                      Tauri IPC                              │
│                            │                                │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                  Rust Backend                        │    │
│  │                                                      │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │    │
│  │  │ Scanner  │ │   EXIF   │ │   Face   │            │    │
│  │  │          │ │ Extractor│ │ Detector │            │    │
│  │  └──────────┘ └──────────┘ └──────────┘            │    │
│  │                                                      │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │    │
│  │  │ Geocoder │ │ Clusterer│ │  Search  │            │    │
│  │  │          │ │          │ │  Engine  │            │    │
│  │  └──────────┘ └──────────┘ └──────────┘            │    │
│  │                                                      │    │
│  │  ┌──────────┐ ┌──────────┐                         │    │
│  │  │ Database │ │    ML    │                         │    │
│  │  │  (SQLite)│ │ (ONNX RT)│                         │    │
│  │  └──────────┘ └──────────┘                         │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

**Key Crates:**
```toml
[dependencies]
tauri = "2"
rusqlite = { version = "0.31", features = ["bundled"] }
ort = "2.0"                    # ONNX Runtime
image = "0.25"
kamadak-exif = "0.5"
walkdir = "2"
sha2 = "0.10"
rayon = "1.10"                 # Parallel processing
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
```

---

## Processing Pipeline

```
┌─────────────────────────────────────────────────────────────┐
│                    Processing Pipeline                       │
└─────────────────────────────────────────────────────────────┘

  ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
  │  Scan   │────▶│  EXIF   │────▶│Thumbnail│────▶│  Face   │
  │         │     │ Extract │     │ Generate│     │ Detect  │
  └─────────┘     └─────────┘     └─────────┘     └─────────┘
                                                       │
                                                       ▼
  ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
  │  Done   │◀────│  Burst  │◀────│Duplicate│◀────│  Face   │
  │         │     │ Detect  │     │ Detect  │     │ Cluster │
  └─────────┘     └─────────┘     └─────────┘     └─────────┘
```

**Parallelization:**
- Scanner: Single-threaded (IO bound)
- EXIF extraction: Multi-threaded (CPU bound)
- Thumbnail generation: Multi-threaded (CPU bound)
- Face detection: Batched, GPU if available
- Face clustering: Single-threaded (needs all embeddings)
- Duplicate detection: Single DB query
- Burst detection: Single DB query

---

## Settings

Stored in app config (not on drive):

```json
{
  "theme": "system",
  "thumbnail_size": 300,
  "face_detection_confidence": 0.5,
  "face_clustering_threshold": 0.6,
  "burst_time_window_seconds": 3,
  "trash_auto_delete_days": 30,
  "scan_hidden_folders": false,
  "date_format": "locale",
  "remembered_drives": [
    "/Volumes/MyPhotoDrive",
    "/Volumes/Backup2019"
  ]
}
```

---

## Performance Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| Initial scan (100k files) | < 2 minutes | Path + hash only |
| EXIF extraction (100k files) | < 10 minutes | Parallel |
| Thumbnail generation (100k) | < 30 minutes | 300px, parallel |
| Face detection (100k) | < 60 minutes | GPU accelerated |
| Face clustering (500k faces) | < 5 minutes | After embeddings ready |
| Timeline scroll | 60 FPS | Virtual scrolling |
| Search results | < 500ms | Indexed queries |
| App launch (indexed drive) | < 3 seconds | DB already built |

---

## Error Handling

| Error | Handling |
|-------|----------|
| Corrupted image | Log, skip, mark as error in DB |
| EXIF read failure | Use fallbacks, continue |
| Face detection OOM | Reduce batch size, retry |
| Database corruption | Backup detection, offer rebuild |
| Drive disconnected | Graceful pause, prompt reconnect |
| Insufficient permissions | Warn user, skip inaccessible folders |

---

## Security Considerations

- **No network calls** (except optional update check)
- **No telemetry**
- **Database is local SQLite** (no credentials)
- **Face embeddings are not reversible** (can't reconstruct face from embedding)
- **Thumbnails stored locally** (same privacy as originals)

---

## Future-Proofing

Database schema designed to accommodate Phase 2:
- `photos` table has room for additional tags
- Separate tables for albums, scenes, etc.
- Embedding storage pattern reusable for CLIP embeddings

---

This specification should be sufficient to implement the complete MVP. All features, flows, data structures, and UI layouts are defined.
