# Phase 2: Bug Fixes & Data Integrity

## Overview

9 bugs/correctness issues, ordered from simplest to most complex.

---

## Bug 1: Non-atomic `set_suggested_best` in burst_repo

**File:** `src/db/burst_repo.rs` lines 83-96
**Problem:** Two separate UPDATEs (clear all -> set one) without a transaction. A crash between them leaves no suggestion set.

**Fix:** Wrap in transaction:
```rust
pub fn set_suggested_best(&self, group_id: i64, photo_id: i64) -> SqliteResult<()> {
    let tx = self.conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE burst_group_members SET is_suggested_best = FALSE WHERE group_id = ?1",
        params![group_id],
    )?;
    tx.execute(
        "UPDATE burst_group_members SET is_suggested_best = TRUE 
         WHERE group_id = ?1 AND photo_id = ?2",
        params![group_id, photo_id],
    )?;
    tx.commit()
}
```

**Also fix:** `src/db/duplicate_repo.rs` — same pattern for `set_keep_photo` if present.

---

## Bug 2: Silent embedding deserialization failures

**File:** `src/db/face_repo.rs` lines 84, 110, 134, and ~4 other locations
**Problem:** `FaceEmbedding::from_bytes(&bytes)` returns `None` on corruption — face silently dropped, no logging, no metrics.

**Fix:** At every `if let Some(emb) = FaceEmbedding::from_bytes(...)` pattern, change to:
```rust
match FaceEmbedding::from_bytes(&bytes) {
    Some(emb) => { /* existing push logic */ },
    None => {
        tracing::warn!(
            "Corrupted face embedding for face_id={}: {} bytes (expected {})",
            id, bytes.len(), 512 * 4
        );
    }
}
```

Locations to change (all in `face_repo.rs`):
- `get_all_faces_with_embeddings` (~line 84)
- `get_all_faces_with_photo_embeddings` (~line 110)
- `get_unclustered_faces_with_embeddings` (~line 134)
- `get_unclustered_faces_with_photo_embeddings` (~line 159)
- `get_cluster_centroids` (~line 189)
- `get_gallery_embeddings` (~line 292)
- `refresh_gallery_tx` (~line 590)

---

## Bug 3: Config validation missing

**File:** `src/config/mod.rs`
**Problem:** After JSON deserialization (line 53), values are used unchecked. Hand-edited config with `face_detection_confidence: 50.0` would cause bizarre behavior.

**Fix:** Add `validate()` method, call after deserialization:

```rust
impl AppConfig {
    fn validate(&mut self) {
        self.face_detection_confidence = self.face_detection_confidence.clamp(0.1, 0.95);
        self.face_clustering_threshold = self.face_clustering_threshold.clamp(0.1, 0.8);
        self.thumbnail_size = self.thumbnail_size.clamp(100, 1000);
        self.burst_time_window_seconds = self.burst_time_window_seconds.max(1).min(30);
        self.trash_auto_delete_days = self.trash_auto_delete_days.max(1).min(365);
        self.window_width = self.window_width.clamp(400, 7680);
        self.window_height = self.window_height.clamp(300, 4320);
    }

    pub fn load() -> Self {
        // ... existing deserialization ...
        let mut cfg = /* deserialized value */;
        cfg.validate();
        cfg
    }
}
```

---

## Bug 4: Photo sort after fetch negates DB ORDER BY

**File:** `src/db/photo_repo.rs` line ~205 and ~219
**Problem:** `get_by_ids` fetches photos in chunks of 900 with `ORDER BY date_taken DESC` per chunk, then re-sorts the entire result at line 219. The per-chunk ORDER BY is wasted work.

**Fix:** Remove `ORDER BY` from the chunked SQL query (line ~205). The final in-memory sort at line 219 is the actual sort:

```rust
// In the SQL query, remove: ORDER BY date_taken DESC
// The line 219 sort handles it:
all.sort_by(|a, b| b.date_taken.cmp(&a.date_taken));
```

---

## Bug 5: Contextual identity brightness — hard cutoff

**File:** `src/services/face_processor.rs` lines 344-352
**Problem:** Hard threshold of `0.12` brightness difference is arbitrary. Scene changes can have 40%+ difference, same scene with exposure change can differ by 20%.

**Fix:** Replace hard cutoff with smooth falloff:

```rust
// Replace the binary threshold check with a gradient:
let brightness_diff = (target_brightness - source_brightness).abs();
// Full 0.1 bonus at identical brightness, tapering to 0 at 0.3 difference
let brightness_bonus = 0.1 * (1.0 - (brightness_diff / 0.3).clamp(0.0, 1.0));
confidence += brightness_bonus;
```

This is a soft signal instead of a gate — a large brightness difference reduces the bonus to zero but never blocks the match entirely.

---

## Bug 6: Burst detection threshold too strict

**File:** `src/services/burst_detector.rs` line 46
**Problem:** `similarity_threshold: 0.90` cosine similarity on 32x32 grayscale signatures. Even slight zoom/pan in a burst drops below 0.90, splitting legitimate groups.

**Fix:**
```rust
// Line 46: lower threshold
similarity_threshold: 0.80,  // was 0.90
```

Also consider increasing signature resolution from 32x32 to 48x48 for better discrimination at the lower threshold:

```rust
// In the signature computation function:
let gray = img.grayscale()
    .resize_exact(48, 48, FilterType::Triangle)  // was 32, 32
    .to_luma8();
```

---

## Bug 7: Reindexer loads entire DB into memory

**File:** `src/services/reindexer.rs` lines 80-96
**Problem:** All photos loaded into `HashMap<String, (i64, String, Option<String>)>`. OOM risk on large libraries (100k+ photos).

**Fix:** Use a temporary SQLite table for the diff instead of in-memory HashMap:

```rust
pub fn detect_changes(&self, conn: &Connection, drive_root: &Path) -> SqliteResult<IndexChanges> {
    // Create temp table for filesystem state
    conn.execute_batch("
        CREATE TEMP TABLE IF NOT EXISTS found_files (
            path TEXT PRIMARY KEY,
            mtime TEXT
        );
        DELETE FROM found_files;
    ")?;

    // Walk filesystem, insert paths into temp table
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO found_files VALUES (?1, ?2)")?;
    for entry in WalkDir::new(drive_root)... {
        stmt.execute(params![relative_path, mtime_str])?;
    }

    // Added: files on disk not in DB
    let added = conn.prepare("
        SELECT f.path FROM found_files f
        LEFT JOIN photos p ON p.file_path = f.path
        WHERE p.id IS NULL
    ")?;

    // Removed: files in DB not on disk
    let removed = conn.prepare("
        SELECT p.id, p.file_path FROM photos p
        LEFT JOIN found_files f ON f.path = p.file_path
        WHERE f.path IS NULL AND p.is_trashed = FALSE
    ")?;

    // Modified: matching paths with different mtime
    let modified = conn.prepare("
        SELECT p.id, p.file_path FROM photos p
        INNER JOIN found_files f ON f.path = p.file_path
        WHERE f.mtime > p.updated_at
    ")?;

    // Cleanup
    conn.execute("DROP TABLE IF EXISTS temp.found_files", [])?;

    Ok(IndexChanges { added, removed, modified })
}
```

Memory usage becomes O(1) in Rust, O(N) in SQLite temp storage (disk-backed for large sets).

---

## Bug 8: Burst/duplicate sync loses user resolutions

**Files:** `src/db/burst_repo.rs` lines 65-79, `src/db/duplicate_repo.rs` lines 36-73
**Problem:** `sync_burst_groups` and `sync_duplicate_groups` DELETE ALL groups then reinsert. User decisions (`is_suggested_best`, `resolved` flags) are lost.

**Fix — Merge-based sync for burst_repo:**

```rust
pub fn sync_burst_groups(
    &self,
    groups: &[(String, String, Vec<i64>)],
) -> SqliteResult<()> {
    let tx = self.conn.unchecked_transaction()?;

    // Load existing groups with their photo_id sets
    let existing: HashMap<BTreeSet<i64>, i64> = self.get_existing_group_sets(&tx)?;
    let mut seen_sets: HashSet<BTreeSet<i64>> = HashSet::new();

    for (start_time, end_time, photo_ids) in groups {
        let set: BTreeSet<i64> = photo_ids.iter().copied().collect();
        seen_sets.insert(set.clone());

        if existing.contains_key(&set) {
            continue; // Group exists — preserve user decisions
        }
        // New group — create it
        self.create_group_tx(&tx, start_time, end_time, photo_ids)?;
    }

    // Remove groups that no longer match any detection
    for (set, group_id) in &existing {
        if !seen_sets.contains(set) {
            self.delete_group_tx(&tx, *group_id)?;
        }
    }

    tx.commit()
}
```

**Fix — Same pattern for duplicate_repo:**

```rust
pub fn sync_duplicate_groups(
    &self,
    groups: &[(String, Vec<i64>, Option<i64>)],
) -> SqliteResult<()> {
    let tx = self.conn.unchecked_transaction()?;

    // Load existing groups by hash
    let existing: HashMap<String, i64> = self.get_existing_group_hashes(&tx)?;
    let mut seen_hashes: HashSet<String> = HashSet::new();

    for (hash, photo_ids, suggested_keep) in groups {
        seen_hashes.insert(hash.clone());

        if existing.contains_key(hash) {
            continue; // Group exists — keep user's keep/dismiss choices
        }
        // New duplicate group
        self.create_group_tx(&tx, hash, photo_ids, *suggested_keep)?;
    }

    // Remove groups whose hash no longer has duplicates
    for (hash, group_id) in &existing {
        if !seen_hashes.contains(hash) {
            self.delete_group_tx(&tx, *group_id)?;
        }
    }

    tx.commit()
}
```

Helper methods needed:
- `get_existing_group_sets(&tx)` — returns `HashMap<BTreeSet<i64>, i64>` of photo_id sets -> group_id
- `get_existing_group_hashes(&tx)` — returns `HashMap<String, i64>` of hash -> group_id
- `create_group_tx(&tx, ...)` — creates group within transaction
- `delete_group_tx(&tx, group_id)` — deletes group and members within transaction

---

## Order of Implementation

| Step | Bug | Complexity | Risk |
|------|-----|------------|------|
| 1 | Bug 1 — atomic set_suggested_best | Trivial | None |
| 2 | Bug 2 — embedding deserialization logging | Trivial | None |
| 3 | Bug 3 — config validation | Simple | None |
| 4 | Bug 4 — photo sort cleanup | Simple | None |
| 5 | Bug 5 — brightness soft falloff | Simple | Low |
| 6 | Bug 6 — burst threshold | Simple | Needs testing |
| 7 | Bug 7 — reindexer memory | Moderate | Medium |
| 8 | Bug 8 — sync preserving user decisions | Moderate | Medium |

## Verification

| Bug | Test |
|-----|------|
| 1 | Call `set_suggested_best`, verify both UPDATEs in single transaction |
| 2 | Corrupt an embedding in DB (`UPDATE faces SET embedding = X'DEAD'`), run clustering, verify warning in logs |
| 3 | Unit test: `validate()` clamps out-of-range values correctly |
| 4 | `cargo test` — existing behavior unchanged |
| 5 | Process photos with known brightness differences, verify inferred identities improve |
| 6 | Test burst detection on a known burst set with slight reframing — verify grouped |
| 7 | Test `detect_changes` on mock dir with 50K entries, verify flat memory usage |
| 8 | Set `is_suggested_best` manually, re-run sync, verify preserved |
