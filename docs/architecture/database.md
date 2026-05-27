# Database

Smriti uses a single SQLite database per library, stored at
`<library>/.photovault/photovault.db`. Access goes through `rusqlite`
with a small `Database` wrapper and per-table repository modules in
`src/db/`.

## Where it lives

The DB sits on the same drive as the photos. This is deliberate:
the library is portable — unplug the drive, plug it elsewhere, open
Smriti, point it at the new mount path, and everything resumes.
Faces, albums, tags, search history all travel with the photos.

(The folder is named `.photovault/` for backwards compatibility with
libraries indexed under the project's earlier name.)

## Migrations

Schema migrations live in `src/db/migrations/`. Each migration is an
SQL file; `Database::open` applies any unapplied migrations in
order. There's no "rollback" — migrations are forward-only and must
remain idempotent across versions.

## Core tables

| Table | What it stores |
|---|---|
| `photos` | The canonical photo record: path, hash, EXIF, GPS, dates, dimensions. |
| `albums` | Manual + accepted-suggestion albums. |
| `album_photos` | Join table; ordered for manual album reordering. |
| `album_suggestions` | Detected-but-not-accepted trip / event suggestions. |
| `faces` | Per-photo detected faces: bbox, embedding, quality score. |
| `clusters` (a.k.a. `persons`) | Named identity groups; one row per person. |
| `face_assignments` | Which face belongs to which cluster, with confidence. |
| `duplicates` | Exact and perceptual duplicate groups. |
| `bursts` | Detected burst groups with timestamps. |
| `photo_stacks` / `photo_stack_members` | Timeline stack suggestions derived from duplicate and burst groups, with cover photo and per-member quality scores. |
| `memory_blocks` | User dismissals for surfaced memories. |
| `trash` (via `photos.is_trashed`) | Soft-deleted photos. |
| `search_history` | Recent search queries, capped. |

## Conventions

- All tables use `INTEGER PRIMARY KEY` row IDs (SQLite's implicit
  `rowid` aliases).
- Timestamps are stored as RFC-3339 strings in UTC.
- File paths are stored as `relative_path` from the library root, so
  moving the library to a new mount point doesn't break references.
- Soft-delete is the default — `is_trashed = TRUE` on `photos` rather
  than `DELETE FROM photos`. Permanent deletion is a separate
  explicit action.

## Repositories

`src/db/<entity>_repo/` modules provide typed access to each table.
Repos are split into `read.rs` (queries) and `write.rs` (mutations)
so the read side can be borrowed independently from the write side
when needed.

Engine code uses repos directly; the Tauri shell never touches the DB
directly — it goes through the engine services.

## Concurrency

SQLite is opened with **WAL mode** for concurrent read-while-write
performance. Smriti spawns one writer task and many reader tasks.
The async scanner and frontend reads coexist through the WAL.

## Backup

The library folder including `.photovault/` is the entire database.
Standard file-level backup works. SQLite is robust to crash — WAL
ensures the DB stays consistent across power loss.

## See also

- [Indexing](../user-guide/indexing.md) — what populates these tables.
- [Services](services.md) — the layer that reads / writes them.
