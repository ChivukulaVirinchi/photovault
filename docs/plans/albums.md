# Albums — Design & Architecture

Status: **draft for review**

---

## Goal

Let users organize photos into named albums, and intelligently suggest
albums for trips and events the system detects. Manual albums ship first
(Phase 1); auto-suggestions layer on top (Phase 2).

This is the Google Photos feature people actually use daily — a local,
offline-first implementation that works with external drives.

---

## What ships

### Phase 1 — Manual Albums (foundation)

CRUD operations, UI for creating/managing albums, and "Add to album" from
every context in the app. Shippable standalone.

### Phase 2 — Suggested Albums (intelligence)

Trip and event detection from existing metadata. Suggestions shown to
user, accepted into real albums or auto-expired. Includes "Save memory
as album" shortcut and Timeline banner integration.

---

## Phase 1: Manual Albums

### Album CRUD

- **Create**: name only (no description — keeps the flow fast)
- **Rename**: inline edit on album detail view
- **Delete**: removes the album grouping; photos are untouched
- **Cover photo**: auto-picked by default (first photo with a face and
  landscape orientation, else first photo by date). User can override
  from album detail view.

### Photo management

- **Add photos** from any context:
  - Timeline multi-select → "Add to album" action
  - Photo detail view → "Add to album" in actions bar
  - Person detail (cluster) → "Add all to album"
  - Memory detail → "Save as album" (creates new album from all photos
    in the memory)
  - Search results → "Add to album" (with multi-select)
  - Map popover → "Add to album"
  - Duplicate/burst detail → "Add to album"
- **Remove photos** from album detail view (multi-select + remove)
- **A photo can belong to multiple albums** (many-to-many)
- **Sort order**: chronological by date_taken (default and only option
  in v1; manual reorder is a future polish item)

### "Add to album" flow

When user triggers "Add to album" with selected photos:

1. Modal/overlay appears showing existing albums as a scrollable list
   of cover thumbnails + names
2. Each album row has a checkbox (pre-checked if ALL selected photos are
   already in that album)
3. "Create new album" option at top → inline name input → creates and
   adds in one step
4. Confirm button applies changes

This is the Google Photos pattern — fast, obvious, works for 1 photo or
100.

### Albums view (sidebar entry)

- Grid of album cover cards (similar to People view layout)
- Each card: cover photo thumbnail, album name, photo count, date range
- Click → Album detail view
- "Create album" button in top bar
- Sort: most recently updated first (albums you're actively adding to
  float to top)

### Album detail view

- Header: album name (editable), photo count, date range
- Photo grid (reuse existing `photo_grid` component)
- Click photo → PhotoDetail with album-scoped prev/next navigation
  (same pattern as `memory_photos` / `cluster_photos`)
- Actions: rename, change cover, delete album
- Multi-select mode for bulk remove

### Navigation integration

- Albums entry in sidebar (below Memories, above People — or wherever
  feels right in the hierarchy)
- Photo detail view shows "In albums: X, Y" when photo belongs to
  albums
- Back navigation from album photo detail returns to album detail view

---

## Phase 2: Suggested Albums

### Detection algorithms

Two suggestion types, both derived from data we already index.

#### Trip detection

A "trip" = photos from a location cluster that is clearly not the user's
home, spanning multiple days.

**Home location detection:**
- Compute most-frequent `location_city` weighted by **distinct calendar
  weeks** (not photo count). This prevents daily office photography from
  skewing home detection.
- A city is "home" if it appears in >= 40% of the user's photographed
  weeks.
- If no city reaches 40%, fall back to the single most-frequent city.
- Cache the home city; recompute only when photos are re-indexed.
- User can override in Settings (explicit home city).

**Trip detection rules (ALL must pass):**
1. `location_city` differs from home
2. City appears in **< 10% of total photographed weeks** in the library
   (filters out office city, parents' city, etc.)
3. GPS centroid of trip photos is **50+ km** from home GPS centroid
   (haversine distance; filters out "different neighborhood")
4. Photos span **2+ distinct calendar days**
5. **8+ photos** across the trip span
6. No existing album already contains > 60% of these photos (don't
   re-suggest what the user already organized)

**Trip title generation:**
- Single city: "Trip to {city}"
- Multiple cities in same country: "Trip to {country}"
- Multiple countries: "Trip to {city1}, {city2}" (max 2 names)

**Trip merging:**
- If two candidate trips to the same city/region have < 3 gap days
  between them, merge into one trip.

#### Event detection

An "event" = a dense cluster of photos in a short time window where
something clearly happened (social gathering, outing, celebration).

**Event detection rules (ALL must pass):**
1. **8+ photos** within a **4-hour sliding window**
2. At least ONE of:
   a. **2+ distinct face clusters** present (social signal), OR
   b. Location appears on **< 3 distinct days** in the entire library
      (you went somewhere unusual)
3. Not already > 60% covered by an existing album
4. Not a subset of a detected trip (trips take priority; events within
   a trip become part of the trip, not separate suggestions)

**Event title generation:**
- With location: "{Day of week} in {city}" (e.g., "Saturday in Rome")
- Without location: "{Month} {day}, {year}" (e.g., "November 15, 2023")
- With multiple face clusters and location: "{city}, {Month} {year}"

### Suggestion lifecycle

Suggestions are ephemeral — they don't pollute the albums table until
accepted.

**States:** `pending` → `accepted` | `dismissed` | `expired`

**Visibility rules (counted by app opens, not calendar days):**
- `seen_count < 3`: shown in Albums view AND as a card in the Timeline
  highlights banner (interleaved with memory cards, ranked by score)
- `seen_count 3-10`: shown in Albums view under "Suggested for you"
  section only
- `seen_count > 10` and still pending: auto-transition to `expired`

Using **app opens** instead of days means we don't penalize users who
open the app infrequently.

**Fingerprinting:**
- Each suggestion stores a `fingerprint` = hash of sorted photo IDs
- Prevents re-generating the same suggestion after dismissal/expiry
- If the photo set changes significantly (>30% new photos in the
  cluster), a new fingerprint is generated and a fresh suggestion can
  appear

**User actions:**
- **Accept**: opens a name confirmation (pre-filled with generated
  title), creates a real album, populates `album_photos`, marks
  suggestion as accepted. One-tap with optional rename.
- **Dismiss**: marks as dismissed, never shown again for this
  fingerprint.
- **Ignore**: seen_count increments on each app open, eventually
  auto-expires.

### Suggestion generation

**When it runs:**
- After photo scanning completes (same trigger pattern as face
  processing / duplicate detection)
- On a weekly regeneration check (compare last generation timestamp)
- NOT on every app open — only when new data or enough time has passed

**Performance budget:**
- Trip detection: 1 SQL query (GROUP BY location_city, date) + Rust
  clustering logic. Target < 100ms / 100k photos.
- Event detection: 1 SQL query (photos ordered by date_taken) + sliding
  window in Rust. Target < 200ms / 100k photos.
- Both run as `Task::perform`, never block the UI.

### Timeline banner integration

The Timeline already has a Memories banner (horizontal carousel). Rather
than adding a second banner:

- **Single "Highlights" banner** above the Timeline grid
- Interleaves memory cards and suggestion cards, ranked by score
- Suggestion cards are visually distinct: subtle "Suggested album" label,
  accept/dismiss buttons directly on the card
- Memory cards remain as-is
- Minimalists see one strip, not two competing banners

### "Save memory as album"

From the Memory detail (slideshow) view, a "Save as album" button:
1. Creates a new album named after the memory title
2. Adds all `memory_photos` to the album
3. Navigates to the new album detail view
4. The memory continues to exist independently (memories are computed,
   albums are persisted — no conflict)

---

## Schema

```sql
-- ============================================================
-- ALBUMS
-- User-created photo collections
-- ============================================================

CREATE TABLE IF NOT EXISTS albums (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    cover_photo_id INTEGER,
    cover_auto_picked BOOLEAN DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS album_photos (
    id INTEGER PRIMARY KEY,
    album_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    added_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    sort_position INTEGER DEFAULT 0,
    FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(album_id, photo_id)
);

CREATE INDEX IF NOT EXISTS idx_album_photos_album ON album_photos(album_id);
CREATE INDEX IF NOT EXISTS idx_album_photos_photo ON album_photos(photo_id);

-- ============================================================
-- ALBUM SUGGESTIONS (Phase 2)
-- System-detected trip/event candidates
-- ============================================================

CREATE TABLE IF NOT EXISTS album_suggestions (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,                -- 'trip' | 'event'
    title TEXT NOT NULL,
    photo_ids_json TEXT NOT NULL,      -- JSON array of photo IDs
    cover_photo_id INTEGER,
    fingerprint TEXT NOT NULL UNIQUE,  -- hash of sorted photo IDs
    seen_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',     -- 'pending'|'accepted'|'dismissed'|'expired'
    accepted_album_id INTEGER,        -- set when accepted
    home_city_at_generation TEXT,      -- snapshot for debugging
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL,
    FOREIGN KEY (accepted_album_id) REFERENCES albums(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_suggestions_status
    ON album_suggestions(status);
CREATE INDEX IF NOT EXISTS idx_suggestions_fingerprint
    ON album_suggestions(fingerprint);
```

Design notes:
- `albums` is clean — no suggestion state leaks into it
- `album_suggestions` stores `photo_ids_json` denormalized (avoids a
  join table for ephemeral data). Only on acceptance do we normalize
  into `album_photos`.
- `fingerprint` prevents re-suggesting the same photo cluster
- `home_city_at_generation` is a debug/audit field — helps diagnose
  "why was this suggested?" without recomputing
- Phase 1 migration adds `albums` + `album_photos` only
- Phase 2 migration adds `album_suggestions`

---

## Architecture

### Module layout

```
src/
  services/
    albums.rs              -- Phase 2: trip_detector, event_detector,
                              home_location, suggestion scoring
  db/
    album_repo.rs          -- albums + album_photos CRUD (Phase 1)
    album_suggestion_repo.rs -- suggestion lifecycle (Phase 2)
  views/
    albums.rs              -- AlbumsView (grid of covers) +
                              AlbumDetailView + suggestion cards
  components/
    album_picker.rs        -- "Add to album" modal widget (Phase 1)
  app/
    handlers/
      albums.rs            -- message handlers
    messages.rs            -- new Message variants
    state/mod.rs           -- new View variants + state fields
```

### State additions

```rust
// View enum
View::Albums,          // album grid
View::AlbumDetail,     // single album's photos

// PhotoVault fields
albums: Vec<AlbumRecord>,
selected_album_id: Option<i64>,
album_photos: Vec<Photo>,
album_picker_open: bool,            // "add to album" modal visible
album_picker_target_photo_ids: Vec<i64>,  // photos being added
album_picker_search: String,        // filter albums in picker
edit_album_name: String,
editing_album_id: Option<i64>,

// Phase 2 additions
album_suggestions: Vec<AlbumSuggestionRecord>,
```

### Messages

```rust
// Phase 1
CreateAlbum(String),                     // name
AlbumCreated(i64),                       // new album ID
RenameAlbum(i64, String),
DeleteAlbum(i64),
SetAlbumCover(i64, i64),                 // album_id, photo_id
OpenAlbum(i64),
AlbumPhotosLoaded(Vec<Photo>),
AddPhotosToAlbum(i64, Vec<i64>),         // album_id, photo_ids
RemovePhotosFromAlbum(i64, Vec<i64>),
OpenAlbumPicker(Vec<i64>),               // photo_ids to add
CloseAlbumPicker,
SaveMemoryAsAlbum,

// Phase 2
SuggestionsGenerated(Vec<AlbumSuggestionRecord>),
AcceptSuggestion(i64),                   // suggestion_id
DismissSuggestion(i64),
SuggestionAccepted(i64, i64),            // suggestion_id, new album_id
```

### Data flow

```
Phase 1 — Manual:

User creates album / adds photos
    │
    ▼
Message handler → album_repo CRUD
    │
    ▼
State updated → UI re-renders

Phase 2 — Suggestions:

Scan completes / weekly check
    │
    ▼
Task::perform(detect_trips + detect_events)
    │
    ▼
Message::SuggestionsGenerated(suggestions)
    │
    ▼
app.album_suggestions updated → Albums view + banner show cards
    │
    ▼
User accepts → creates album + populates album_photos
            → suggestion marked accepted
User dismisses → suggestion marked dismissed
User ignores → seen_count++ each app open → auto-expires
```

---

## Display order

**Albums view:**
- Manual albums: sorted by `updated_at DESC` (most recently modified
  first)
- Suggested albums section (Phase 2): below manual albums, sorted by
  suggestion score

**Album detail (photo grid):**
- Sorted by `date_taken ASC` (chronological, oldest first — tells the
  story of the trip/event)

**Memory detail (existing feature, noted here for consistency):**
- Randomized order for emotional impact (not chronological)

---

## Performance

| Operation | Work | Budget |
|-----------|------|--------|
| Create/rename/delete album | 1 SQL statement | < 5ms |
| Add N photos to album | N INSERTs (batched) | < 50ms / 1000 photos |
| Load album list | 1 query + cover thumbnails | < 20ms |
| Load album photos | 1 JOIN query | < 30ms / 10k album |
| Trip detection (Phase 2) | 1 GROUP BY query + Rust clustering | < 100ms / 100k photos |
| Event detection (Phase 2) | 1 ordered query + sliding window | < 200ms / 100k photos |

All Phase 2 detection runs as `Task::perform`. No UI blocking.

---

## Implementation order

### Phase 1 (Manual Albums)

1. Schema migration: `albums` + `album_photos` tables
2. `album_repo.rs`: CRUD (create, rename, delete, add_photos,
   remove_photos, list_albums, get_album_photos, set_cover)
3. State + messages: View::Albums, View::AlbumDetail, album state
   fields, message variants
4. Albums view: grid of album cover cards + "Create album" button
5. Album detail view: photo grid with album-scoped navigation
6. Album picker component: "Add to album" modal
7. "Add to album" from timeline multi-select (primary entry point)
8. "Add to album" from photo detail view
9. "Add to album" from person detail, memory detail ("Save as album"),
   search results
10. Photo detail integration: show "In albums: X, Y" badge
11. Sidebar entry: Albums with count badge

### Phase 2 (Suggested Albums)

1. Schema migration: `album_suggestions` table
2. `album_suggestion_repo.rs`: CRUD + lifecycle
3. `albums.rs` service: home_location detector
4. `albums.rs` service: trip_detector
5. `albums.rs` service: event_detector
6. Suggestion generation trigger (post-scan + weekly)
7. Suggestion cards in Albums view ("Suggested for you" section)
8. Accept/dismiss UX + album creation flow
9. Timeline highlights banner: interleave suggestions with memory cards
10. Seen-count tracking + auto-expiry
11. Settings: explicit home city override

---

## What this plan explicitly does NOT cover

- Smart albums / saved searches (different concept, future feature)
- Nested albums or album folders
- Manual photo reordering within albums (date sort only in v1)
- Album sharing (out of scope per project constraints)
- Video support in albums (blocked on general video support)
- "People albums" (People view already serves this purpose)
- Exact SQL syntax (pseudocode only, real queries at implementation)
- Exact iced widget layout (text description only, follows existing
  patterns)
- Visual design / colors / typography (follows existing theme)
