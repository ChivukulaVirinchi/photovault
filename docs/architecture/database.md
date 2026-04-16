# Database

PhotoVault uses SQLite via `rusqlite` with migration support.

Core entities include photos, faces, clusters, albums, suggestions,
duplicates, bursts, trash, and search history.

The database is stored with the indexed library for portability.
