# Albums Phase 1 — Manual Albums Implementation Plan

Status: **ready to implement**

Phase 1 covers manual album CRUD, the Albums view, album detail view,
"Add to album" from all contexts, and sidebar integration. No
auto-suggestion intelligence — that's Phase 2.

---

## Implementation Steps (in order)

11 steps total. Each step is a compilable checkpoint.

---

### Step 1: Database migration (v11 → v12)

**Files to modify:**
- `src/db/schema.rs`
- `src/db/migrations.rs`

#### 1a. `src/db/schema.rs` — add tables to SCHEMA_SQL

Insert after the `memory_blocks` table block (before INDEXES section):

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
    photo_count INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS album_photos (
    id INTEGER PRIMARY KEY,
    album_id INTEGER NOT NULL,
    photo_id INTEGER NOT NULL,
    added_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
    FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
    UNIQUE(album_id, photo_id)
);
```

Add indexes in the INDEXES section:

```sql
CREATE INDEX IF NOT EXISTS idx_album_photos_album ON album_photos(album_id);
CREATE INDEX IF NOT EXISTS idx_album_photos_photo ON album_photos(photo_id);
```

Bump schema version: change `INSERT INTO schema_version (version) VALUES (11);`
→ `VALUES (12)`.

#### 1b. `src/db/migrations.rs` — add migrate_v11_to_v12

Add to `run_migrations`:

```rust
if current_version < 12 {
    migrate_v11_to_v12(conn)?;
}
```

New function:

```rust
fn migrate_v11_to_v12(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            cover_photo_id INTEGER,
            cover_auto_picked BOOLEAN DEFAULT TRUE,
            photo_count INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (cover_photo_id) REFERENCES photos(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS album_photos (
            id INTEGER PRIMARY KEY,
            album_id INTEGER NOT NULL,
            photo_id INTEGER NOT NULL,
            added_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
            FOREIGN KEY (photo_id) REFERENCES photos(id) ON DELETE CASCADE,
            UNIQUE(album_id, photo_id)
        );

        CREATE INDEX IF NOT EXISTS idx_album_photos_album ON album_photos(album_id);
        CREATE INDEX IF NOT EXISTS idx_album_photos_photo ON album_photos(photo_id);

        INSERT INTO schema_version (version) VALUES (12);
        "#,
    )?;

    tracing::info!("Migrated database to schema version 12 (albums)");
    Ok(())
}
```

**Checkpoint**: `cargo build` passes. Existing DBs auto-migrate on open.

---

### Step 2: Album repository (`album_repo.rs`)

**Files to create:**
- `src/db/album_repo.rs`

**Files to modify:**
- `src/db/mod.rs`

#### 2a. `src/db/album_repo.rs`

Follow the `BurstRepo` pattern: struct wrapping `&Connection`, plain
methods returning `SqliteResult`.

```rust
//! Album database operations

use rusqlite::{params, Connection, Result as SqliteResult};

/// Album record for list views (cover + summary info)
#[derive(Debug, Clone)]
pub struct AlbumRecord {
    pub id: i64,
    pub name: String,
    pub cover_photo_id: Option<i64>,
    pub cover_auto_picked: bool,
    pub photo_count: i64,
    /// Earliest date_taken among album photos (ISO string or None)
    pub date_range_start: Option<String>,
    /// Latest date_taken among album photos (ISO string or None)
    pub date_range_end: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct AlbumRepo<'a> {
    conn: &'a Connection,
}
```

**Methods to implement:**

1. **`new(conn) -> Self`**

2. **`create(name: &str) -> SqliteResult<i64>`**
   ```sql
   INSERT INTO albums (name) VALUES (?1)
   ```
   Return `conn.last_insert_rowid()`.

3. **`rename(album_id: i64, name: &str) -> SqliteResult<()>`**
   ```sql
   UPDATE albums SET name = ?1, updated_at = CURRENT_TIMESTAMP
   WHERE id = ?2
   ```

4. **`delete(album_id: i64) -> SqliteResult<()>`**
   Delete from `album_photos` WHERE album_id, then DELETE from `albums`
   WHERE id. (CASCADE handles it but explicit is clearer.)

5. **`add_photos(album_id: i64, photo_ids: &[i64]) -> SqliteResult<usize>`**
   Loop with INSERT OR IGNORE into `album_photos`. Count actual inserts.
   Then call `update_album_stats(album_id)`. Return count of newly added.

6. **`remove_photos(album_id: i64, photo_ids: &[i64]) -> SqliteResult<()>`**
   Loop with DELETE FROM album_photos WHERE album_id = ?1 AND photo_id = ?2.
   Then call `update_album_stats(album_id)`.

7. **`set_cover(album_id: i64, photo_id: i64) -> SqliteResult<()>`**
   ```sql
   UPDATE albums SET cover_photo_id = ?1, cover_auto_picked = FALSE,
          updated_at = CURRENT_TIMESTAMP
   WHERE id = ?2
   ```

8. **`get_all() -> SqliteResult<Vec<AlbumRecord>>`**
   ```sql
   SELECT a.id, a.name, a.cover_photo_id, a.cover_auto_picked,
          a.photo_count, a.created_at, a.updated_at,
          MIN(p.date_taken) AS date_range_start,
          MAX(p.date_taken) AS date_range_end
   FROM albums a
   LEFT JOIN album_photos ap ON a.id = ap.album_id
   LEFT JOIN photos p ON ap.photo_id = p.id
   GROUP BY a.id
   ORDER BY a.updated_at DESC
   ```

9. **`get_album_photo_ids(album_id: i64) -> SqliteResult<Vec<i64>>`**
   ```sql
   SELECT ap.photo_id FROM album_photos ap
   JOIN photos p ON ap.photo_id = p.id
   WHERE ap.album_id = ?1
   ORDER BY p.date_taken ASC
   ```

10. **`get_albums_for_photo(photo_id: i64) -> SqliteResult<Vec<(i64, String)>>`**
    ```sql
    SELECT a.id, a.name FROM albums a
    JOIN album_photos ap ON a.id = ap.album_id
    WHERE ap.photo_id = ?1
    ORDER BY a.name ASC
    ```
    Returns `(album_id, album_name)` pairs. Used by photo detail view to
    show "In albums: X, Y".

11. **`auto_pick_cover(album_id: i64) -> SqliteResult<()>`**
    Only called when `cover_auto_picked` is true. Picks the best cover:
    ```sql
    SELECT ap.photo_id FROM album_photos ap
    JOIN photos p ON ap.photo_id = p.id
    LEFT JOIN faces f ON p.id = f.photo_id
    WHERE ap.album_id = ?1 AND p.is_trashed = FALSE
    GROUP BY ap.photo_id
    ORDER BY
      COUNT(f.id) > 0 DESC,        -- prefer photos with faces
      p.width > p.height DESC,      -- prefer landscape
      p.date_taken DESC             -- prefer newest
    LIMIT 1
    ```
    Update albums SET cover_photo_id = result, cover_auto_picked = TRUE.

12. **`update_album_stats(album_id: i64) -> SqliteResult<()>`** (private)
    ```sql
    UPDATE albums SET
      photo_count = (SELECT COUNT(*) FROM album_photos WHERE album_id = ?1),
      updated_at = CURRENT_TIMESTAMP
    WHERE id = ?1
    ```
    If `cover_auto_picked` is true, also call `auto_pick_cover`.

#### 2b. `src/db/mod.rs` — register the module

Add:
```rust
pub mod album_repo;
pub use album_repo::{AlbumRecord, AlbumRepo};
```

**Checkpoint**: `cargo build` passes. No UI yet, but repo is testable.

---

### Step 3: State + View enum + Messages

**Files to modify:**
- `src/app/state/mod.rs`
- `src/app/messages.rs`

#### 3a. `src/app/state/mod.rs`

**View enum** — add two variants:

```rust
pub enum View {
    // ... existing variants ...
    Albums,
    AlbumDetail,
}
```

**PhotoVault struct** — add fields (group them under a `// --- Albums ---`
comment):

```rust
// --- Albums ---
/// All albums (loaded from DB, ordered by updated_at DESC)
pub(crate) albums: Vec<crate::db::AlbumRecord>,

/// Currently selected album ID (for detail view)
pub(crate) selected_album_id: Option<i64>,

/// Photos in the currently open album (loaded when entering detail)
pub(crate) album_photos: Vec<Photo>,

/// Whether the album picker overlay is open
pub(crate) album_picker_open: bool,

/// Photo IDs queued for the album picker (the photos being added)
pub(crate) album_picker_target_ids: Vec<i64>,

/// Inline text for creating a new album from the picker
pub(crate) album_picker_new_name: String,

/// Whether the "create new album" input is visible in the picker
pub(crate) album_picker_creating: bool,

/// Album name being edited (rename flow in album detail)
pub(crate) edit_album_name: String,

/// Album ID being renamed (None = not editing)
pub(crate) editing_album_id: Option<i64>,

/// Album names for the currently viewed photo (populated in detail view)
pub(crate) current_photo_albums: Vec<(i64, String)>,
```

**PhotoVault::new()** — add initializers:

```rust
albums: Vec::new(),
selected_album_id: None,
album_photos: Vec::new(),
album_picker_open: false,
album_picker_target_ids: Vec::new(),
album_picker_new_name: String::new(),
album_picker_creating: false,
edit_album_name: String::new(),
editing_album_id: None,
current_photo_albums: Vec::new(),
```

**photo_detail_navigation_list()** — add album case. Insert before the
final `else` branch:

```rust
} else if self.previous_view == Some(View::AlbumDetail) && !self.album_photos.is_empty() {
    &self.album_photos
}
```

**Sidebar active-state** — in the `nav_button` function, the `is_active`
check needs to include AlbumDetail → Albums:

```rust
|| (matches!(target, View::Albums) && matches!(current, View::AlbumDetail))
```

#### 3b. `src/app/messages.rs`

Add at the end of the Message enum (before the closing brace), with a
section comment:

```rust
// --- Albums ---
/// Create a new album with the given name
CreateAlbum(String),
/// Album created; carry the new album ID
AlbumCreated(i64),
/// Rename an existing album
RenameAlbum(i64, String),
/// Delete an album (photos are NOT trashed)
DeleteAlbum(i64),
/// Set the cover photo for an album
SetAlbumCover(i64, i64),
/// Navigate into an album's detail view
OpenAlbum(i64),
/// Album list loaded from DB
AlbumsLoaded(Vec<crate::db::AlbumRecord>),
/// Album photos loaded for the detail view
AlbumPhotosLoaded(Vec<Photo>),
/// Add photos to an existing album
AddPhotosToAlbum(i64, Vec<i64>),
/// Remove photos from an album
RemovePhotosFromAlbum(i64, Vec<i64>),
/// Open the album picker overlay for these photo IDs
OpenAlbumPicker(Vec<i64>),
/// Close the album picker without acting
CloseAlbumPicker,
/// Text changed in the "new album" input inside the picker
AlbumPickerNameChanged(String),
/// Toggle the "create new" input in the picker
AlbumPickerToggleCreate,
/// Create album from picker and add the queued photos to it
AlbumPickerCreateAndAdd,
/// Start editing an album's name (inline rename)
StartEditAlbumName(i64),
/// Album name text changed during editing
EditAlbumName(String),
/// Save the edited album name
SaveAlbumName(i64),
/// Create album from current memory's photos ("Save as album")
SaveMemoryAsAlbum,
/// Return from album detail to albums grid
BackToAlbums,
```

**Checkpoint**: `cargo build` passes (handlers not wired yet, but enums compile).

---

### Step 4: Album loaders

**File to modify:**
- `src/app/state/loaders.rs`

Add two methods to `impl PhotoVault`:

**`load_albums()`** — loads the album list for the grid view:

```rust
pub(crate) fn load_albums(&self) -> Task<Message> {
    let Some(ref drive_path) = self.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();

    Task::perform(
        async move {
            match Database::open_for_drive(&drive_path) {
                Ok(db) => {
                    let repo = AlbumRepo::new(&db.conn);
                    repo.get_all().unwrap_or_default()
                }
                Err(e) => {
                    tracing::error!("Failed to load albums: {}", e);
                    Vec::new()
                }
            }
        },
        Message::AlbumsLoaded,
    )
}
```

**`load_album_photos(album_id)`** — loads photos for the detail view
(full Photo structs with resolved thumbnail paths, same pattern as
`load_photos`):

```rust
pub(crate) fn load_album_photos(&self, album_id: i64) -> Task<Message> {
    let Some(ref drive_path) = self.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();

    Task::perform(
        async move {
            match Database::open_for_drive(&drive_path) {
                Ok(db) => {
                    let album_repo = AlbumRepo::new(&db.conn);
                    let photo_ids = album_repo
                        .get_album_photo_ids(album_id)
                        .unwrap_or_default();

                    if photo_ids.is_empty() {
                        return Vec::new();
                    }

                    let photo_repo = PhotoRepo::new(&db.conn);
                    let mut photos: Vec<Photo> = photo_ids
                        .iter()
                        .filter_map(|id| photo_repo.get_by_id(*id).ok())
                        .flatten()
                        .collect();

                    for photo in &mut photos {
                        if let Some(ref rel_path) = photo.thumbnail_path {
                            let abs_path = drive_path.join(rel_path);
                            photo.thumbnail_path =
                                Some(abs_path.to_string_lossy().to_string());
                        }
                    }

                    photos
                }
                Err(e) => {
                    tracing::error!("Failed to load album photos: {}", e);
                    Vec::new()
                }
            }
        },
        Message::AlbumPhotosLoaded,
    )
}
```

Add needed imports to `loaders.rs`:

```rust
use crate::db::AlbumRepo;
```

**Note**: `PhotoRepo::get_by_id` may not exist yet. If not, add it to
`photo_repo.rs`:

```rust
pub fn get_by_id(&self, id: i64) -> SqliteResult<Option<Photo>> {
    // SELECT * FROM photos WHERE id = ?1
    // Same column mapping as get_all_by_date, but for a single row
}
```

Check if it exists first — it likely does since face detail uses similar
lookups. If not, add it following the existing query_row pattern in
`photo_repo.rs`.

**Checkpoint**: `cargo build` passes.

---

### Step 5: Message handlers (`handlers/albums.rs`)

**Files to create:**
- `src/app/handlers/albums.rs`

**Files to modify:**
- `src/app/handlers/mod.rs`

#### 5a. `src/app/handlers/albums.rs`

Follow the `memories.rs` / `bursts.rs` pattern exactly:

```rust
//! Message handlers for Albums.

use iced::Task;

use crate::db::{AlbumRepo, Database};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};
```

**Functions to implement:**

1. **`albums_loaded(app, albums) -> Task<Message>`**
   ```rust
   app.albums = albums;
   Task::none()
   ```

2. **`create_album(app, name) -> Task<Message>`**
   Open DB, call `AlbumRepo::create(&name)`, get new ID.
   Then reload albums list: return `app.load_albums()`.

3. **`album_created(app, album_id) -> Task<Message>`**
   If `album_picker_open` is true, this was created from the picker —
   add the queued photos:
   ```rust
   if app.album_picker_open {
       let photo_ids = app.album_picker_target_ids.clone();
       app.album_picker_open = false;
       app.album_picker_target_ids.clear();
       app.album_picker_new_name.clear();
       app.album_picker_creating = false;
       return handle(app, Message::AddPhotosToAlbum(album_id, photo_ids));
   }
   app.load_albums()
   ```
   *(Actually, simplify: create_album can do both create + add in one
   handler. No need for AlbumCreated as a separate message. Merge the
   logic into create_album.)*

   **Revised**: Drop `AlbumCreated` message. `create_album` does:
   - Open DB → `AlbumRepo::create(name)` → get album_id
   - If `app.album_picker_open` → also call `add_photos(album_id, target_ids)`
   - Close picker state
   - Reload albums
   - Return `app.load_albums()`

4. **`rename_album(app, album_id, name) -> Task<Message>`**
   Open DB, call `AlbumRepo::rename(album_id, &name)`.
   Update the in-memory `app.albums` entry. Clear editing state.
   Return `Task::none()`.

5. **`delete_album(app, album_id) -> Task<Message>`**
   Open DB, call `AlbumRepo::delete(album_id)`.
   If currently viewing this album's detail, navigate back:
   ```rust
   if app.selected_album_id == Some(album_id) {
       app.selected_album_id = None;
       app.album_photos.clear();
       app.current_view = View::Albums;
   }
   ```
   Return `app.load_albums()`.

6. **`set_album_cover(app, album_id, photo_id) -> Task<Message>`**
   Open DB, call `AlbumRepo::set_cover(album_id, photo_id)`.
   Reload albums list.

7. **`open_album(app, album_id) -> Task<Message>`**
   ```rust
   app.selected_album_id = Some(album_id);
   app.current_view = View::AlbumDetail;
   app.load_album_photos(album_id)
   ```

8. **`album_photos_loaded(app, photos) -> Task<Message>`**
   ```rust
   app.album_photos = photos;
   Task::none()
   ```

9. **`add_photos_to_album(app, album_id, photo_ids) -> Task<Message>`**
   Open DB, call `AlbumRepo::add_photos(album_id, &photo_ids)`.
   Close picker state.
   Clear timeline selection (if any).
   Reload albums list.
   If currently in album detail for this album, also reload album photos:
   ```rust
   let mut tasks = vec![app.load_albums()];
   if app.selected_album_id == Some(album_id) {
       tasks.push(app.load_album_photos(album_id));
   }
   // Clear selection
   app.selected_timeline_photo_ids.clear();
   app.album_picker_open = false;
   app.album_picker_target_ids.clear();
   Task::batch(tasks)
   ```

10. **`remove_photos_from_album(app, album_id, photo_ids) -> Task<Message>`**
    Open DB, call `AlbumRepo::remove_photos(album_id, &photo_ids)`.
    Reload album photos + albums list (to update counts).

11. **`open_album_picker(app, photo_ids) -> Task<Message>`**
    ```rust
    app.album_picker_open = true;
    app.album_picker_target_ids = photo_ids;
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    // Ensure album list is fresh
    app.load_albums()
    ```

12. **`close_album_picker(app) -> Task<Message>`**
    ```rust
    app.album_picker_open = false;
    app.album_picker_target_ids.clear();
    app.album_picker_new_name.clear();
    app.album_picker_creating = false;
    Task::none()
    ```

13. **`album_picker_name_changed(app, name) -> Task<Message>`**
    ```rust
    app.album_picker_new_name = name;
    Task::none()
    ```

14. **`album_picker_toggle_create(app) -> Task<Message>`**
    ```rust
    app.album_picker_creating = !app.album_picker_creating;
    app.album_picker_new_name.clear();
    Task::none()
    ```

15. **`album_picker_create_and_add(app) -> Task<Message>`**
    Same as `create_album` but with `album_picker_open` context.
    Extracts `app.album_picker_new_name`, creates album, adds photos,
    closes picker. One DB transaction.

16. **`start_edit_album_name(app, album_id) -> Task<Message>`**
    ```rust
    let current_name = app.albums.iter()
        .find(|a| a.id == album_id)
        .map(|a| a.name.clone())
        .unwrap_or_default();
    app.editing_album_id = Some(album_id);
    app.edit_album_name = current_name;
    Task::none()
    ```

17. **`edit_album_name(app, name) -> Task<Message>`**
    ```rust
    app.edit_album_name = name;
    Task::none()
    ```

18. **`save_album_name(app, album_id) -> Task<Message>`**
    Calls `rename_album` logic, then clears editing state:
    ```rust
    app.editing_album_id = None;
    ```

19. **`save_memory_as_album(app) -> Task<Message>`**
    ```rust
    let Some(ref memory_id) = app.selected_memory_id else {
        return Task::none();
    };
    let card = app.memories.iter().find(|c| &c.id == memory_id);
    let Some(card) = card else { return Task::none(); };

    let name = card.title.clone();
    let photo_ids = card.photo_ids.clone();

    // Create album and add photos
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        let repo = AlbumRepo::new(&db.conn);
        if let Ok(album_id) = repo.create(&name) {
            let _ = repo.add_photos(album_id, &photo_ids);
            // Navigate to the new album
            app.selected_album_id = Some(album_id);
            app.current_view = View::AlbumDetail;
            return app.load_album_photos(album_id);
        }
    }
    Task::none()
    ```

20. **`back_to_albums(app) -> Task<Message>`**
    ```rust
    app.selected_album_id = None;
    app.album_photos.clear();
    app.current_view = View::Albums;
    app.load_albums()
    ```

#### 5b. `src/app/handlers/mod.rs`

Add module declaration:

```rust
mod albums;
```

Add match arms in the `handle` function. Insert a new section:

```rust
// --- Albums ---
Message::CreateAlbum(name) => albums::create_album(app, name),
Message::AlbumsLoaded(list) => albums::albums_loaded(app, list),
Message::RenameAlbum(id, name) => albums::rename_album(app, id, name),
Message::DeleteAlbum(id) => albums::delete_album(app, id),
Message::SetAlbumCover(aid, pid) => albums::set_album_cover(app, aid, pid),
Message::OpenAlbum(id) => albums::open_album(app, id),
Message::AlbumPhotosLoaded(photos) => albums::album_photos_loaded(app, photos),
Message::AddPhotosToAlbum(aid, pids) => albums::add_photos_to_album(app, aid, pids),
Message::RemovePhotosFromAlbum(aid, pids) => albums::remove_photos_from_album(app, aid, pids),
Message::OpenAlbumPicker(ids) => albums::open_album_picker(app, ids),
Message::CloseAlbumPicker => albums::close_album_picker(app),
Message::AlbumPickerNameChanged(name) => albums::album_picker_name_changed(app, name),
Message::AlbumPickerToggleCreate => albums::album_picker_toggle_create(app),
Message::AlbumPickerCreateAndAdd => albums::album_picker_create_and_add(app),
Message::StartEditAlbumName(id) => albums::start_edit_album_name(app, id),
Message::EditAlbumName(name) => albums::edit_album_name(app, name),
Message::SaveAlbumName(id) => albums::save_album_name(app, id),
Message::SaveMemoryAsAlbum => albums::save_memory_as_album(app),
Message::BackToAlbums => albums::back_to_albums(app),
```

Also remove the `AlbumCreated` message — it's not needed (see step 5a
note above).

**Checkpoint**: `cargo build` passes. Handlers compile but aren't
reachable from UI yet.

---

### Step 6: Navigate handler + sidebar

**Files to modify:**
- `src/app/handlers/scanning.rs` (navigate_to)
- `src/components/sidebar.rs`

#### 6a. `src/app/handlers/scanning.rs`

In `navigate_to`, add an `else if` branch for Albums (before the final
`else`):

```rust
} else if view == View::Albums {
    app.current_view = view;
    return app.load_albums();
}
```

#### 6b. `src/components/sidebar.rs`

In the `nav_items` column, add "Albums" entry. Place it between
"Memories" and "Search" — this is the "Library" section:

```rust
let nav_items = column![
    Self::nav_button("Timeline", View::Timeline, current_view, app_theme),
    Self::nav_button("Map", View::Map, current_view, app_theme),
    Self::nav_button("Memories", View::Memories, current_view, app_theme),
    Self::nav_button("Albums", View::Albums, current_view, app_theme),  // NEW
    Self::nav_button("Search", View::Search, current_view, app_theme),
    Space::with_height(12),
    // ... rest unchanged
];
```

In the `is_active` check inside `nav_button`, add the AlbumDetail case:

```rust
|| (matches!(target, View::Albums) && matches!(current, View::AlbumDetail))
```

**Checkpoint**: Albums appears in sidebar. Clicking it loads the view
(which doesn't render yet).

---

### Step 7: Albums view + album detail view

**Files to create:**
- `src/views/albums.rs`

**Files to modify:**
- `src/views/mod.rs`

#### 7a. `src/views/albums.rs`

This file contains three public functions:

**1. `albums_view()` — grid of album cover cards**

Signature:
```rust
pub fn albums_view(
    albums: &[AlbumRecord],
    drive_path: Option<&std::path::Path>,
    theme: AppTheme,
) -> Element<'static, Message>
```

Layout (follow PeopleView grid pattern):
- Title: "Albums" (size 28)
- "Create Album" button in the top bar (right-aligned). On press:
  opens a flow — simplest v1: the button toggles an inline text_input
  + confirm button right below the title bar. State:
  `app.album_picker_creating` + `app.album_picker_new_name`.
  Actually, we can reuse the album picker state here OR add a
  dedicated "create from albums view" state. Simplest: clicking
  "Create Album" dispatches `Message::AlbumPickerToggleCreate`, and
  if `album_picker_creating` is true, show an inline text_input +
  "Create" button. On submit → `Message::CreateAlbum(name)`.

  **But wait** — the albums_view function doesn't have access to
  `album_picker_creating` or `album_picker_new_name`. We need to
  pass these. Updated signature:

  ```rust
  pub fn albums_view(
      albums: &[AlbumRecord],
      drive_path: Option<&std::path::Path>,
      creating: bool,
      create_name: &str,
      theme: AppTheme,
  ) -> Element<'static, Message>
  ```

- Below title bar: scrollable grid of album cards (4 columns on wide
  screens, responsive like PeopleView).
- Each card:
  - Cover photo thumbnail (160×160, ContentFit::Cover) — resolve path
    from `cover_photo_id`. The cover thumbnail path needs resolving.
    **Better approach**: enrich AlbumRecord with `cover_thumbnail_path:
    Option<String>` during `load_albums`, same pattern as face cluster
    thumbnails. Add this field to AlbumRecord.
  - Album name text below (size 12)
  - Photo count + date range as tertiary text (size 10)
  - On press → `Message::OpenAlbum(album.id)`
- Empty state: "No albums yet. Create one to get started." centered text.

**2. `album_detail_view()` — single album's photos**

Signature:
```rust
pub fn album_detail_view(
    album: &AlbumRecord,
    photos: &[Photo],
    columns: usize,
    is_editing_name: bool,
    edit_name: &str,
    selected_photo_ids: &HashSet<i64>,
    hovered_photo_id: Option<i64>,
    theme: AppTheme,
) -> Element<'static, Message>
```

Layout:
- Back button ("< Albums") → `Message::BackToAlbums`
- Album name as title (size 24). If `is_editing_name`, show
  `text_input` instead (same pattern as cluster name editing).
  Click title → `Message::StartEditAlbumName(album.id)`.
  Submit → `Message::SaveAlbumName(album.id)`.
- Subtitle: "{N} photos · {date range}" (tertiary text)
- Action buttons row: "Rename", "Delete"
  - Rename → `Message::StartEditAlbumName(album.id)`
  - Delete → `Message::DeleteAlbum(album.id)` (direct, no confirm
    needed since photos aren't affected)
- Photo grid: reuse `photo_grid_simple()` from `components/photo_grid.rs`.
  Pass `selected_photo_ids` and `hovered_photo_id` for multi-select.
- Empty state: "This album is empty. Add photos from the timeline."

**3. `album_picker_overlay()` — the "Add to album" modal**

Signature:
```rust
pub fn album_picker_overlay(
    albums: &[AlbumRecord],
    target_count: usize,
    creating: bool,
    create_name: &str,
    theme: AppTheme,
) -> Element<'static, Message>
```

Layout: semi-transparent overlay with centered card.
- Title: "Add {N} photos to album"
- Scrollable list of album rows (cover mini-thumb 32×32 + name + count)
  - On press → `Message::AddPhotosToAlbum(album.id, photo_ids)`
    (photo_ids comes from app state, not the view — so the message
    just carries album_id, and the handler reads
    `app.album_picker_target_ids`)

  **Revised message**: `AddPhotosToAlbum(i64, Vec<i64>)` already
  carries both. But the view doesn't have access to the target IDs.
  Two options:
  - (a) Pass target_ids into the view and embed them in the message
  - (b) Use a simpler message `AlbumPickerSelect(album_id)` and let
    the handler read app state

  Go with **(b)** for cleaner views. Add a new message:

  ```rust
  /// User selected an album from the picker
  AlbumPickerSelect(i64),
  ```

  Handler: reads `app.album_picker_target_ids`, dispatches
  `AddPhotosToAlbum(album_id, target_ids)`.

- "Create new album" row at top (special styling). On press →
  `Message::AlbumPickerToggleCreate`. If `creating` is true, show
  text_input + "Create" button. Submit → `AlbumPickerCreateAndAdd`.
- Close button (X) → `Message::CloseAlbumPicker`

Rendering approach: use `iced::widget::stack![]` to layer the overlay on
top of the current content. The overlay consists of:
- Full-screen container with semi-transparent background (click to
  dismiss)
- Centered card (max 400px wide, 500px tall) with the album list

If `stack` isn't available in iced 0.13, use a `column` approach where
the picker replaces the action bar area (less elegant but functional).

#### 7b. `src/views/mod.rs`

Add:
```rust
pub mod albums;
```

No `pub use` needed — we call `crate::views::albums::albums_view()`
directly.

**Checkpoint**: Views compile. Not wired to app yet.

---

### Step 8: AlbumRecord cover path enrichment

The `AlbumRecord` from the DB doesn't have a resolved thumbnail path.
We need to enrich it during loading, same pattern as face cluster
thumbnails.

**Modify `load_albums()` in `loaders.rs`:**

After fetching albums from the repo, resolve cover paths:

```rust
let mut albums = repo.get_all().unwrap_or_default();

// Resolve cover photo thumbnail paths
let photo_repo = PhotoRepo::new(&db.conn);
for album in &mut albums {
    if let Some(cover_id) = album.cover_photo_id {
        if let Ok(Some(photo)) = photo_repo.get_by_id(cover_id) {
            album.cover_thumbnail_path = photo.thumbnail_path
                .map(|p| drive_path.join(p).to_string_lossy().to_string());
        }
    }
}
```

**Add field to AlbumRecord:**
```rust
/// Resolved absolute thumbnail path for the cover photo (set during loading, not from DB)
pub cover_thumbnail_path: Option<String>,
```

Initialize to `None` in the DB query mapping.

---

### Step 9: Wire views into app/views.rs

**File to modify:**
- `src/app/views.rs`

#### 9a. Add imports

```rust
use crate::views::albums;
```

#### 9b. Add match arms in the content match

After the `View::Memories` arm:

```rust
View::Albums => {
    albums::albums_view(
        &app.albums,
        app.selected_drive.as_deref(),
        app.album_picker_creating,
        &app.album_picker_new_name,
        app.config.theme,
    )
}
View::AlbumDetail => {
    if let Some(album_id) = app.selected_album_id {
        let album = app.albums.iter().find(|a| a.id == album_id);
        if let Some(album) = album {
            let available_width = (app.window_width - 200.0 - 32.0).max(168.0);
            let columns = (available_width / 168.0).floor().max(2.0) as usize;
            albums::album_detail_view(
                album,
                &app.album_photos,
                columns,
                app.editing_album_id == Some(album_id),
                &app.edit_album_name,
                &app.selected_timeline_photo_ids,
                app.hovered_timeline_photo_id,
                app.config.theme,
            )
        } else {
            albums::albums_view(
                &app.albums,
                app.selected_drive.as_deref(),
                app.album_picker_creating,
                &app.album_picker_new_name,
                app.config.theme,
            )
        }
    } else {
        albums::albums_view(
            &app.albums,
            app.selected_drive.as_deref(),
            app.album_picker_creating,
            &app.album_picker_new_name,
            app.config.theme,
        )
    }
}
```

#### 9c. Album picker overlay

The picker overlay needs to render on top of the current view when
`app.album_picker_open` is true. Add this check **after** the content
match, **before** the `main_row` assembly:

```rust
let content = if app.album_picker_open {
    // Wrap current content with the picker overlay on top
    let overlay = albums::album_picker_overlay(
        &app.albums,
        app.album_picker_target_ids.len(),
        app.album_picker_creating,
        &app.album_picker_new_name,
        app.config.theme,
    );
    iced::widget::stack![content, overlay].into()
} else {
    content
};
```

If `iced::widget::stack` is not available in the version used, fall back
to rendering the picker as a `column` above the content (less pretty but
works).

**Checkpoint**: Albums view renders. Navigation works. Creating albums
works via the title bar flow.

---

### Step 10: "Add to Album" from timeline + photo detail

This is where the feature becomes useful. Three integration points.

#### 10a. Timeline selection action bar

**File to modify:**
- `src/app/views.rs`

In the `View::Timeline` arm, inside the `if !selected_timeline_photo_ids.is_empty()`
block, the action bar currently shows "x | N selected | [Delete]".

Add an "Add to Album" button between the count text and the delete
button:

```rust
let album_btn = button(text("Add to Album").size(12).color(p.text_primary))
    .padding([6, 12])
    .style(move |_theme: &iced::Theme, status| button::Style {
        background: Some(match status {
            button::Status::Hovered => accent_hover.into(),
            _ => accent_muted.into(),
        }),
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Message::OpenAlbumPicker(
        app.selected_timeline_photo_ids
            .iter()
            .copied()
            .collect::<Vec<_>>(),
    ));
```

Insert into the `row![]`:
```rust
row![
    clear_btn,
    text(...),
    Space::with_width(Length::Fill),
    album_btn,           // NEW
    Space::with_width(8), // NEW
    delete_btn,
]
```

Do the same for the Documents view action bar (copy-paste the same
button).

#### 10b. Photo detail view top bar

**File to modify:**
- `src/views/photo_detail.rs`

In the top bar row, add an "Album" tool button between "Info" and the
spacer:

```rust
let album_btn = Self::tool_btn(
    "Album",
    Message::OpenAlbumPicker(vec![photo_id]),
    p,
);
```

Insert into the row:
```rust
row![
    rotate_btn,
    Space::with_width(4),
    info_btn,
    Space::with_width(4),
    album_btn,           // NEW
    Space::with_width(Length::Fill),
    trash_btn,
    Space::with_width(8),
    close_btn,
]
```

Also show "In albums: X, Y" in the metadata panel. In the metadata
section (wherever photo info is displayed), add:

```rust
// Show album membership
if !app.current_photo_albums.is_empty() {
    let album_names: Vec<&str> = app.current_photo_albums
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    // Render as a line: "Albums: Vacation, Birthday Party"
}
```

This requires `current_photo_albums` to be populated. Add to
`load_photo_detail_for_index` in `loaders.rs`:

```rust
// Load album membership
self.current_photo_albums.clear();
if let Some(ref db) = self.database {
    let album_repo = AlbumRepo::new(&db.conn);
    if let Ok(albums) = album_repo.get_albums_for_photo(photo_id) {
        self.current_photo_albums = albums;
    }
}
```

#### 10c. Memory detail — "Save as Album" button

**File to modify:**
- `src/views/memories.rs`

In `memory_detail_view`, add a "Save as Album" button in the top
controls area:

```rust
let save_album_btn = button(text("Save as Album").size(12).color(text_primary))
    .padding([6, 12])
    .style(/* accent style */)
    .on_press(Message::SaveMemoryAsAlbum);
```

Place it next to the existing slideshow controls.

**Checkpoint**: The full v1 feature is functional. Users can create
albums, add photos from timeline selection, photo detail, and memory
detail, view albums, open album detail, and navigate between them.

---

### Step 11: Person detail + Album detail action bar

Two remaining integration points.

#### 11a. Person detail — "Add all to Album"

**File to modify:**
- `src/views/people/detail.rs`

Add a button in the cluster detail header:

```rust
let add_album_btn = button(text("Add all to Album").size(12))
    .on_press(Message::OpenAlbumPicker(
        photos.iter().map(|p| p.id).collect()
    ));
```

#### 11b. Album detail — selection action bar for bulk remove

When photos are selected in album detail view (using the same
multi-select mechanism as timeline), show an action bar with:
- "Remove from Album" button → `Message::RemovePhotosFromAlbum(album_id, selected_ids)`
- "Add to Album" button → `Message::OpenAlbumPicker(selected_ids)` (for
  adding to a different album)

This reuses the same `selected_timeline_photo_ids` set and the same
photo grid multi-select behavior. The difference is only in what buttons
appear in the action bar.

---

## File change summary

### New files (2)
| File | Purpose |
|------|---------|
| `src/db/album_repo.rs` | Album + album_photos CRUD |
| `src/views/albums.rs` | Albums grid view + album detail + picker overlay |

### Modified files (9)
| File | Changes |
|------|---------|
| `src/db/schema.rs` | Add albums + album_photos tables, bump version to 12 |
| `src/db/migrations.rs` | Add migrate_v11_to_v12 |
| `src/db/mod.rs` | Register album_repo module + re-exports |
| `src/app/state/mod.rs` | Add View::Albums, View::AlbumDetail, album state fields |
| `src/app/messages.rs` | Add ~18 album Message variants |
| `src/app/handlers/mod.rs` | Register albums module + match arms |
| `src/app/handlers/albums.rs` | (new file, listed above) |
| `src/app/state/loaders.rs` | Add load_albums(), load_album_photos() |
| `src/app/views.rs` | Wire Albums/AlbumDetail views, picker overlay, action bar buttons |
| `src/components/sidebar.rs` | Add "Albums" nav entry |
| `src/views/mod.rs` | Register albums module |
| `src/views/photo_detail.rs` | Add "Album" button to top bar |
| `src/views/memories.rs` | Add "Save as Album" button |
| `src/views/people/detail.rs` | Add "Add all to Album" button |

### Possibly modified (1)
| File | Changes |
|------|---------|
| `src/db/photo_repo.rs` | Add `get_by_id()` if it doesn't exist |

---

## Message flow diagrams

### Create album from Albums view

```
User clicks "Create Album" in Albums view
  → AlbumPickerToggleCreate
  → app.album_picker_creating = true, UI shows text_input
User types name, presses Enter/Create
  → CreateAlbum("My Vacation")
  → handler: DB insert, reload albums
  → AlbumsLoaded(albums)
  → app.albums updated, UI re-renders
```

### Add photos from timeline selection

```
User selects photos in Timeline (existing multi-select)
  → selection bar appears with "Add to Album" button
User clicks "Add to Album"
  → OpenAlbumPicker(vec![photo_id_1, photo_id_2, ...])
  → handler: sets picker state, loads fresh album list
  → AlbumsLoaded(albums)
  → UI renders picker overlay on top of timeline
User clicks an existing album
  → AlbumPickerSelect(album_id)
  → handler: reads target_ids from app state
  → AddPhotosToAlbum(album_id, photo_ids)
  → handler: DB insert, close picker, clear selection, reload albums
  → AlbumsLoaded(albums)
```

### Add photos from picker → Create new

```
User opens picker (from any context)
  → OpenAlbumPicker(photo_ids)
User clicks "Create new album" in picker
  → AlbumPickerToggleCreate
  → UI shows text_input inside picker
User types name, presses Create
  → AlbumPickerCreateAndAdd
  → handler: DB create album, DB add photos, close picker
  → AlbumsLoaded(albums)
```

### Save memory as album

```
User is viewing a memory slideshow
User clicks "Save as Album"
  → SaveMemoryAsAlbum
  → handler: creates album with memory title, adds memory photo_ids
  → navigates to View::AlbumDetail
  → AlbumPhotosLoaded(photos)
```

### Delete album

```
User is in Album detail, clicks "Delete"
  → DeleteAlbum(album_id)
  → handler: DB delete, navigate to View::Albums, reload
  → AlbumsLoaded(albums)
Photos are NOT affected. Only the grouping is removed.
```

---

## Key design decisions

1. **No confirmation dialog for delete**: albums are just groupings.
   Deleting one doesn't touch the actual photos. Low-risk action.

2. **Reuse `selected_timeline_photo_ids`**: the album detail photo grid
   uses the same multi-select infrastructure as timeline. This means
   selections persist across views if the user navigates away, which
   matches existing behavior.

3. **`album_picker_open` is a global overlay**: it can be triggered from
   any view (timeline, photo detail, person detail, memory detail) and
   renders on top of whatever is currently shown.

4. **No drag-and-drop reordering**: photos sort by date_taken ASC. Manual
   ordering is Phase 2 or later polish.

5. **Cover auto-pick runs on every add/remove**: when `cover_auto_picked`
   is true, the best cover is recomputed whenever photos change. This
   keeps covers fresh without user effort. If the user explicitly sets a
   cover, auto-pick stops.

6. **Album photos loaded as full Photo structs**: this allows reuse of
   `photo_grid_simple` without any changes. Same thumbnail resolution
   pattern as timeline.
