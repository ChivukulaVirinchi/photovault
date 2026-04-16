# Unified Search — Implementation Plan

Status: **ready to implement**

Transform the current photo-only search into a multi-entity finder.
A single search box returns People, Albums, Places, and Photos in
grouped, clickable results. Live as-you-type, debounced 200ms. Recent
searches persist per-drive.

---

## Implementation Steps (in order)

7 steps total. Each step is a compilable checkpoint.

---

### Step 1: Database migration (v13 → v14) — recent searches

**Files to modify:**
- `src/db/schema.rs`
- `src/db/migrations.rs`

#### 1a. Add table to schema

In `src/db/schema.rs`, before the INDEXES section, add:

```sql
-- ============================================================
-- RECENT SEARCHES
-- Per-library search history (last N queries)
-- ============================================================

CREATE TABLE IF NOT EXISTS recent_searches (
    id INTEGER PRIMARY KEY,
    query TEXT NOT NULL,
    last_used DATETIME DEFAULT CURRENT_TIMESTAMP,
    use_count INTEGER DEFAULT 1,
    UNIQUE(query)
);
```

Add index:
```sql
CREATE INDEX IF NOT EXISTS idx_recent_searches_used ON recent_searches(last_used DESC);
```

Bump schema version to 14.

#### 1b. Migration

In `src/db/migrations.rs`:

```rust
if current_version < 14 {
    migrate_v13_to_v14(conn)?;
}
```

```rust
fn migrate_v13_to_v14(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS recent_searches (
            id INTEGER PRIMARY KEY,
            query TEXT NOT NULL,
            last_used DATETIME DEFAULT CURRENT_TIMESTAMP,
            use_count INTEGER DEFAULT 1,
            UNIQUE(query)
        );

        CREATE INDEX IF NOT EXISTS idx_recent_searches_used
            ON recent_searches(last_used DESC);

        INSERT INTO schema_version (version) VALUES (14);
        "#,
    )?;
    tracing::info!("Migrated database to schema version 14 (recent searches)");
    Ok(())
}
```

**Checkpoint**: Builds, existing DBs auto-migrate.

---

### Step 2: Recent searches repo

**Files to create:**
- `src/db/recent_search_repo.rs`

**Files to modify:**
- `src/db/mod.rs`

#### 2a. `src/db/recent_search_repo.rs`

```rust
//! Recent searches persistence (per-library history).

use rusqlite::{params, Connection, Result as SqliteResult};

#[derive(Debug, Clone)]
pub struct RecentSearch {
    pub query: String,
    pub last_used: String,
    pub use_count: i64,
}

pub struct RecentSearchRepo<'a> {
    conn: &'a Connection,
}

impl<'a> RecentSearchRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }

    /// Record a search. Bumps use_count + last_used if it exists.
    pub fn record(&self, query: &str) -> SqliteResult<()> {
        let trimmed = query.trim();
        if trimmed.is_empty() { return Ok(()); }
        self.conn.execute(
            r#"
            INSERT INTO recent_searches (query, last_used, use_count)
            VALUES (?1, CURRENT_TIMESTAMP, 1)
            ON CONFLICT(query) DO UPDATE SET
                last_used = CURRENT_TIMESTAMP,
                use_count = use_count + 1
            "#,
            params![trimmed],
        )?;
        Ok(())
    }

    /// Get the N most-recently-used searches.
    pub fn get_recent(&self, limit: i64) -> SqliteResult<Vec<RecentSearch>> {
        let mut stmt = self.conn.prepare(
            "SELECT query, last_used, use_count FROM recent_searches
             ORDER BY last_used DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(RecentSearch {
                query: row.get(0)?,
                last_used: row.get(1)?,
                use_count: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Remove a single recent search.
    pub fn remove(&self, query: &str) -> SqliteResult<()> {
        self.conn.execute(
            "DELETE FROM recent_searches WHERE query = ?1",
            params![query],
        )?;
        Ok(())
    }

    /// Clear all recent searches.
    pub fn clear(&self) -> SqliteResult<()> {
        self.conn.execute("DELETE FROM recent_searches", [])?;
        Ok(())
    }
}
```

#### 2b. Register in `src/db/mod.rs`

```rust
pub mod recent_search_repo;
pub use recent_search_repo::{RecentSearch, RecentSearchRepo};
```

**Checkpoint**: Builds.

---

### Step 3: Unified search service

The current `SearchService::search` only returns photos. We add new
entity result types and a multi-entity query function.

**Files to modify:**
- `src/services/search.rs`

#### 3a. Add new result structs

After the existing `SearchResult` and `SearchResultGroup`:

```rust
/// A person matching the search query.
#[derive(Debug, Clone)]
pub struct PersonHit {
    pub cluster_id: i64,
    pub name: String,
    pub photo_count: i64,
    pub face_thumbnail_path: Option<String>,  // resolved at load time
}

/// An album matching the search query.
#[derive(Debug, Clone)]
pub struct AlbumHit {
    pub album_id: i64,
    pub name: String,
    pub photo_count: i64,
    pub cover_thumbnail_path: Option<String>,  // resolved at load time
}

/// A place (city) matching the search query.
#[derive(Debug, Clone)]
pub struct PlaceHit {
    pub city: String,
    pub country: Option<String>,
    pub photo_count: i64,
}

/// Unified search results across all entity types.
#[derive(Debug, Clone, Default)]
pub struct UnifiedSearchResults {
    pub people: Vec<PersonHit>,
    pub albums: Vec<AlbumHit>,
    pub places: Vec<PlaceHit>,
    pub photos: Vec<SearchResult>,
    pub photos_grouped: Vec<SearchResultGroup>,
    pub photo_ids: Vec<i64>,  // flat list for cull mode
}
```

#### 3b. Add unified search method

Add to `impl SearchService`:

```rust
/// Run a unified multi-entity search.
///
/// The query string is parsed once via QueryParser; people/album/place
/// matches use simple substring LIKE. Photos use the existing
/// QueryParser-based filter.
pub fn search_unified(
    conn: &Connection,
    raw_query: &str,
) -> SqliteResult<UnifiedSearchResults> {
    let query = raw_query.trim();
    if query.is_empty() {
        return Ok(UnifiedSearchResults::default());
    }

    let mut results = UnifiedSearchResults::default();

    // 1. People — fuzzy match on cluster name
    results.people = Self::search_people(conn, query)?;

    // 2. Albums — fuzzy match on album name
    results.albums = Self::search_albums(conn, query)?;

    // 3. Places — fuzzy match on city or country
    results.places = Self::search_places(conn, query)?;

    // 4. Photos — use existing parsed query filter
    let parsed = crate::search::QueryParser::parse(query);
    let photos = Self::search(conn, &parsed)?;
    results.photo_ids = photos.iter().map(|r| r.photo_id).collect();
    results.photos_grouped = Self::group_by_date(photos.clone());
    results.photos = photos;

    Ok(results)
}

fn search_people(conn: &Connection, q: &str) -> SqliteResult<Vec<PersonHit>> {
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        r#"
        SELECT fc.id, fc.name, fc.photo_count, fc.representative_face_id
        FROM face_clusters fc
        WHERE fc.name IS NOT NULL
          AND LOWER(fc.name) LIKE LOWER(?1)
        ORDER BY fc.photo_count DESC
        LIMIT 10
        "#,
    )?;
    let rows = stmt.query_map(params![like], |row| {
        Ok(PersonHit {
            cluster_id: row.get(0)?,
            name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            photo_count: row.get(2)?,
            face_thumbnail_path: None,  // resolved at load time
        })
    })?;
    rows.collect()
}

fn search_albums(conn: &Connection, q: &str) -> SqliteResult<Vec<AlbumHit>> {
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, photo_count, cover_photo_id
        FROM albums
        WHERE LOWER(name) LIKE LOWER(?1)
        ORDER BY updated_at DESC
        LIMIT 10
        "#,
    )?;
    let rows = stmt.query_map(params![like], |row| {
        Ok(AlbumHit {
            album_id: row.get(0)?,
            name: row.get(1)?,
            photo_count: row.get(2)?,
            cover_thumbnail_path: None,  // resolved at load time
        })
    })?;
    rows.collect()
}

fn search_places(conn: &Connection, q: &str) -> SqliteResult<Vec<PlaceHit>> {
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        r#"
        SELECT location_city, location_country, COUNT(*) AS cnt
        FROM photos
        WHERE is_trashed = FALSE
          AND location_city IS NOT NULL
          AND (LOWER(location_city) LIKE LOWER(?1)
               OR LOWER(location_country) LIKE LOWER(?1))
        GROUP BY location_city, location_country
        ORDER BY cnt DESC
        LIMIT 10
        "#,
    )?;
    let rows = stmt.query_map(params![like], |row| {
        Ok(PlaceHit {
            city: row.get(0)?,
            country: row.get(1)?,
            photo_count: row.get(2)?,
        })
    })?;
    rows.collect()
}
```

**Checkpoint**: Builds. New entity searches available.

---

### Step 4: State + Messages

**Files to modify:**
- `src/app/state/mod.rs`
- `src/app/messages.rs`

#### 4a. State changes

Replace the existing search state fields with:

```rust
// --- Unified Search ---
/// Current text in the search input.
pub(crate) search_query: String,

/// Live unified results (None = haven't searched yet, empty = no matches).
pub(crate) search_results: Option<crate::services::UnifiedSearchResults>,

/// True while a search is in flight.
pub(crate) search_loading: bool,

/// Generation counter for debounce. Each input change bumps this.
/// A pending debounced search compares its generation to this value;
/// if it doesn't match, the input changed and we skip the search.
pub(crate) search_generation: u64,

/// Recent searches loaded from DB (shown when input is empty).
pub(crate) recent_searches: Vec<crate::db::RecentSearch>,

/// Whether the search input currently has focus (controls recent dropdown).
pub(crate) search_input_focused: bool,

/// Currently highlighted result index for keyboard navigation.
/// Counts across all sections (people first, then albums, places, photos).
pub(crate) search_highlighted_index: Option<usize>,
```

Note: `search_query`, `search_results`, `search_loading` already exist
but `search_results` type changes from `Option<Vec<SearchResultGroup>>`
to `Option<UnifiedSearchResults>`. Also remove
`search_suggestions: Vec<String>` and `search_result_photo_ids: Vec<i64>`
(now embedded in UnifiedSearchResults).

#### 4b. Initializers in `new()`

Add/update:
```rust
search_query: String::new(),
search_results: None,
search_loading: false,
search_generation: 0,
recent_searches: Vec::new(),
search_input_focused: false,
search_highlighted_index: None,
```

Remove old initializers for `search_suggestions` and
`search_result_photo_ids`.

#### 4c. Update message variants

In `src/app/messages.rs`:

**Remove** these obsolete variants:
- `SearchSuggestionSelected(String)`
- `SearchSuggestionsLoaded(Vec<String>)`

**Modify**:
- Change `SearchComplete(Vec<SearchResultGroup>, Vec<i64>)` →
  `SearchComplete(u64, Box<crate::services::UnifiedSearchResults>)` (the u64 is generation)

**Add new variants** (in the search section):
```rust
/// Debounced search trigger — fires 200ms after the last input change.
/// Carries generation; ignored if generation doesn't match current.
SearchDebouncedTick(u64),
/// Recent searches loaded from DB.
RecentSearchesLoaded(Vec<crate::db::RecentSearch>),
/// User clicked a recent search chip.
SearchRecentSelected(String),
/// User clicked the X next to a recent search.
SearchRecentRemove(String),
/// User clicked "Clear all" on the recent searches list.
SearchClearRecent,
/// Search input gained focus.
SearchInputFocused,
/// Search input lost focus.
SearchInputBlurred,
/// User clicked a person hit in results.
SearchOpenPerson(i64),
/// User clicked an album hit in results.
SearchOpenAlbum(i64),
/// User clicked a place hit — re-run search filtered to that city.
SearchOpenPlace(String),
/// Keyboard arrow up/down for navigating results.
SearchHighlightPrev,
SearchHighlightNext,
/// Activate the currently highlighted result (Enter key).
SearchActivateHighlighted,
```

Update the existing handler match arms accordingly (remove old ones,
wire new ones).

#### 4d. Update `src/app/handlers/mod.rs`

Remove the obsolete match arms and add the new ones. Also remove the
`SearchSuggestionSelected` and `SearchSuggestionsLoaded` handlers.

**Checkpoint**: Builds (after handlers updated in next step).

---

### Step 5: Handlers

**File to modify:**
- `src/app/handlers/search_cull.rs`

Replace the existing search handlers with:

#### 5a. `search_input_changed`

```rust
pub(crate) fn search_input_changed(app: &mut PhotoVault, input: String) -> Task<Message> {
    app.search_query = input.clone();
    app.search_generation = app.search_generation.wrapping_add(1);
    let gen = app.search_generation;

    if input.trim().is_empty() {
        // Clear results, show recent searches instead
        app.search_results = None;
        app.search_loading = false;
        return Task::none();
    }

    // Schedule debounced search
    Task::perform(
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            gen
        },
        Message::SearchDebouncedTick,
    )
}
```

#### 5b. `search_debounced_tick`

```rust
pub(crate) fn search_debounced_tick(app: &mut PhotoVault, gen: u64) -> Task<Message> {
    // If input changed since this debounce was scheduled, skip
    if gen != app.search_generation {
        return Task::none();
    }
    if app.search_query.trim().is_empty() {
        return Task::none();
    }
    execute_search(app)
}
```

#### 5c. `execute_search` — refactored to unified

```rust
pub(crate) fn execute_search(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else { return Task::none(); };
    let query_text = app.search_query.clone();
    let drive_path = drive_path.clone();
    let gen = app.search_generation;

    app.search_loading = true;

    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                let db = Database::open_for_drive(&drive_path)
                    .map_err(|e| format!("DB open: {}", e))?;
                let mut results = SearchService::search_unified(&db.conn, &query_text)
                    .map_err(|e| format!("Search: {}", e))?;

                // Resolve face thumbnails for people hits
                for person in &mut results.people {
                    let face_id: Option<i64> = db.conn.query_row(
                        "SELECT id FROM faces WHERE cluster_id = ?1 ORDER BY confidence DESC LIMIT 1",
                        rusqlite::params![person.cluster_id],
                        |row| row.get(0),
                    ).ok();
                    if let Some(fid) = face_id {
                        let crop = drive_path
                            .join(".photovault")
                            .join("face_crops")
                            .join(format!("{}.jpg", fid));
                        if crop.exists() {
                            person.face_thumbnail_path = Some(crop.to_string_lossy().to_string());
                        }
                    }
                }

                // Resolve album cover thumbnails
                let photo_repo = crate::db::PhotoRepo::new(&db.conn);
                for album in &mut results.albums {
                    if let Ok(Some(cover_id_row)) = db.conn.query_row(
                        "SELECT cover_photo_id FROM albums WHERE id = ?1",
                        rusqlite::params![album.album_id],
                        |row| row.get::<_, Option<i64>>(0),
                    ).map(Some) {
                        if let Some(cid) = cover_id_row {
                            if let Ok(Some(p)) = photo_repo.get_by_id(cid) {
                                album.cover_thumbnail_path = p.thumbnail_path
                                    .map(|tp| drive_path.join(tp).to_string_lossy().to_string());
                            }
                        }
                    }
                }

                Ok::<_, String>(results)
            }).await;

            match result {
                Ok(Ok(r)) => (gen, Box::new(r)),
                _ => (gen, Box::new(crate::services::UnifiedSearchResults::default())),
            }
        },
        |(gen, results)| Message::SearchComplete(gen, results),
    )
}
```

#### 5d. `search_complete`

```rust
pub(crate) fn search_complete(
    app: &mut PhotoVault,
    gen: u64,
    results: Box<crate::services::UnifiedSearchResults>,
) -> Task<Message> {
    // Discard stale results from older generations
    if gen != app.search_generation {
        return Task::none();
    }
    app.search_loading = false;
    app.search_results = Some(*results);
    app.search_highlighted_index = None;

    // Record this search as recent (best-effort, non-blocking)
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let query = app.search_query.clone();
        return Task::perform(
            async move {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    let _ = crate::db::RecentSearchRepo::new(&db.conn).record(&query);
                    crate::db::RecentSearchRepo::new(&db.conn)
                        .get_recent(10).unwrap_or_default()
                } else {
                    Vec::new()
                }
            },
            Message::RecentSearchesLoaded,
        );
    }
    Task::none()
}
```

#### 5e. Recent search handlers

```rust
pub(crate) fn recent_searches_loaded(
    app: &mut PhotoVault,
    list: Vec<crate::db::RecentSearch>,
) -> Task<Message> {
    app.recent_searches = list;
    Task::none()
}

pub(crate) fn search_recent_selected(app: &mut PhotoVault, query: String) -> Task<Message> {
    app.search_query = query;
    app.search_generation = app.search_generation.wrapping_add(1);
    execute_search(app)
}

pub(crate) fn search_recent_remove(app: &mut PhotoVault, query: String) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else { return Task::none(); };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        let _ = crate::db::RecentSearchRepo::new(&db.conn).remove(&query);
    }
    app.recent_searches.retain(|r| r.query != query);
    Task::none()
}

pub(crate) fn search_clear_recent(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else { return Task::none(); };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        let _ = crate::db::RecentSearchRepo::new(&db.conn).clear();
    }
    app.recent_searches.clear();
    Task::none()
}

pub(crate) fn search_input_focused(app: &mut PhotoVault) -> Task<Message> {
    app.search_input_focused = true;
    if app.recent_searches.is_empty() {
        return load_recent_searches(app);
    }
    Task::none()
}

pub(crate) fn search_input_blurred(app: &mut PhotoVault) -> Task<Message> {
    app.search_input_focused = false;
    Task::none()
}

fn load_recent_searches(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else { return Task::none(); };
    let drive_path = drive_path.clone();
    Task::perform(
        async move {
            if let Ok(db) = Database::open_for_drive(&drive_path) {
                crate::db::RecentSearchRepo::new(&db.conn)
                    .get_recent(10).unwrap_or_default()
            } else {
                Vec::new()
            }
        },
        Message::RecentSearchesLoaded,
    )
}
```

#### 5f. Entity-click handlers

```rust
pub(crate) fn search_open_person(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    super::handle(app, Message::SelectCluster(cluster_id))
}

pub(crate) fn search_open_album(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    super::handle(app, Message::OpenAlbum(album_id))
}

pub(crate) fn search_open_place(app: &mut PhotoVault, city: String) -> Task<Message> {
    // Re-run search scoped to that city
    app.search_query = city;
    app.search_generation = app.search_generation.wrapping_add(1);
    execute_search(app)
}
```

#### 5g. Keyboard navigation handlers

```rust
pub(crate) fn search_highlight_next(app: &mut PhotoVault) -> Task<Message> {
    let total = total_results(app);
    if total == 0 { return Task::none(); }
    let next = match app.search_highlighted_index {
        None => 0,
        Some(i) => (i + 1) % total,
    };
    app.search_highlighted_index = Some(next);
    Task::none()
}

pub(crate) fn search_highlight_prev(app: &mut PhotoVault) -> Task<Message> {
    let total = total_results(app);
    if total == 0 { return Task::none(); }
    let prev = match app.search_highlighted_index {
        None => total - 1,
        Some(0) => total - 1,
        Some(i) => i - 1,
    };
    app.search_highlighted_index = Some(prev);
    Task::none()
}

pub(crate) fn search_activate_highlighted(app: &mut PhotoVault) -> Task<Message> {
    let Some(idx) = app.search_highlighted_index else {
        return execute_search(app);  // No highlight → submit search
    };
    let Some(ref results) = app.search_results else { return Task::none(); };

    // Resolve which entity the index points to
    let mut cursor = idx;
    if cursor < results.people.len() {
        let cid = results.people[cursor].cluster_id;
        return super::handle(app, Message::SearchOpenPerson(cid));
    }
    cursor -= results.people.len();
    if cursor < results.albums.len() {
        let aid = results.albums[cursor].album_id;
        return super::handle(app, Message::SearchOpenAlbum(aid));
    }
    cursor -= results.albums.len();
    if cursor < results.places.len() {
        let city = results.places[cursor].city.clone();
        return super::handle(app, Message::SearchOpenPlace(city));
    }
    cursor -= results.places.len();
    if cursor < results.photos.len() {
        let pid = results.photos[cursor].photo_id;
        return super::handle(app, Message::SelectPhoto(pid));
    }
    Task::none()
}

fn total_results(app: &PhotoVault) -> usize {
    app.search_results.as_ref().map(|r| {
        r.people.len() + r.albums.len() + r.places.len() + r.photos.len().min(20)
    }).unwrap_or(0)
}
```

#### 5h. Wire all handlers in `src/app/handlers/mod.rs`

Replace existing search match arms with the new ones. Keep
`Message::ExecuteSearch => search_cull::execute_search(app)`.

**Checkpoint**: Builds (will need view rewrite next).

---

### Step 6: New search view

The view rewrite is the biggest piece.

**File to rewrite:**
- `src/views/search.rs`

#### 6a. New view function signature

```rust
pub fn view(
    query: &str,
    results: Option<&UnifiedSearchResults>,
    recent: &[crate::db::RecentSearch],
    is_loading: bool,
    is_focused: bool,
    highlighted_index: Option<usize>,
    drive_path: Option<&Path>,
    photos: &[Photo],
    theme: AppTheme,
) -> Element<'static, Message>
```

#### 6b. Layout structure

```
┌─────────────────────────────────────────┐
│  [Search input.....................] X  │  <- search bar with clear btn
├─────────────────────────────────────────┤
│                                         │
│  When input empty + focused:            │
│    "Recent searches"                    │
│    • ooty                       x       │
│    • dad in tokyo               x       │
│    • march 2019                 x       │
│    [Clear all]                          │
│                                         │
│  When input has results:                │
│    PEOPLE (3)                           │
│      [face] Dad        12 photos  >     │
│      [face] Mom         8 photos  >     │
│                                         │
│    ALBUMS (1)                           │
│      [cover] Trip to Ooty  47 photos >  │
│                                         │
│    PLACES (1)                           │
│      Ooty, India       47 photos    >   │
│                                         │
│    PHOTOS (47)                          │
│      [grid of 4 thumbs]  [Show all]     │
│                                         │
│  When input has no matches:             │
│    "No matches for 'xyz'"               │
│    "Try a different query"              │
│                                         │
│  When input is empty + not focused:     │
│    Search examples (existing behavior)  │
│                                         │
└─────────────────────────────────────────┘
```

#### 6c. Component functions

Build small composable functions:

```rust
fn search_bar(query: &str, has_text: bool, p: &Palette) -> Element<'static, Message>
fn recent_searches_panel(recent: &[RecentSearch], p: &Palette) -> Element<'static, Message>
fn results_panel(
    results: &UnifiedSearchResults,
    highlighted: Option<usize>,
    drive_path: Option<&Path>,
    photos: &[Photo],
    theme: AppTheme,
) -> Element<'static, Message>
fn no_results(query: &str, p: &Palette) -> Element<'static, Message>
fn examples_panel(p: &Palette) -> Element<'static, Message>

fn person_row(hit: &PersonHit, is_highlighted: bool, p: &Palette) -> Element<'static, Message>
fn album_row(hit: &AlbumHit, is_highlighted: bool, p: &Palette) -> Element<'static, Message>
fn place_row(hit: &PlaceHit, is_highlighted: bool, p: &Palette) -> Element<'static, Message>
fn photos_section(
    photos: &[SearchResult],
    highlighted_offset: Option<usize>,
    drive_path: Option<&Path>,
    photos_full: &[Photo],
    theme: AppTheme,
) -> Element<'static, Message>

fn section_header(label: &str, count: usize, p: &Palette) -> Element<'static, Message>
```

#### 6d. Person row (the killer detail — face thumbnails)

```rust
fn person_row(
    hit: &PersonHit,
    is_highlighted: bool,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let face: Element<'static, Message> = if let Some(ref path) = hit.face_thumbnail_path {
        container(
            iced::widget::image(iced::widget::image::Handle::from_path(path))
                .width(40).height(40).content_fit(ContentFit::Cover),
        )
        .width(40).height(40)
        .style(move |_t| container::Style {
            border: iced::Border { radius: 20.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
    } else {
        container(text(&hit.name[..1.min(hit.name.len())]).size(16).color(p.text_secondary))
            .width(40).height(40)
            .center_x(Length::Fixed(40.0)).center_y(Length::Fixed(40.0))
            .style(move |_t| container::Style {
                background: Some(p.bg_elevated.into()),
                border: iced::Border { radius: 20.0.into(), ..Default::default() },
                ..Default::default()
            })
            .into()
    };

    let bg = if is_highlighted { p.bg_hover } else { p.bg_elevated };
    let cluster_id = hit.cluster_id;

    button(
        row![
            face,
            Space::with_width(12),
            column![
                text(hit.name.clone()).size(14).color(p.text_primary),
                text(format!("{} photos", hit.photo_count)).size(11).color(p.text_tertiary),
            ].spacing(2),
        ].align_y(Alignment::Center),
    )
    .padding([8, 12])
    .width(Length::Fill)
    .style(move |_t, status| button::Style {
        background: Some(match status {
            button::Status::Hovered => p.bg_hover.into(),
            _ => bg.into(),
        }),
        border: iced::Border { radius: 8.0.into(), ..Default::default() },
        ..Default::default()
    })
    .on_press(Message::SearchOpenPerson(cluster_id))
    .into()
}
```

Album and place rows follow identical patterns — square cover for
album, location pin emoji for place.

#### 6e. Photos section

Reuse the existing thumbnail rendering from the old search view.
Wrap in a `Show all (N)` button if more than 16 photos.

#### 6f. Recent searches panel

```rust
fn recent_searches_panel(
    recent: &[RecentSearch],
    p: &Palette,
) -> Element<'static, Message> {
    let mut col = column![
        row![
            text("Recent searches").size(12).color(p.text_secondary),
            Space::with_width(Length::Fill),
            button(text("Clear all").size(11).color(p.text_tertiary))
                .padding([4, 8])
                .style(|_t, _s| button::Style::default())
                .on_press(Message::SearchClearRecent),
        ].align_y(Alignment::Center),
        Space::with_height(8),
    ].spacing(4);

    for r in recent {
        let q = r.query.clone();
        let q_for_remove = r.query.clone();
        col = col.push(
            row![
                button(text(r.query.clone()).size(13).color(p.text_primary))
                    .padding([6, 10])
                    .width(Length::Fill)
                    .style(move |_t, status| button::Style {
                        background: match status {
                            button::Status::Hovered => Some(p.bg_hover.into()),
                            _ => Some(p.bg_elevated.into()),
                        },
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Message::SearchRecentSelected(q)),
                button(text("x").size(11).color(p.text_tertiary))
                    .padding([6, 8])
                    .style(|_t, _s| button::Style::default())
                    .on_press(Message::SearchRecentRemove(q_for_remove)),
            ].spacing(4).align_y(Alignment::Center),
        );
    }
    col.into()
}
```

#### 6g. Critical iced rules — apply to whole view

- The outermost container can use `height(Length::Fill)`
- The single top-level `scrollable()` wraps everything below the search bar
- NEVER put `height(Length::Fill)` INSIDE the scrollable
- Empty states use padding for vertical centering, never Fill height

#### 6h. Top-level assembly

```rust
let body: Element<'static, Message> = if is_loading {
    // loading state
} else if let Some(results) = results {
    if is_empty(results) {
        no_results(query, &p)
    } else {
        results_panel(results, highlighted_index, drive_path, photos, theme)
    }
} else if query.is_empty() && is_focused && !recent.is_empty() {
    recent_searches_panel(recent, &p)
} else {
    examples_panel(&p)
};

let content = column![
    text("Search").size(22).color(p.text_primary),
    Space::with_height(20),
    search_bar(query, !query.is_empty(), &p),
    Space::with_height(14),
    body,
    Space::with_height(32),
];

container(scrollable(content).id(scrollable::Id::new("search")))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_t| container::Style {
        background: Some(p.bg_primary.into()),
        ..Default::default()
    })
    .into()
```

#### 6i. Update view caller in `src/app/views.rs`

The View::Search arm becomes:

```rust
View::Search => SearchView::view(
    &app.search_query,
    app.search_results.as_ref(),
    &app.recent_searches,
    app.search_loading,
    app.search_input_focused,
    app.search_highlighted_index,
    app.selected_drive.as_deref(),
    &app.photos,
    app.config.theme,
),
```

**Checkpoint**: Full unified search functional.

---

### Step 7: Polish — keyboard navigation + focus tracking

#### 7a. Wire keyboard navigation

In `src/app/handlers/timeline.rs` (where `key_pressed` lives) or wherever
keyboard events are routed, add Search-view-specific handling:

```rust
// When current_view is View::Search:
//   Down arrow → SearchHighlightNext
//   Up arrow → SearchHighlightPrev
//   Enter → SearchActivateHighlighted (already handled by text_input on_submit)
//   Escape → clear input
```

Note: text_input's `on_submit` already fires Enter as `ExecuteSearch`.
We can either hook keyboard events at the view level or just rely on
text_input's submit + arrow keys handled by a global key listener.

For simplicity in v1, **skip arrow-key navigation** — just rely on
text_input submit + click. Add it later if needed. (The plan documents
the messages but the v1 implementation can defer the keyboard listener
wiring.)

#### 7b. Update navigate_to for Search view

In `src/app/handlers/scanning.rs`, when navigating to View::Search,
load recent searches:

```rust
} else if view == View::Search {
    app.current_view = view;
    if app.recent_searches.is_empty() {
        let drive_path = app.selected_drive.clone();
        if let Some(drive_path) = drive_path {
            return Task::perform(
                async move {
                    if let Ok(db) = Database::open_for_drive(&drive_path) {
                        crate::db::RecentSearchRepo::new(&db.conn)
                            .get_recent(10).unwrap_or_default()
                    } else { Vec::new() }
                },
                Message::RecentSearchesLoaded,
            );
        }
    }
    return Task::none();
}
```

**Checkpoint**: Recent searches load when user opens Search view.

---

## File change summary

### New files (2)
| File | Purpose |
|------|---------|
| `src/db/recent_search_repo.rs` | Recent searches CRUD |
| (none for views — modifying existing) | |

### Modified files (~7)
| File | Changes |
|------|---------|
| `src/db/schema.rs` | Add recent_searches table, bump to v14 |
| `src/db/migrations.rs` | Add v13→v14 migration |
| `src/db/mod.rs` | Register recent_search_repo |
| `src/services/search.rs` | Add UnifiedSearchResults + new entity result types + search_unified() |
| `src/app/state/mod.rs` | Replace search state with unified version |
| `src/app/messages.rs` | Replace search messages with unified set |
| `src/app/handlers/search_cull.rs` | Replace handlers with unified set + debounce + recent searches |
| `src/app/handlers/mod.rs` | Wire new search match arms |
| `src/app/handlers/scanning.rs` | Load recent searches on Search view navigate |
| `src/views/search.rs` | Full rewrite for unified results + recent + entity rows |
| `src/app/views.rs` | Update SearchView::view call signature |

---

## Performance budget

| Operation | Work | Target |
|-----------|------|--------|
| Single keystroke | State update + debounce schedule | < 1ms |
| Debounced search trigger | 4 SQL queries (people/albums/places/photos) | < 50ms / 100k photos |
| Cover thumbnail resolution | 1 query per album, 1 per person | < 20ms total |
| Recent search record | 1 INSERT/UPDATE | < 5ms |
| Recent search load | 1 SELECT | < 5ms |

Debounce eliminates wasteful intermediate searches — only the final
input after 200ms triggers DB work.

Generation counter ensures stale results are discarded if the user
typed faster than the search returned.

---

## Click navigation table

| Result type | Click action |
|-------------|--------------|
| Person hit | Navigate to ClusterDetail (existing person view) |
| Album hit | Navigate to AlbumDetail |
| Place hit | Re-run search with city as new query |
| Photo result | Open PhotoDetail with search results as nav list |
| Recent search chip | Re-run that exact query |
| Recent search × | Remove from history |

---

## What this plan explicitly does NOT cover

- Boolean operators (AND / OR / NOT) — out of scope for v1
- Search by image / "more like this" — needs perceptual hashing infra
- Calendar date picker widget — text parser handles "March 2019" etc
- Saved/pinned searches — low value, skipped
- Voice input — not relevant for desktop
- OCR text as a separate result group — already searched via text path
- Arrow-key keyboard navigation — message variants defined but full
  keyboard listener wiring deferred (text_input handles Enter)
- Search filter chips/toggles — keep the search box as the only UI
- "Search within results" — refine by editing the query

---

## Implementation order

Each step is a compilable checkpoint:

1. Schema migration v13→v14 (recent_searches table)
2. recent_search_repo with CRUD
3. UnifiedSearchResults + search_unified service method
4. State + Messages refactor (replace old search state)
5. Handlers refactor (debounce + unified + recent + entity clicks)
6. New search view (entity rows + recent panel + grouped results)
7. Recent searches loading on view navigate

Total estimated change: ~700 LOC added, ~150 LOC removed.
