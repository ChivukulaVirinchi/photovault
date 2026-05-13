export type SlideDirection = "next" | "prev";

export function uniquePhotoIds(ids: number[]): number[] {
  const seen = new Set<number>();
  const out: number[] = [];
  for (const id of ids) {
    if (!Number.isFinite(id) || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

export function resolveStartIndex(ids: number[], startId?: number | null): number {
  if (ids.length === 0) return -1;
  if (startId == null) return 0;
  const i = ids.indexOf(startId);
  return i >= 0 ? i : 0;
}

export function moveIndex(
  current: number,
  total: number,
  direction: SlideDirection,
  loop: boolean,
): number {
  if (total <= 0) return -1;
  const clamped = Math.max(0, Math.min(current, total - 1));
  if (direction === "next") {
    if (clamped < total - 1) return clamped + 1;
    return loop ? 0 : clamped;
  }
  if (clamped > 0) return clamped - 1;
  return loop ? total - 1 : clamped;
}

export function shouldLoadMore(index: number, total: number, threshold = 8): boolean {
  if (total <= 0) return false;
  return total - index <= threshold;
}
