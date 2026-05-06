/// browseContext — the ordered list of photo IDs that PhotoDetail uses
/// for prev/next navigation.
///
/// When a "source" view (Timeline, Album, Person, Memory, Search,
/// Documents, Trash, etc.) loads its photos, it `set()`s the ordered
/// IDs here. PhotoDetail then reads `prev(id)` / `next(id)` to know
/// which photo comes before/after the one currently open.
///
/// If the user navigates past the loaded edge, the source view is
/// expected to fetch more and call `extend()`. (Not all source views
/// page; that's fine — prev/next will simply return null at the edges.)

class BrowseContext {
  ids = $state<number[]>([]);
  /// Identifier for which view set this context, so a stale source-view's
  /// data doesn't accidentally drive nav after the user has moved on.
  source = $state<string | null>(null);

  set(source: string, ids: number[]) {
    this.source = source;
    this.ids = ids;
  }

  extend(ids: number[]) {
    // Append, deduping IDs we already have (safety against re-fetched pages).
    const seen = new Set(this.ids);
    const fresh = ids.filter((id) => !seen.has(id));
    if (fresh.length > 0) this.ids = [...this.ids, ...fresh];
  }

  clear() {
    this.ids = [];
    this.source = null;
  }

  indexOf(id: number): number {
    return this.ids.indexOf(id);
  }

  prev(id: number): number | null {
    const i = this.indexOf(id);
    if (i <= 0) return null;
    return this.ids[i - 1];
  }

  next(id: number): number | null {
    const i = this.indexOf(id);
    if (i < 0 || i >= this.ids.length - 1) return null;
    return this.ids[i + 1];
  }

  position(id: number): { index: number; total: number } | null {
    const i = this.indexOf(id);
    if (i < 0) return null;
    return { index: i + 1, total: this.ids.length };
  }
}

export const browseContext = new BrowseContext();
