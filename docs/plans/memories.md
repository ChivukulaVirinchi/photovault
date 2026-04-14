# Memories — Design & Architecture

Status: **draft for review**. Settle the architectural shape here before
implementation.

---

## Goal

Surface older photos on anniversaries and milestones, the way Google Photos
does "N years ago today". Pure data-surfacing on what we already index. No
new ML, no new heavy tables.

## What ships in v1

Three memory types:

1. **On This Day** — photos from the same calendar date in prior years.
2. **Fallback Window** — photos within ±3 days of today in prior years,
   **only when On This Day is empty**.
3. **Seasonal Recap** — "[Month] [Year]" rollup for past years with
   enough photos.

Deferred: trip retrospectives (needs Auto Albums), person-specific
memories (needs birthday data / year-end triggers), then-and-now,
slideshow / music / video highlights, AI captions.

## UX decisions (locked)

- **Surface locations**: horizontal carousel banner above the Timeline
  grid + dedicated **Memories** entry in the sidebar.
- **Banner style**: Apple-Photos-style horizontal strip, ~60% card width
  so 1.5 cards are visible + hint of next. No auto-rotation.
- **Card click**: opens a filmstrip-style view of that memory's photos
  (not an album entity — just a filtered view).
- **User controls**:
  - Per-person block ("Hide memories of [Alice]"), set from the Person
    detail view *or* from a memory card's menu.
  - Global on/off toggle in Settings.
  - *No per-card dismiss* (rotation handles daily freshness; person
    block handles the real emotional-safety case).
- **Empty library / too-new library**: banner hidden silently. No
  "come back in a year" nag.
- **Card visual**: 16:9 hero photo, caption overlay, small "+N more"
  badge in corner.

---

## Architecture

### Core concepts

```
Memory           — abstract unit: "N photos representing one rediscovery moment"
 ├─ kind         — OnThisDay | FallbackWindow | SeasonalRecap
 ├─ title        — "5 years ago today", "August 2020"
 ├─ subtitle     — "12 photos" or "Italy"
 ├─ photo_ids    — the photos inside
 ├─ hero_id      — pre-selected best photo for the card
 └─ score        — ranking weight

MemoryCard       — UI-facing flattened form (same data, render-ready)
MemoryBlock      — user preference "hide memories of cluster_id / date range"
```

`Memory` is a computed value, not a DB row. It exists only in app state for
the current day. `MemoryBlock` is the only thing that persists.

### Data flow (startup)

```
App launch
   │
   ▼
config::AppConfig::load()
   │
   ▼
Database::open_for_drive(drive)
   │
   ▼
memories::generate_for_today(conn, today) ─────────┐
   │                                                │
   │  (runs 3 generators + ranker + block-filter)   │
   │                                                │
   ▼                                                │
app.memories: Vec<MemoryCard>      ◄────────────────┘
   │
   ▼
Timeline view renders banner from app.memories
Sidebar reflects badge count from app.memories.len()
```

### Data flow (day change)

```
iced subscription (iced::time::every(60_000))
   │
   ▼
Message::MemoriesTick
   │
   ├─ if today == app.memories_for_date → noop
   │
   └─ else → regenerate (same pipeline as startup)
```

Day-rollover detection is a simple `NaiveDate` compare, not a clock-based
trigger — keeps it timezone-safe and survives sleep/wake.

### Data flow (user action — person block)

```
User clicks "Hide memories of Alice" on a memory card / person detail
   │
   ▼
Message::BlockMemoriesForPerson(cluster_id)
   │
   ▼
handler: INSERT INTO memory_blocks (kind='person', key=cluster_id)
   │
   ▼
handler: app.memories.retain(|m| !m.involves_cluster(cluster_id))
   │
   ▼
banner rerenders (Alice-containing cards gone instantly, no regen needed)
```

The block is enforced at generation time too — next day's regen filters out
any memory that heavily features a blocked person.

### Generators — one per memory kind

Each generator is a pure function: `(conn, today) -> Vec<Memory>`.

```
fn on_this_day(conn, today) -> Vec<Memory>
  SQL: SELECT photo_id FROM photos
       WHERE strftime('%m-%d', date_taken) = '<today m-d>'
         AND year(date_taken) < <current year>
         AND is_trashed = FALSE
       GROUP BY year(date_taken)
  One Memory per prior year with >=1 photo.

fn fallback_window(conn, today) -> Vec<Memory>
  Called ONLY if on_this_day returned empty.
  SQL: same, but ±3 days around today's m-d.
  Collapsed into one Memory per prior year.

fn seasonal_recap(conn, today) -> Vec<Memory>
  SQL: SELECT photo_id FROM photos
       WHERE strftime('%Y-%m', date_taken) IN (<today's month, for each prior year>)
         AND is_trashed = FALSE
  One Memory per prior year-month with >= THRESHOLD photos
  (THRESHOLD = 10, tunable).
```

All three queries hit the existing `idx_photos_date` index; combined
startup cost on a 100k-photo library is under 50ms.

### Ranker — decides banner order

Input: `Vec<Memory>` from all generators.
Output: `Vec<MemoryCard>` sorted high-to-low by score.

Score formula (rough):

```
score = log2(photo_count + 1)                 ← bigger collections win
      * sqrt(years_ago + 1)                   ← older wins, sublinear
      * (1.3 if has_faces else 1.0)           ← faces add emotional weight
      * (1.5 if kind == SeasonalRecap else 1) ← seasons get a bump
                                                (they're rarer per date)
```

No ML, pure SQL-derivable signals. Tweakable numbers.

### Hero photo selection — one per memory

Within each `Memory`, the hero is chosen once at generation time by:

1. Not trashed
2. Horizontal orientation (better for 16:9 banner card)
3. Has detected faces
4. Highest face confidence × photo-level sharpness (Laplacian var; already
   computed during face processing for the quality filter)

One scalar sort, pick top. Tie-breaker: most recent date_taken within the
memory.

### Blocks — the only persistent state

```
CREATE TABLE memory_blocks (
    id              INTEGER PRIMARY KEY,
    kind            TEXT NOT NULL,   -- 'person' (v1); 'date_range' (future)
    target_key      TEXT NOT NULL,   -- cluster_id as string, or "mm-dd..mm-dd"
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(kind, target_key)
);
```

Checked at generator time: any Memory whose hero face OR >50% of photo
members belong to a blocked cluster is dropped.

No migrations dance — just add to schema.rs and let the existing
"drop DB if old" strategy for pre-feature DBs handle it.

### UI components

```
MemoriesBanner                ← horizontal strip on Timeline
 └─ for each MemoryCard:
     └─ Card widget
         ├─ hero image (16:9 crop from thumbnail)
         ├─ caption text over gradient
         ├─ "+N more" badge
         └─ on_press → NavigateTo(View::MemoryDetail(memory_id))

MemoriesView                  ← sidebar entry "Memories"
 ├─ all MemoryCards as full-width tiles
 └─ settings link at top: "Hide memories of..."

MemoryDetailView              ← filmstrip view of one memory
 ├─ title, subtitle
 ├─ photo grid (reuse photo_grid_simple)
 └─ menu: "Hide memories of [person in this memory]", "Back"
```

No new iced widgets — composed from `scrollable`, `button`, `image`,
`container` already in the codebase.

### State integration

Fields added to `PhotoVault`:

```rust
memories: Vec<MemoryCard>
memories_for_date: chrono::NaiveDate
memories_selected_index: Option<usize>   // which memory the user clicked
```

Messages added:

```rust
MemoriesTick                              // day-rollover check (subscription)
MemoriesRegenerated(Vec<MemoryCard>)      // async generator result
OpenMemory(MemoryId)                       // banner card click
CloseMemoryDetail
BlockMemoriesForPerson(cluster_id)
SetMemoriesEnabled(bool)
```

### View enum extension

```rust
enum View {
    ...
    Memories,
    MemoryDetail,
    ...
}
```

Plus `selected_memory_id: Option<MemoryId>` in state, same pattern as
cluster detail.

### Module layout

```
src/
  services/
    memories.rs          — generators + ranker + hero selection (pure logic)
  db/
    memory_repo.rs       — memory_blocks CRUD
  views/
    memories.rs          — MemoriesView + MemoriesBanner + MemoryDetailView
  app/
    handlers/
      memories.rs        — message handlers
    messages.rs          — add Message variants
    state/mod.rs         — add View variants + state fields
```

No changes to existing face/thumbnail/scan code. All new code is additive.

### Performance

| Trigger | Work | Budget |
|---------|------|--------|
| App startup | 3 SQL queries + ranking | < 50 ms / 100k photos |
| Day rollover | Same, once per 24 h | Same |
| Block action | 1 INSERT + in-memory filter | < 5 ms |
| Banner render | Loads hero thumbnail paths | Uses existing thumbnail cache |

Memories never block the UI; generator runs on a small `Task::perform`.

### What makes this design feel right

- **No persistent memory table**. Memories are a *view* of existing
  photos, not a second source of truth. No drift, no stale data, no
  migrations.
- **One small user-prefs table** (`memory_blocks`) is the entire
  persistent surface. Easy to reason about.
- **Three generators, one ranker, one hero-selector** — five pure
  functions, each independently testable.
- **Regen-on-day-change** keeps the app state consistent with wall clock
  without requiring a running daemon.
- **Blocks are enforced at two layers** (generator filter + in-memory
  retain) so the user sees instant feedback AND the next day's regen is
  correct.

---

## Open questions (none blocking, worth mulling)

1. **Seasonal recap threshold** — 10 photos per month before a recap is
   generated. Good number? Too low → every month becomes a recap. Too
   high → nothing surfaces for quiet months.

2. **Years-back limit** — should we cap "On This Day" at the last 10
   years? Otherwise someone with a 25-year library gets 25 cards on
   some anniversaries. Proposed: cap at 10, prefer newer.

3. **Hero-selection tie-breakers** — current design favors horizontal
   orientation. On phone-heavy libraries (mostly portrait), this may
   starve the banner of good heroes. Alternative: crop portrait photos
   center-top for the banner. Defer.

4. **Should Memories appear in Search results** when user searches for
   a date? (e.g., "Christmas 2022" matches a Seasonal Recap.) Probably
   yes — surface the memory card inline in search results. Light
   integration.

---

## What this plan explicitly does NOT cover

- Implementation line-by-line. That comes after all 5 Tier 1 features are
  designed, as one coordinated implementation pass.
- Exact SQL syntax — pseudocode only, real queries at implementation time.
- Exact iced widget arrangement — text description only.
- Visual design / colors / typography — follows existing theme.
