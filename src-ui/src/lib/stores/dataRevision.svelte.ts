/// Library-data revision counter.
///
/// A single number that increases whenever the library's *data* changes
/// in a way that can invalidate a route's saved answers — completed
/// scan/metadata/faces/geocoding/takeout jobs and trash/restore
/// mutations. Screens that keep long-lived caches (Insights, Search)
/// capture the revision alongside the session and their query; when
/// the revision moves, their cached answers are stale and must be
/// refreshed (stale-while-revalidate: show the old answer immediately,
/// load the new one in the background).
///
/// This is deliberately coarse: pixel-only work (thumbnails, asset
/// installs) never bumps it, so unrelated progress does not trigger
/// recomputation.
class DataRevisionStore {
  version = $state(0);

  bump() {
    this.version += 1;
  }
}

export const dataRevision = new DataRevisionStore();
