class PhotoVisibilityStore {
  trashedIds = $state<Set<number>>(new Set());
  version = $state(0);

  markTrashed(ids: number[]) {
    if (ids.length === 0) return;
    const next = new Set(this.trashedIds);
    for (const id of ids) next.add(id);
    this.trashedIds = next;
    this.version += 1;
  }

  markRestored(ids: number[]) {
    if (ids.length === 0) return;
    const next = new Set(this.trashedIds);
    for (const id of ids) next.delete(id);
    this.trashedIds = next;
    this.version += 1;
  }

  clear() {
    if (this.trashedIds.size === 0) return;
    this.trashedIds = new Set();
    this.version += 1;
  }
}

export const photoVisibility = new PhotoVisibilityStore();
