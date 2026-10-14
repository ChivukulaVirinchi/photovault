/// Justified ("Google Photos") row packing: photos keep their aspect ratio,
/// every full row is scaled to exactly fill the container width, and rows
/// stay close to a target height. The last row of a group is left at the
/// target height instead of being stretched.

export interface Sized {
  width: number | null;
  height: number | null;
  orientation: number;
}

export interface JustifiedRow<T> {
  items: T[];
  /// Pixel width of each item, in order. Full rows sum (plus gaps) to the
  /// container width exactly.
  widths: number[];
  height: number;
}

/// Extreme panoramas and slivers are clamped so one photo can't take a
/// whole row as a thin strip or shrink to a few pixels.
const MIN_ASPECT = 0.5;
const MAX_ASPECT = 2.5;
/// Typical photo aspect, used for photos whose size isn't known yet and for
/// height estimates of rows that aren't loaded.
export const TYPICAL_ASPECT = 4 / 3;

/// Display aspect ratio (width / height) after EXIF orientation. Stored
/// dimensions are the raw sensor size, so orientations 5–8 (rotated 90°)
/// swap them. Unknown sizes render square until metadata arrives.
export function displayAspect(item: Sized): number {
  const { width, height, orientation } = item;
  if (!width || !height || width <= 0 || height <= 0) return 1;
  const rotated = orientation >= 5 && orientation <= 8;
  const aspect = rotated ? height / width : width / height;
  return Math.min(MAX_ASPECT, Math.max(MIN_ASPECT, aspect));
}

export function justifyRows<T extends Sized>(
  items: readonly T[],
  containerWidth: number,
  targetHeight: number,
  gap: number,
): JustifiedRow<T>[] {
  const rows: JustifiedRow<T>[] = [];
  if (items.length === 0 || containerWidth <= 0 || targetHeight <= 0) return rows;

  let row: T[] = [];
  let aspects: number[] = [];
  let aspectSum = 0;

  for (const item of items) {
    const aspect = displayAspect(item);
    row.push(item);
    aspects.push(aspect);
    aspectSum += aspect;
    const naturalWidth = aspectSum * targetHeight + gap * (row.length - 1);
    if (naturalWidth >= containerWidth) {
      const height = (containerWidth - gap * (row.length - 1)) / aspectSum;
      rows.push({ items: row, widths: fitWidths(aspects, height, containerWidth, gap), height: Math.round(height) });
      row = [];
      aspects = [];
      aspectSum = 0;
    }
  }
  if (row.length > 0) {
    rows.push({
      items: row,
      widths: aspects.map((a) => Math.max(1, Math.round(a * targetHeight))),
      height: Math.round(targetHeight),
    });
  }
  return rows;
}

/// Round widths to whole pixels, giving the rounding remainder to the last
/// item so the row lands exactly on the container edge.
function fitWidths(aspects: number[], height: number, containerWidth: number, gap: number): number[] {
  const widths = aspects.map((a) => Math.max(1, Math.round(a * height)));
  const used = widths.reduce((sum, w) => sum + w, 0) + gap * (widths.length - 1);
  widths[widths.length - 1] = Math.max(1, widths[widths.length - 1] + (containerWidth - used));
  return widths;
}

/// Average photos per full row, for estimating the height of photos that
/// aren't loaded yet.
export function estimatedPerRow(containerWidth: number, targetHeight: number, gap: number): number {
  if (containerWidth <= 0 || targetHeight <= 0) return 1;
  return Math.max(1, (containerWidth + gap) / (TYPICAL_ASPECT * targetHeight + gap));
}

/// Index of the item in `widths` whose horizontal span contains `x` (or the
/// nearest one), for moving keyboard focus between rows of uneven widths.
export function indexAtX(widths: readonly number[], gap: number, x: number): number {
  let left = 0;
  let best = 0;
  let bestDistance = Infinity;
  for (let i = 0; i < widths.length; i++) {
    const right = left + widths[i];
    if (x >= left && x <= right) return i;
    const distance = x < left ? left - x : x - right;
    if (distance < bestDistance) {
      bestDistance = distance;
      best = i;
    }
    left = right + gap;
  }
  return best;
}

/// Horizontal centre of item `index` within a row.
export function centerX(widths: readonly number[], gap: number, index: number): number {
  let left = 0;
  for (let i = 0; i < index; i++) left += widths[i] + gap;
  return left + (widths[index] ?? 0) / 2;
}
