import { routeCache } from "./routeCache.svelte";
import { dataRevision } from "./dataRevision.svelte";

class PhotoVisibilityStore {
  trashedIds = $state<Set<number>>(new Set());
  version = $state(0);

  markTrashed(ids: number[]) {
    if (ids.length === 0) return;
    const next = new Set(this.trashedIds);
    for (const id of ids) next.add(id);
    this.trashedIds = next;
    this.version += 1;
    // Trash contents and map pins change the moment a photo is trashed.
    routeCache.invalidate("trash");
    routeCache.invalidate("map");
    // Library totals (insights) and saved search answers are stale too.
    dataRevision.bump();
  }

  markRestored(ids: number[]) {
    if (ids.length === 0) return;
    const next = new Set(this.trashedIds);
    for (const id of ids) next.delete(id);
    this.trashedIds = next;
    this.version += 1;
    routeCache.invalidate("trash");
    routeCache.invalidate("map");
    dataRevision.bump();
  }

  clear() {
    if (this.trashedIds.size === 0) return;
    this.trashedIds = new Set();
    this.version += 1;
  }
}

export const photoVisibility = new PhotoVisibilityStore();
