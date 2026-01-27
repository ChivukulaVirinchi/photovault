# Phase 2 - Post MVP Specification

## Overview

Phase 2 extends PhotoVault with advanced features that enhance discovery, organization, and usability. These features are valuable but not essential for the core "browse your photos offline" experience.

**Prerequisite:** All Phase 1 features must be complete and stable.

---

## Feature Specifications

### 1. Map View

**Purpose:** Visualize photos geographically on a world map.

**What it does:**
- Displays an interactive map
- Photos plotted as markers/clusters at their GPS coordinates
- Click a cluster to see photos from that location
- Zoom in to see individual photo markers

**UI Layout:**
```
┌─────────────────────────────────────────────────────────────┐
│  Map View                                  [Filters] [List]  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│         ┌────────────────────────────────────────┐          │
│         │                                        │          │
│         │              WORLD MAP                 │          │
│         │                                        │          │
│         │    ○ (23)                              │          │
│         │         ○ (156)    ○ (8)               │          │
│         │                          ○ (45)       │          │
│         │                                        │          │
│         └────────────────────────────────────────┘          │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Selected: Tokyo, Japan (156 photos)                    │ │
│  │ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ►    │ │
│  │ │     │ │     │ │     │ │     │ │     │ │     │       │ │
│  │ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘       │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Map Library Options:**
- **Leaflet** (via JS): Open source, offline tile support
- **MapLibre GL**: Vector tiles, smooth zoom, offline capable
- Recommendation: MapLibre GL for performance

**Offline Map Tiles:**
- Ship basic world map tiles with app (~100-200MB)
- Or download on-demand and cache
- User can pre-download regions for offline use

**Clustering Algorithm:**
- At low zoom: Cluster photos within ~100km radius
- Show cluster count as badge
- At high zoom: Individual markers
- Use supercluster algorithm for performance

**Data Requirements:**
- Only photos with GPS coordinates shown
- Query: `SELECT * FROM photos WHERE gps_latitude IS NOT NULL`

**Interactions:**
| Action | Result |
|--------|--------|
| Click cluster | Zoom in + show photo strip below |
| Click single marker | Show photo detail |
| Drag map | Pan view |
| Scroll | Zoom in/out |
| Click photo in strip | Open full photo view |

**Filters (optional):**
- By date range
- By person (show only photos with Dad)
- By year

**Database Additions:**
None required - uses existing `gps_latitude`, `gps_longitude` from `photos` table.

**Performance Considerations:**
- For 100k+ photos with GPS, clustering must be performant
- Pre-compute clusters at various zoom levels? Or real-time with spatial index
- Consider storing cluster assignments in DB for fast load

---

### 2. Memories

**Purpose:** Surface nostalgic "on this day in past years" photos.

**What it does:**
- On app launch, check if there are photos from this date in previous years
- Show a "Memories" card: "3 years ago in Tokyo"
- User can browse through past years' photos from today's date

**UI - Home Screen Card:**
```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  ✨ Memories                                           │ │
│  │                                                        │ │
│  │  ┌──────────────┐                                     │ │
│  │  │              │  3 years ago                        │ │
│  │  │   [PHOTO]    │  March 15, 2022                     │ │
│  │  │              │  Tokyo, Japan                       │ │
│  │  └──────────────┘                                     │ │
│  │                                                        │ │
│  │  12 photos from this day  ──────────────────►         │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Continue browsing...                                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Memories Detail View:**
```
┌─────────────────────────────────────────────────────────────┐
│  [<] Memories from March 15                                  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ═══ 2022 (3 years ago) ═══════════════════ Tokyo ═════════ │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐                           │
│  │     │ │     │ │     │ │     │                           │
│  └─────┘ └─────┘ └─────┘ └─────┘                           │
│                                                              │
│  ═══ 2019 (6 years ago) ═══════════════════ Home ══════════ │
│  ┌─────┐ ┌─────┐                                            │
│  │     │ │     │                                            │
│  └─────┘ └─────┘                                            │
│                                                              │
│  ═══ 2015 (10 years ago) ══════════════════ Beach ═════════ │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐          │
│  │     │ │     │ │     │ │     │ │     │ │     │          │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Algorithm:**
```sql
-- Get memories for today (March 15)
SELECT * FROM photos 
WHERE strftime('%m-%d', date_taken) = '03-15'
  AND strftime('%Y', date_taken) < strftime('%Y', 'now')
  AND is_trashed = FALSE
ORDER BY date_taken DESC;
```

**Memory Selection Logic:**
1. Query photos from this day across all years
2. Group by year
3. For display card, pick "best" photo:
   - Prefer photos with faces
   - Prefer photos with location
   - Prefer landscape orientation
4. If multiple years have photos, show most recent first

**Notification (optional):**
- On app launch, if memories exist, show subtle notification
- Don't be annoying - user can dismiss or disable

**Edge Cases:**
- Feb 29 (leap year): Show Feb 28 memories on non-leap years
- No memories: Don't show the card at all

---

### 3. Scene/Object Tagging

**Purpose:** Auto-tag photos with what's in them (beach, dog, food, wedding).

**What it does:**
- Uses CLIP or similar vision model to generate semantic tags
- Enables searches like "show me all beach photos"
- Tags shown on photo detail view

**Model:** OpenAI CLIP (ViT-B/32) or similar
- ONNX format for local inference
- ~350MB model size
- Input: 224x224 image
- Output: 512-dimensional embedding

**How CLIP Tagging Works:**
1. Generate image embedding using CLIP vision encoder
2. Compare against pre-computed text embeddings for tag categories
3. Tags with similarity > threshold get assigned

**Tag Categories (predefined):**
```
Scenes:
- beach, mountain, forest, city, street, park, garden
- indoor, outdoor, night, sunset, sunrise
- restaurant, cafe, airport, hotel

Events:
- wedding, birthday, graduation, party, concert
- vacation, road trip, hiking

Objects:
- car, bicycle, motorcycle, boat, airplane
- food, drink, cake, coffee
- computer, phone, book

Animals:
- dog, cat, bird, horse, fish

Activities:
- swimming, running, cycling, skiing
- cooking, reading, working, playing
```

**Database Additions:**
```sql
CREATE TABLE photo_tags (
    id INTEGER PRIMARY KEY,
    photo_id INTEGER NOT NULL,
    tag TEXT NOT NULL,
    confidence REAL NOT NULL,
    source TEXT DEFAULT 'clip',  -- 'clip' | 'manual'
    
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX idx_photo_tags_tag ON photo_tags(tag);
CREATE INDEX idx_photo_tags_photo ON photo_tags(photo_id);
```

**Processing Flow:**
1. Photo enters tagging queue (after face detection)
2. Load image, resize to 224x224
3. Run CLIP vision encoder → image embedding
4. Compare against all tag text embeddings (cosine similarity)
5. For each tag with similarity > 0.25:
   - Insert into `photo_tags` with confidence
6. Mark photo as tagged

**UI - Search Integration:**
```
┌─────────────────────────────────────────────────────────────┐
│  🔍 [beach vacation                                      ]   │
│                                                              │
│  Suggestions: beach (234) | vacation (567) | beach sunset   │
├─────────────────────────────────────────────────────────────┤
```

**UI - Photo Detail Tags:**
```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│                    ┌─────────────────┐                       │
│                    │                 │                       │
│                    │     [PHOTO]     │                       │
│                    │                 │                       │
│                    └─────────────────┘                       │
│                                                              │
│  March 15, 2019 • Tokyo, Japan                              │
│                                                              │
│  Tags: [beach] [sunset] [outdoor]                           │
│  People: [Dad] [Mom]                                         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Manual Tagging:**
- User can add custom tags
- User can remove incorrect auto-tags
- Manual tags marked with `source = 'manual'`

**Performance:**
- CLIP inference: ~50-100ms per image on CPU
- Batch processing: 10 images at a time
- Run as background task, lower priority than face detection

---

### 4. Albums

**Purpose:** Manual organization of photos into named collections.

**What it does:**
- User creates named albums
- User adds photos to albums
- Photos can be in multiple albums
- Album has cover photo, name, description

**Database Additions:**
```sql
CREATE TABLE albums (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    cover_photo_id INTEGER,
    photo_count INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
);

CREATE TABLE album_photos (
    id INTEGER PRIMARY KEY,
    album_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    added_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    sort_order INTEGER,
    
    FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(album_id, photo_id)
);
```

**UI - Albums View:**
```
┌─────────────────────────────────────────────────────────────┐
│  Albums                                      [+ New Album]   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │             │  │             │  │             │         │
│  │   [COVER]   │  │   [COVER]   │  │   [COVER]   │         │
│  │             │  │             │  │             │         │
│  │ Japan 2019  │  │  Wedding    │  │ Kids Growth │         │
│  │   156 📷    │  │   423 📷    │  │   892 📷    │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Create Album Flow:**
1. User clicks "+ New Album"
2. Modal appears: Name (required), Description (optional)
3. User enters details, clicks Create
4. Album created, opens empty album view
5. User can add photos via:
   - "Add Photos" button → opens photo picker
   - Drag and drop from timeline
   - Right-click photo in timeline → "Add to Album"

**Album Detail View:**
```
┌─────────────────────────────────────────────────────────────┐
│  [<] Japan 2019                     [Edit] [Share] [Delete]  │
│                                                              │
│  March 10-25, 2019 • 156 photos                             │
│  "Our amazing trip to Tokyo and Kyoto"                      │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐          │
│  │     │ │     │ │     │ │     │ │     │ │     │          │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Album Operations:**
| Action | Behavior |
|--------|----------|
| Edit | Change name, description, cover photo |
| Delete | Delete album only, photos remain |
| Remove photo | Remove from album, photo remains in library |
| Reorder | Drag to reorder photos within album |
| Set cover | Right-click photo → "Set as Cover" |

**Smart Albums (future consideration):**
- Auto-generated based on criteria
- "All photos with Dad"
- "Photos from 2019"
- Not in MVP for Phase 2, but schema supports it

---

### 5. Video Support

**Purpose:** Handle video files alongside photos.

**What it does:**
- Detect and index video files
- Generate thumbnail from video
- Display in timeline alongside photos
- Basic playback within app
- Extract video metadata (duration, resolution, codec)

**Supported Formats:**
- MP4 (.mp4, .m4v)
- MOV (.mov)
- AVI (.avi)
- MKV (.mkv)
- WebM (.webm)

**Database Additions:**
```sql
-- Extend photos table or create separate videos table?
-- Recommendation: Add video fields to photos table with type discriminator

ALTER TABLE photos ADD COLUMN media_type TEXT DEFAULT 'photo';  -- 'photo' | 'video'
ALTER TABLE photos ADD COLUMN duration_seconds REAL;
ALTER TABLE photos ADD COLUMN video_codec TEXT;
ALTER TABLE photos ADD COLUMN audio_codec TEXT;
ALTER TABLE photos ADD COLUMN frame_rate REAL;
```

**Thumbnail Generation:**
- Extract frame at 1 second (or 10% of duration for short videos)
- Use FFmpeg (ship as binary) or `ffmpeg-next` Rust bindings
- Store thumbnail same as photos

**Metadata Extraction:**
- Duration
- Resolution (width x height)
- Codec (H.264, H.265, etc.)
- Frame rate
- Creation date (from metadata or file)

**UI - Timeline:**
```
┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐
│     │ │     │ │ ▶   │ │     │ │ ▶   │
│ IMG │ │ IMG │ │ VID │ │ IMG │ │ VID │
│     │ │     │ │0:45 │ │     │ │2:30 │
└─────┘ └─────┘ └─────┘ └─────┘ └─────┘
```
- Video thumbnails show play icon overlay
- Duration badge in corner

**UI - Video Playback:**
```
┌─────────────────────────────────────────────────────────────┐
│  [<] Video_001.mp4                                    [X]    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│                    ┌─────────────────┐                       │
│                    │                 │                       │
│                    │                 │                       │
│                    │     [VIDEO]     │                       │
│                    │                 │                       │
│                    │                 │                       │
│                    └─────────────────┘                       │
│                                                              │
│  ▶  ━━━━━━━━━━━●━━━━━━━━━━━━━  0:45 / 2:30    🔊 ━━●━━     │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Playback Features:**
- Play/pause
- Seek
- Volume control
- Fullscreen
- Playback speed (0.5x, 1x, 1.5x, 2x)

**Face Detection in Videos:**
- Extract key frames (1 per second or scene changes)
- Run face detection on key frames
- Store faces with frame timestamp
- More complex, consider making optional

**Challenges:**
- Video processing is CPU/GPU intensive
- Large file sizes
- Codec support varies by platform
- Consider using system video player as fallback

---

### 6. Face Cluster Splitting

**Purpose:** Fix incorrect cluster merges where two people got grouped together.

**What it does:**
- User identifies a cluster contains multiple people
- User enters "split mode"
- User selects faces that belong to a different person
- Selected faces moved to new cluster

**UI - Cluster Split Mode:**
```
┌─────────────────────────────────────────────────────────────┐
│  Split Cluster: Dad (1,234 photos)         [Cancel] [Save]   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Select faces that DON'T belong to "Dad"                    │
│                                                              │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐          │
│  │     │ │ ✓   │ │     │ │ ✓   │ │ ✓   │ │     │          │
│  │  😊 │ │  😊 │ │  😊 │ │  😊 │ │  😊 │ │  😊 │          │
│  │     │ │     │ │     │ │     │ │     │ │     │          │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘          │
│                                                              │
│  3 faces selected → will become new cluster                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Split Flow:**
1. User opens cluster detail view
2. Clicks "Split Cluster" button
3. Enters split mode - shows face thumbnails (not full photos)
4. User clicks faces that don't belong
5. User clicks "Save"
6. New cluster created with selected faces
7. Original cluster updated (face count reduced)

**Algorithm:**
```sql
-- Create new cluster
INSERT INTO face_clusters (name, face_count) VALUES (NULL, 0);

-- Get new cluster ID
SET @new_cluster_id = last_insert_rowid();

-- Move selected faces
UPDATE faces SET cluster_id = @new_cluster_id WHERE id IN (selected_face_ids);

-- Update face counts
UPDATE face_clusters SET face_count = (
    SELECT COUNT(*) FROM faces WHERE cluster_id = face_clusters.id
);
```

**UI Considerations:**
- Show face crops, not full photos (easier to spot wrong faces)
- Group visually similar faces together (re-run clustering on this subset?)
- Highlight likely outliers (faces with lowest similarity to cluster centroid)

**Auto-suggest Splits:**
- Calculate each face's distance to cluster centroid
- Faces far from centroid might be wrong
- Show warning: "3 faces seem different from others - review?"

---

### 7. OCR (Optical Character Recognition)

**Purpose:** Extract and search text visible in photos.

**What it does:**
- Detects text in screenshots, documents, signs, etc.
- Stores extracted text in database
- Enables search by text content
- "Find the photo of that restaurant menu"

**Model:** Tesseract OCR or PaddleOCR
- Tesseract: Mature, many languages, ~30MB per language
- PaddleOCR: Better accuracy, ONNX available, ~50MB

**Recommendation:** PaddleOCR for accuracy

**Database Additions:**
```sql
ALTER TABLE photos ADD COLUMN ocr_text TEXT;
ALTER TABLE photos ADD COLUMN ocr_processed BOOLEAN DEFAULT FALSE;

-- Full-text search index
CREATE VIRTUAL TABLE photos_fts USING fts5(
    ocr_text,
    content='photos',
    content_rowid='id'
);
```

**Processing Flow:**
1. Photo enters OCR queue (lower priority than faces)
2. Check if likely to contain text:
   - Aspect ratio near 16:9 or 9:16 (screenshots)
   - Filename contains "screenshot"
   - Or just process all (more thorough)
3. Run OCR model
4. Store extracted text
5. Update FTS index

**Search Integration:**
```sql
-- Search for text in photos
SELECT p.* FROM photos p
JOIN photos_fts fts ON p.id = fts.rowid
WHERE photos_fts MATCH 'restaurant menu'
```

**UI - Search:**
```
┌─────────────────────────────────────────────────────────────┐
│  🔍 [restaurant menu                                     ]   │
│                                                              │
│  Found in text: 12 photos                                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────┐ "...The Restaurant Menu features..." (Screenshot)  │
│  │     │ March 15, 2022                                     │
│  └─────┘                                                    │
│                                                              │
│  ┌─────┐ "...restaurant menu board..." (Photo)             │
│  │     │ July 8, 2021                                       │
│  └─────┘                                                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Languages:**
- Ship with English by default
- Additional language packs downloadable (~30MB each)
- Auto-detect language? Or user setting?

**Performance:**
- OCR is slow (~1-2 seconds per image)
- Run as lowest priority background task
- Only process images likely to have text? (reduces scope)

---

### 8. LAN Sync

**Purpose:** Share the photo index across multiple machines on local network.

**What it does:**
- Machine A has indexed drive connected
- Machine B on same network can browse Machine A's photos
- No cloud involved - direct device-to-device
- Photos streamed on-demand, not copied

**Architecture:**
```
┌─────────────────┐         LAN          ┌─────────────────┐
│   Machine A     │◄──────────────────►  │   Machine B     │
│ (Drive plugged) │     TCP/mDNS         │  (Viewer only)  │
│                 │                       │                 │
│  ┌───────────┐  │                       │  ┌───────────┐  │
│  │ PhotoVault│  │                       │  │ PhotoVault│  │
│  │  Server   │  │                       │  │  Client   │  │
│  └───────────┘  │                       │  └───────────┘  │
│       ↑         │                       │                 │
│  ┌────┴────┐    │                       │                 │
│  │  Drive  │    │                       │                 │
│  └─────────┘    │                       │                 │
└─────────────────┘                       └─────────────────┘
```

**Discovery:** mDNS (Bonjour/Avahi)
- Machine A advertises: `_photovault._tcp.local`
- Machine B discovers available servers
- No manual IP configuration needed

**Protocol:**
- Simple HTTP REST API on local port
- Endpoints:
  - `GET /api/photos` - List photos (paginated)
  - `GET /api/photos/:id` - Photo metadata
  - `GET /api/photos/:id/thumbnail` - Thumbnail image
  - `GET /api/photos/:id/full` - Full resolution image
  - `GET /api/people` - List face clusters
  - `GET /api/search?q=...` - Search

**Security:**
- Optional PIN/password protection
- Only accessible on local network (bind to LAN IP, not 0.0.0.0)
- No encryption (it's local), but could add if paranoid

**UI - Server Mode:**
```
┌─────────────────────────────────────────────────────────────┐
│  Settings > LAN Sharing                                      │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Share this library on local network                        │
│                                                              │
│  [●] Enable sharing                                         │
│                                                              │
│  Status: Sharing as "John's MacBook"                        │
│  Connected viewers: 1 (iPad)                                │
│                                                              │
│  [ ] Require PIN to connect                                 │
│  PIN: [1234    ]                                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**UI - Client Mode:**
```
┌─────────────────────────────────────────────────────────────┐
│  Libraries                                                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Local Drives:                                              │
│  ┌──────────────────────────────────────────────────┐       │
│  │ 📁 My Photo Drive          45,230 photos        │       │
│  └──────────────────────────────────────────────────┘       │
│                                                              │
│  Network Libraries:                                          │
│  ┌──────────────────────────────────────────────────┐       │
│  │ 🌐 John's MacBook          120,456 photos  [Connect] │   │
│  └──────────────────────────────────────────────────┘       │
│  ┌──────────────────────────────────────────────────┐       │
│  │ 🌐 Living Room PC           89,102 photos  [Connect] │   │
│  └──────────────────────────────────────────────────┘       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Caching:**
- Client caches thumbnails locally
- Full images streamed on-demand
- Cache eviction when space needed

**Limitations:**
- No write operations from client (read-only browsing)
- Search might be slow (queries go to server)
- Requires both machines on same network

---

### 9. RAW Format Support

**Purpose:** Handle professional camera RAW files.

**What it does:**
- Detect and index RAW files
- Generate viewable thumbnails from RAW
- Extract EXIF metadata
- Display alongside regular photos

**Supported Formats:**
| Format | Extension | Camera Brands |
|--------|-----------|---------------|
| CR2/CR3 | .cr2, .cr3 | Canon |
| NEF | .nef | Nikon |
| ARW | .arw | Sony |
| ORF | .orf | Olympus |
| RAF | .raf | Fujifilm |
| DNG | .dng | Adobe, various |
| RW2 | .rw2 | Panasonic |

**Implementation Options:**
1. **LibRaw** - C library, Rust bindings available (`libraw-rs`)
2. **rawloader** - Pure Rust, fewer formats
3. **dcraw** - CLI tool, shell out

**Recommendation:** LibRaw for format coverage

**Thumbnail Generation:**
1. Read embedded JPEG preview from RAW (most RAW files have this)
2. If no preview, decode RAW and generate thumbnail
3. Embedded preview is much faster

**EXIF from RAW:**
- RAW files contain full EXIF
- Same extraction as JPEG
- Additional data: lens info, focal length, aperture

**UI Considerations:**
- RAW + JPEG pairs: Many cameras shoot RAW+JPEG
- Detect pairs (same timestamp, same base filename)
- Show as single item? Or separate?
- User preference: "Show RAW+JPEG as single photo"

**Database:**
```sql
ALTER TABLE photos ADD COLUMN raw_pair_id INTEGER;  -- Links RAW to its JPEG
ALTER TABLE photos ADD COLUMN is_raw BOOLEAN DEFAULT FALSE;
```

**Performance:**
- RAW decoding is slow (1-5 seconds per file)
- Always use embedded preview when available
- Full decode only if preview missing or user requests full view

---

### 10. Export/Share

**Purpose:** Create shareable versions of albums or selections.

**What it does:**
- Export album as folder structure
- Generate static HTML gallery (viewable in browser)
- Create ZIP archive for sharing
- Optional: Reduce resolution for sharing

**Export Options:**

**1. Folder Export:**
```
/Exported/Japan 2019/
  ├── IMG_001.jpg
  ├── IMG_002.jpg
  └── ...
```

**2. HTML Gallery:**
```
/Exported/Japan 2019/
  ├── index.html          (Main gallery page)
  ├── viewer.html         (Photo viewer page)
  ├── style.css
  ├── script.js
  ├── thumbnails/
  │   ├── IMG_001_thumb.jpg
  │   └── ...
  └── photos/
      ├── IMG_001.jpg
      └── ...
```

**3. ZIP Archive:**
- Same as above, but compressed
- Single file for easy sharing

**UI - Export Dialog:**
```
┌─────────────────────────────────────────────────────────────┐
│  Export "Japan 2019"                                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Export format:                                              │
│  (●) Folder with photos                                      │
│  ( ) HTML gallery (viewable in browser)                      │
│  ( ) ZIP archive                                             │
│                                                              │
│  Options:                                                    │
│  [ ] Reduce size (max 2048px longest edge)                  │
│  [ ] Include metadata in filenames                          │
│  [ ] Organize by date (YYYY/MM folders)                     │
│                                                              │
│  Destination: [/Users/john/Desktop        ] [Browse]        │
│                                                              │
│  156 photos, estimated size: 1.2 GB                         │
│                                                              │
│                              [Cancel]  [Export]              │
└─────────────────────────────────────────────────────────────┘
```

**HTML Gallery Features:**
- Responsive grid layout
- Click to view full size
- Keyboard navigation
- Works offline (no external dependencies)
- Optional: Include face tags as overlay

**Resize Options:**
| Option | Max Size | Use Case |
|--------|----------|----------|
| Original | Full resolution | Archival |
| Large | 2048px | Quality sharing |
| Medium | 1024px | Email/web |
| Small | 640px | Quick preview |

**Progress UI:**
```
┌─────────────────────────────────────────────────────────────┐
│  Exporting...                                                │
│                                                              │
│  ████████████████████░░░░░░░░░░  67%                        │
│                                                              │
│  Processing: IMG_104.jpg (104 of 156)                       │
│                                                              │
│  [Cancel]                                                    │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Priority

Recommended order based on user value and complexity:

| Priority | Feature | Effort | User Value |
|----------|---------|--------|------------|
| 1 | Albums | Medium | High |
| 2 | Memories | Low | High |
| 3 | Map View | High | High |
| 4 | Scene/Object Tagging | High | High |
| 5 | Export/Share | Medium | Medium |
| 6 | Video Support | High | Medium |
| 7 | Face Cluster Splitting | Low | Medium |
| 8 | RAW Format Support | Medium | Low (niche) |
| 9 | OCR | Medium | Low (niche) |
| 10 | LAN Sync | High | Low (niche) |

---

## Technical Considerations

### Model Sizes (Shipped with App)

| Model | Size | Purpose |
|-------|------|---------|
| SCRFD (face detection) | ~10MB | Phase 1 |
| ArcFace (face embedding) | ~250MB | Phase 1 |
| Geocoding DB | ~50MB | Phase 1 |
| CLIP (scene tagging) | ~350MB | Phase 2 |
| PaddleOCR | ~50MB | Phase 2 |
| Map tiles (basic) | ~200MB | Phase 2 |
| **Total** | **~900MB** | |

### App Bundle Size Strategy

- Core app without models: ~50MB
- Download models on first use
- Or ship "full" version with all models (~1GB)
- User choice during install?

### Background Processing Queue

Phase 2 adds more processing tasks. Need priority system:

```
Priority 1 (User-initiated):
  - Thumbnail generation for visible photos
  - Search queries
  
Priority 2 (Essential):
  - EXIF extraction
  - Face detection
  - Face clustering
  
Priority 3 (Enhancement):
  - Scene tagging (CLIP)
  - OCR
  - Video thumbnail generation
```

---

## Database Migrations

Phase 2 requires schema changes. Migration strategy:

```sql
-- migrations/002_phase2.sql

-- Add media type support
ALTER TABLE photos ADD COLUMN media_type TEXT DEFAULT 'photo';
ALTER TABLE photos ADD COLUMN duration_seconds REAL;
ALTER TABLE photos ADD COLUMN video_codec TEXT;
ALTER TABLE photos ADD COLUMN audio_codec TEXT;

-- Add OCR
ALTER TABLE photos ADD COLUMN ocr_text TEXT;
ALTER TABLE photos ADD COLUMN ocr_processed BOOLEAN DEFAULT FALSE;

-- Add RAW support
ALTER TABLE photos ADD COLUMN raw_pair_id INTEGER;
ALTER TABLE photos ADD COLUMN is_raw BOOLEAN DEFAULT FALSE;

-- Create albums tables
CREATE TABLE albums (...);
CREATE TABLE album_photos (...);

-- Create tags table
CREATE TABLE photo_tags (...);

-- Create FTS index for OCR
CREATE VIRTUAL TABLE photos_fts USING fts5(...);

-- Update schema version
INSERT INTO schema_version (version) VALUES (2);
```

---

## Success Metrics for Phase 2

- Albums: Users create average 5+ albums
- Memories: Users engage with memories feature weekly
- Map: Users browse map view in 30%+ of sessions
- Scene search: 20%+ of searches use scene tags
- Video: Videos played to completion 50%+ of time

---

This specification covers all Phase 2 features in detail. Implementation should follow the priority order, with each feature being complete and stable before moving to the next.
