/// Cross-grid photo selection store.
///
/// Holds the set of selected photo IDs and the last-clicked anchor for
/// shift-range selection. Sets are not deeply reactive in Svelte 5, so
/// every mutation builds a fresh `Set` and reassigns the $state.

class SelectionStore {
  ids = $state<Set<number>>(new Set());
  /// Last single-clicked id; serves as the anchor for shift-range
  /// selections. Cleared along with the selection.
  anchor = $state<number | null>(null);

  has(id: number): boolean {
    return this.ids.has(id);
  }
  size(): number {
    return this.ids.size;
  }
  list(): number[] {
    return Array.from(this.ids);
  }
  listIn(allIds: number[]): number[] {
    return allIds.filter((id) => this.ids.has(id));
  }
  active(): boolean {
    return this.ids.size > 0;
  }

  toggle(id: number) {
    const next = new Set(this.ids);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.ids = next;
    this.anchor = id;
  }

  add(id: number) {
    if (this.ids.has(id)) return;
    const next = new Set(this.ids);
    next.add(id);
    this.ids = next;
    this.anchor = id;
  }

  /// Replace the selection with a single id.
  set(id: number) {
    this.ids = new Set([id]);
    this.anchor = id;
  }

  /// Replace the selection with a prepared set/list while keeping the
  /// range-selection anchor valid.
  replace(ids: Iterable<number>) {
    const next = new Set(ids);
    this.ids = next;
    if (this.anchor != null && next.has(this.anchor)) return;
    this.anchor = next.values().next().value ?? null;
  }

  /// Select an inclusive range from `anchor` (last-clicked) to `id`,
  /// using the provided ordered list of all visible ids. Falls back
  /// to a single-id select if no anchor is set.
  range(id: number, allIds: number[]) {
    if (this.anchor == null) {
      this.set(id);
      return;
    }
    const a = allIds.indexOf(this.anchor);
    const b = allIds.indexOf(id);
    if (a < 0 || b < 0) {
      this.set(id);
      return;
    }
    const lo = Math.min(a, b);
    const hi = Math.max(a, b);
    const next = new Set(this.ids);
    for (let i = lo; i <= hi; i++) next.add(allIds[i]);
    this.ids = next;
    // Anchor stays put — successive shift-clicks expand from the same
    // origin, matching macOS / Photos.app behaviour.
  }

  clear() {
    this.ids = new Set();
    this.anchor = null;
  }
}

export const selection = new SelectionStore();

/// Cell-click handler shared by every grid that supports multi-select.
/// Returns true if the click was consumed by selection — the caller
/// should NOT navigate in that case.
export function handleCellClick(
  e: MouseEvent,
  photoId: number,
  allIds: number[],
): boolean {
  if (e.shiftKey) {
    e.preventDefault();
    selection.range(photoId, allIds);
    return true;
  }
  if (e.ctrlKey || e.metaKey) {
    e.preventDefault();
    selection.toggle(photoId);
    return true;
  }
  if (selection.active()) {
    e.preventDefault();
    selection.toggle(photoId);
    return true;
  }
  return false;
}
