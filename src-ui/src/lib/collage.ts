/**
 * Collage layout and rendering. Pure functions over plain numbers so the
 * geometry is testable without a DOM; `renderCollage` is the only part that
 * touches a canvas.
 *
 * A layout is a split tree: rows and columns of weighted children, with
 * photo slots at the leaves. One leaf may be marked as the feature, which
 * always takes the first photo. Every count from 2 to 9 also gets a
 * "Mosaic" that packs the photos in rows (or columns) chosen so each photo
 * keeps as close to its own shape as the canvas allows.
 */

export const MIN_PHOTOS = 2;
export const MAX_PHOTOS = 9;
/** Long side of the saved image, in pixels. */
export const EXPORT_LONG_SIDE = 3000;

export type ShapeId = "square" | "portrait" | "story" | "landscape";
export const SHAPES: { id: ShapeId; label: string; ratio: number }[] = [
  { id: "square", label: "1:1", ratio: 1 },
  { id: "portrait", label: "4:5", ratio: 4 / 5 },
  { id: "story", label: "9:16", ratio: 9 / 16 },
  { id: "landscape", label: "3:2", ratio: 3 / 2 },
];

export type BorderId = "white" | "paper" | "black" | "match";
export const BORDERS: { id: BorderId; label: string; color: string | null }[] = [
  { id: "white", label: "White", color: "#ffffff" },
  { id: "paper", label: "Paper", color: "#f3eee4" },
  { id: "black", label: "Black", color: "#111214" },
  { id: "match", label: "Match", color: null },
];

export interface Rect { x: number; y: number; w: number; h: number }
export interface FaceBox { x: number; y: number; width: number; height: number }
/** Where a photo sits in its cell: the image point at the cell centre
 *  (normalised 0..1) and a zoom of 1 or more over a plain cover fit. */
export interface Focus { x: number; y: number; zoom: number }

type Node =
  | { kind: "leaf"; feature?: boolean }
  | { kind: "row" | "col"; children: { weight: number; node: Node }[] };

type Item = Node | [number, Node];
const S: Node = { kind: "leaf" };
const F: Node = { kind: "leaf", feature: true };
const split = (kind: "row" | "col") => (...items: Item[]): Node => ({
  kind,
  children: items.map((item) => (Array.isArray(item) ? { weight: item[0], node: item[1] } : { weight: 1, node: item })),
});
const row = split("row");
const col = split("col");

export interface Layout {
  id: string;
  name: string;
  tree: Node | null; // null = mosaic
}

const mosaic: Layout = { id: "mosaic", name: "Mosaic", tree: null };
const featureBlock = row([2, F], col(S, S)); // 3 photos: one large, two stacked

const LAYOUTS: Record<number, Layout[]> = {
  2: [
    { id: "2-side", name: "Side by side", tree: row(S, S) },
    { id: "2-stack", name: "Stacked", tree: col(S, S) },
    { id: "2-feature", name: "Feature", tree: row([2, F], S) },
  ],
  3: [
    { id: "3-left", name: "Feature left", tree: row([2, F], col(S, S)) },
    { id: "3-top", name: "Feature top", tree: col([1.6, F], row(S, S)) },
    { id: "3-across", name: "Three across", tree: row(S, S, S) },
    { id: "3-down", name: "Three down", tree: col(S, S, S) },
  ],
  4: [
    { id: "4-grid", name: "Grid", tree: col(row(S, S), row(S, S)) },
    { id: "4-left", name: "Feature left", tree: row([2, F], col(S, S, S)) },
    { id: "4-top", name: "Feature top", tree: col([2, F], row(S, S, S)) },
    { id: "4-offset", name: "Offset", tree: row(col([3, S], [2, S]), col([2, S], [3, S])) },
    { id: "4-strips", name: "Strips", tree: row(S, S, S, S) },
  ],
  5: [
    { id: "5-two-three", name: "Two over three", tree: col([1.25, row(S, S)], row(S, S, S)) },
    { id: "5-left", name: "Feature left", tree: row([1.3, F], col(row(S, S), row(S, S))) },
    { id: "5-centre", name: "Feature centre", tree: row(col(S, S), [1.6, F], col(S, S)) },
  ],
  6: [
    { id: "6-grid", name: "Grid", tree: col(row(S, S, S), row(S, S, S)) },
    { id: "6-tall", name: "Tall grid", tree: col(row(S, S), row(S, S), row(S, S)) },
    { id: "6-feature", name: "Feature", tree: col([2, featureBlock], row(S, S, S)) },
  ],
  7: [
    { id: "7-feature", name: "Feature", tree: col([2, featureBlock], row(S, S, S, S)) },
    { id: "7-rows", name: "Three over four", tree: col(row(S, S, S), row(S, S, S, S)) },
    { id: "7-centre", name: "Feature centre", tree: row(col(S, S, S), [1.6, F], col(S, S, S)) },
  ],
  8: [
    { id: "8-grid", name: "Grid", tree: col(row(S, S, S, S), row(S, S, S, S)) },
    { id: "8-feature", name: "Feature", tree: col([2, featureBlock], row(S, S, S, S, S)) },
    { id: "8-centre", name: "Feature centre", tree: row(col(S, S, S, S), [1.6, F], col(S, S, S)) },
  ],
  9: [
    { id: "9-grid", name: "Grid", tree: col(row(S, S, S), row(S, S, S), row(S, S, S)) },
    { id: "9-feature", name: "Feature", tree: col([2, featureBlock], row(S, S, S), row(S, S, S)) },
  ],
};

/** Layouts offered for exactly `count` photos, always ending with Mosaic. */
export function layoutsFor(count: number): Layout[] {
  const n = Math.min(MAX_PHOTOS, Math.max(MIN_PHOTOS, count));
  return [...LAYOUTS[n], mosaic];
}

function leafCount(node: Node): number {
  return node.kind === "leaf" ? 1 : node.children.reduce((sum, c) => sum + leafCount(c.node), 0);
}

/**
 * Cell rectangles in canvas pixels, indexed by photo slot. `gap` is the
 * space between cells and around the edge.
 */
export function layoutRects(layout: Layout, width: number, height: number, gap: number, aspects: number[]): Rect[] {
  const inner: Rect = { x: gap, y: gap, w: Math.max(1, width - 2 * gap), h: Math.max(1, height - 2 * gap) };
  if (!layout.tree) return mosaicRects(inner, gap, aspects);

  const leaves: { rect: Rect; feature: boolean }[] = [];
  const walk = (node: Node, r: Rect) => {
    if (node.kind === "leaf") {
      leaves.push({ rect: r, feature: node.feature === true });
      return;
    }
    const horizontal = node.kind === "row";
    const total = node.children.reduce((sum, c) => sum + c.weight, 0);
    const span = (horizontal ? r.w : r.h) - gap * (node.children.length - 1);
    let offset = horizontal ? r.x : r.y;
    for (const child of node.children) {
      const size = (span * child.weight) / total;
      walk(child.node, horizontal
        ? { x: offset, y: r.y, w: size, h: r.h }
        : { x: r.x, y: offset, w: r.w, h: size });
      offset += size + gap;
    }
  };
  walk(layout.tree, inner);
  // The feature leaf takes the first photo; the rest fill in reading order.
  const feature = leaves.filter((leaf) => leaf.feature);
  return [...feature, ...leaves.filter((leaf) => !leaf.feature)].map((leaf) => leaf.rect);
}

/**
 * Try every way of splitting the photos (in order) into consecutive rows,
 * and into consecutive columns. Each line is sized so its photos keep their
 * shape exactly; the lines are then stretched to fill the canvas. Pick the
 * split that needs the least stretch, which is the least cropping.
 */
function mosaicRects(area: Rect, gap: number, aspects: number[]): Rect[] {
  const n = aspects.length;
  if (n === 0) return [];
  const safe = aspects.map((a) => (Number.isFinite(a) && a > 0 ? a : 1));
  let best: { cost: number; rects: Rect[] } | null = null;

  for (const transpose of [false, true]) {
    // Work in "rows of width W": columns are rows of the transposed canvas.
    const W = transpose ? area.h : area.w;
    const H = transpose ? area.w : area.h;
    const ratios = transpose ? safe.map((a) => 1 / a) : safe;
    for (let mask = 0; mask < 1 << (n - 1); mask++) {
      const lines: number[][] = [[0]];
      for (let i = 1; i < n; i++) {
        if (mask & (1 << (i - 1))) lines.push([i]);
        else lines[lines.length - 1].push(i);
      }
      // A line of one photo stretched across a wide canvas is rarely pretty.
      if (lines.length > 4) continue;
      const natural = lines.map((line) => {
        const usable = W - gap * (line.length - 1);
        return usable / line.reduce((sum, i) => sum + ratios[i], 0);
      });
      const naturalTotal = natural.reduce((a, b) => a + b, 0);
      const usableH = H - gap * (lines.length - 1);
      if (naturalTotal <= 0 || usableH <= 0) continue;
      const stretch = usableH / naturalTotal;
      // Uneven lines (5 in one, 1 in another) read as a mistake: small penalty.
      const sizes = lines.map((l) => l.length);
      const unevenness = (Math.max(...sizes) - Math.min(...sizes)) * 0.04;
      const cost = Math.abs(Math.log(stretch)) + unevenness;
      if (best && cost >= best.cost) continue;

      const rects: Rect[] = new Array(n);
      let y = 0;
      lines.forEach((line, li) => {
        const h = natural[li] * stretch;
        const usable = W - gap * (line.length - 1);
        const sum = line.reduce((s, i) => s + ratios[i], 0);
        let x = 0;
        for (const i of line) {
          const w = (usable * ratios[i]) / sum;
          rects[i] = transpose
            ? { x: area.x + y, y: area.y + x, w: h, h: w }
            : { x: area.x + x, y: area.y + y, w, h };
          x += w + gap;
        }
        y += h + gap;
      });
      best = { cost, rects };
    }
  }
  return best?.rects ?? [];
}

/** Starting crop for a photo: centred on its faces when it has any, so
 *  heads stay in frame, otherwise centred (a touch high on tall photos). */
export function defaultFocus(faces: FaceBox[] | undefined, aspect: number): Focus {
  if (faces && faces.length > 0) {
    const left = Math.min(...faces.map((f) => f.x));
    const top = Math.min(...faces.map((f) => f.y));
    const right = Math.max(...faces.map((f) => f.x + f.width));
    const bottom = Math.max(...faces.map((f) => f.y + f.height));
    return { x: clamp01((left + right) / 2), y: clamp01((top + bottom) / 2), zoom: 1 };
  }
  return { x: 0.5, y: aspect < 1 ? 0.4 : 0.5, zoom: 1 };
}

/** Source rectangle (image pixels) that covers a `cellW × cellH` cell. */
export function sourceRect(imgW: number, imgH: number, cellW: number, cellH: number, focus: Focus) {
  const scale = Math.max(cellW / imgW, cellH / imgH) * Math.max(1, focus.zoom);
  const sw = Math.min(imgW, cellW / scale);
  const sh = Math.min(imgH, cellH / scale);
  const sx = clamp(focus.x * imgW - sw / 2, 0, imgW - sw);
  const sy = clamp(focus.y * imgH - sh / 2, 0, imgH - sh);
  return { sx, sy, sw, sh };
}

/** Move a focus by a drag of `dx, dy` canvas pixels inside a cell, keeping
 *  it within the range that actually changes the crop. */
export function panFocus(focus: Focus, dx: number, dy: number, imgW: number, imgH: number, cellW: number, cellH: number): Focus {
  const { sw, sh } = sourceRect(imgW, imgH, cellW, cellH, focus);
  const halfW = sw / imgW / 2;
  const halfH = sh / imgH / 2;
  return {
    zoom: focus.zoom,
    x: clamp(focus.x - (dx * (sw / cellW)) / imgW, halfW, 1 - halfW),
    y: clamp(focus.y - (dy * (sh / cellH)) / imgH, halfH, 1 - halfH),
  };
}

export function canvasSize(shape: ShapeId, longSide: number): { width: number; height: number } {
  const ratio = SHAPES.find((s) => s.id === shape)?.ratio ?? 1;
  return ratio >= 1
    ? { width: longSide, height: Math.round(longSide / ratio) }
    : { width: Math.round(longSide * ratio), height: longSide };
}

export interface CollagePhoto {
  source: CanvasImageSource;
  width: number;
  height: number;
}

export interface RenderOptions {
  layout: Layout;
  photos: CollagePhoto[]; // in slot order
  focus: Focus[]; // in slot order
  /** Fractions of the canvas's short side. */
  gap: number;
  radius: number;
  background: string;
}

/** Draw the collage at the canvas's current pixel size. Returns the cell
 *  rectangles so callers can hit-test pointer events. */
export function renderCollage(ctx: CanvasRenderingContext2D, width: number, height: number, opts: RenderOptions): Rect[] {
  const short = Math.min(width, height);
  const gap = opts.gap * short;
  const rects = layoutRects(opts.layout, width, height, gap, opts.photos.map((p) => p.width / p.height));
  ctx.save();
  ctx.fillStyle = opts.background;
  ctx.fillRect(0, 0, width, height);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  rects.forEach((r, i) => {
    const photo = opts.photos[i];
    if (!photo) return;
    const radius = Math.min(opts.radius * short, r.w / 2, r.h / 2);
    ctx.save();
    ctx.beginPath();
    if (radius > 0 && typeof ctx.roundRect === "function") ctx.roundRect(r.x, r.y, r.w, r.h, radius);
    else ctx.rect(r.x, r.y, r.w, r.h);
    ctx.clip();
    const { sx, sy, sw, sh } = sourceRect(photo.width, photo.height, r.w, r.h, opts.focus[i] ?? { x: 0.5, y: 0.5, zoom: 1 });
    ctx.drawImage(photo.source, sx, sy, sw, sh, r.x, r.y, r.w, r.h);
    ctx.restore();
  });
  ctx.restore();
  return rects;
}

/** A soft border colour drawn from the photos themselves: their average
 *  colour, lifted most of the way toward white. */
export function matchColor(averages: [number, number, number][]): string {
  if (averages.length === 0) return "#f3eee4";
  const mean = [0, 1, 2].map((c) => averages.reduce((sum, a) => sum + a[c], 0) / averages.length);
  const lift = mean.map((v) => Math.round(v * 0.35 + 255 * 0.65));
  return `rgb(${lift[0]}, ${lift[1]}, ${lift[2]})`;
}

// ---- automatic arrangement ----------------------------------------------

/** Gap used when laying out automatically; matches the dialog's default. */
export const AUTO_GAP = 0.015;
/** Automatic framing zooms in on small faces, but never past this. */
export const MAX_AUTO_ZOOM = 1.6;
const AUTO_SHAPES: { id: ShapeId; penalty: number }[] = [
  { id: "square", penalty: 0 },
  { id: "portrait", penalty: 0 },
  { id: "landscape", penalty: 0.02 },
];
/** Mosaic crops least but looks least designed; prefer a real layout
 *  unless it costs noticeably more cropping. */
const MOSAIC_PENALTY = 0.12;
/** A large feature photo reads as a deliberate design. */
const FEATURE_BONUS = 0.03;

export interface ArrangePhoto {
  /** Width / height as displayed. */
  aspect: number;
  faces: FaceBox[];
}

export interface Arrangement {
  layoutId: string;
  shape: ShapeId;
  /** Slot → photo index. */
  order: number[];
}

/** The faces a photo is about. Faces far smaller than the largest are
 *  usually passers-by, and framing for them would crop the subject. */
export function subjectFaces(faces: FaceBox[] | undefined): FaceBox[] {
  if (!faces || faces.length <= 1) return faces ?? [];
  const largest = Math.max(...faces.map((f) => f.width * f.height));
  return faces.filter((f) => f.width * f.height >= largest * 0.2);
}

function faceBounds(faces: FaceBox[]) {
  if (faces.length === 0) return null;
  const left = Math.min(...faces.map((f) => f.x));
  const top = Math.min(...faces.map((f) => f.y));
  const right = Math.max(...faces.map((f) => f.x + f.width));
  const bottom = Math.max(...faces.map((f) => f.y + f.height));
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

/** The part of a photo visible in a cell, as fractions of its width and
 *  height, for a cover fit at `zoom`. */
function visibleSize(imgAspect: number, cellAspect: number, zoom: number) {
  return {
    w: Math.min(1, cellAspect / imgAspect) / zoom,
    h: Math.min(1, imgAspect / cellAspect) / zoom,
  };
}

/**
 * Framing for a photo in a cell: centred on the people in it, sitting a
 * little above the middle, and zoomed in when they're small, up to
 * `MAX_AUTO_ZOOM` and only while their heads and shoulders still fit.
 * Photos without faces get the plain default.
 */
export function autoFocus(faces: FaceBox[] | undefined, imgAspect: number, cellAspect: number): Focus {
  const subject = faceBounds(subjectFaces(faces));
  if (!subject || !(imgAspect > 0) || !(cellAspect > 0)) return defaultFocus(undefined, imgAspect);
  const base = visibleSize(imgAspect, cellAspect, 1);
  // Face boxes stop at the hairline and chin; leave room for heads and shoulders.
  const needW = Math.max(subject.width * 1.5, 1e-3);
  const needH = Math.max(subject.height * 2.2, 1e-3);
  // Faces filling about a quarter of the frame's height read well at collage size.
  const wanted = (0.26 * base.h) / Math.max(subject.height, 1e-3);
  const fits = Math.min(base.w / needW, base.h / needH);
  const zoom = Math.min(MAX_AUTO_ZOOM, Math.max(1, Math.min(wanted, fits)));
  const view = visibleSize(imgAspect, cellAspect, zoom);
  const cx = (subject.left + subject.right) / 2;
  // Put faces a touch above centre, but never push their tops out of frame.
  const cy = Math.min((subject.top + subject.bottom) / 2 + view.h * 0.08, subject.top + view.h / 2 - 0.01);
  return {
    x: clamp(cx, view.w / 2, 1 - view.w / 2),
    y: clamp(cy, view.h / 2, 1 - view.h / 2),
    zoom,
  };
}

/** How much of the subject's faces a framing cuts off, 0 (none) to 1. */
function faceCut(faces: FaceBox[] | undefined, imgAspect: number, cellAspect: number, focus: Focus): number {
  const subject = faceBounds(subjectFaces(faces));
  if (!subject) return 0;
  const view = visibleSize(imgAspect, cellAspect, focus.zoom);
  const x0 = clamp(focus.x - view.w / 2, 0, 1 - view.w);
  const y0 = clamp(focus.y - view.h / 2, 0, 1 - view.h);
  const overflow =
    Math.max(0, x0 - subject.left) + Math.max(0, subject.right - (x0 + view.w)) +
    Math.max(0, y0 - subject.top) + Math.max(0, subject.bottom - (y0 + view.h));
  return Math.min(1, (overflow / Math.max(subject.width + subject.height, 1e-3)) * 2);
}

/** Cost of showing a photo in a cell: the share of it cropped away, and
 *  heavily, any face cut off. */
function placementCost(photo: ArrangePhoto, cellAspect: number): number {
  const aspect = photo.aspect > 0 ? photo.aspect : 1;
  const crop = 1 - Math.min(aspect / cellAspect, cellAspect / aspect);
  const cut = faceCut(photo.faces, aspect, cellAspect, autoFocus(photo.faces, aspect, cellAspect));
  return crop + 1.5 * cut;
}

/** Whether a layout has a feature cell (always slot 0). */
export function hasFeature(layout: Layout): boolean {
  const walk = (node: Node): boolean => (node.kind === "leaf" ? node.feature === true : node.children.some((c) => walk(c.node)));
  return layout.tree != null && walk(layout.tree);
}

/**
 * Choose a shape, a layout and which photo goes in which cell so that the
 * photos are cropped as little as possible and faces stay whole. Photo 0
 * is treated as the best and takes the feature cell when there is one.
 */
export function autoArrange(photos: ArrangePhoto[]): Arrangement {
  const n = photos.length;
  const identity = photos.map((_, i) => i);
  // Ties keep the first found, so the result is stable.
  const best = { cost: Infinity, arrangement: { layoutId: layoutsFor(n)[0].id, shape: "square" as ShapeId, order: identity } };
  const consider = (cost: number, arrangement: Arrangement) => {
    if (cost < best.cost - 1e-9) Object.assign(best, { cost, arrangement });
  };
  if (n < MIN_PHOTOS) return best.arrangement;

  for (const shape of AUTO_SHAPES) {
    const { width, height } = canvasSize(shape.id, 1000);
    const gap = AUTO_GAP * Math.min(width, height);
    for (const layout of layoutsFor(n)) {
      if (!layout.tree) {
        const rects = layoutRects(layout, width, height, gap, photos.map((p) => p.aspect));
        const cost = weightedCost(rects, (slot, r) => placementCost(photos[slot], r.w / r.h));
        consider(cost + MOSAIC_PENALTY + shape.penalty, { layoutId: layout.id, shape: shape.id, order: identity });
        continue;
      }
      const rects = layoutRects(layout, width, height, gap, []);
      if (rects.length !== n) continue;
      const total = rects.reduce((sum, r) => sum + r.w * r.h, 0);
      // cost[photo][slot], weighted by how much of the collage the cell is.
      const costs = photos.map((p) => rects.map((r) => (placementCost(p, r.w / r.h) * r.w * r.h) / total));
      const feature = hasFeature(layout);
      const { cost, order } = assign(costs, feature ? 0 : -1);
      // Without a feature cell, the best photo should still get the biggest one.
      const area = (r: Rect) => r.w * r.h;
      const biggest = Math.max(...rects.map(area));
      const leadPenalty = !feature && area(rects[order.indexOf(0)]) < biggest - 1e-6 ? 0.05 : 0;
      const bonus = feature ? FEATURE_BONUS : 0;
      consider(cost + leadPenalty + shape.penalty - bonus, { layoutId: layout.id, shape: shape.id, order });
    }
  }
  return best.arrangement;
}

function weightedCost(rects: Rect[], cost: (slot: number, r: Rect) => number): number {
  const total = rects.reduce((sum, r) => sum + r.w * r.h, 0) || 1;
  return rects.reduce((sum, r, slot) => sum + (cost(slot, r) * r.w * r.h) / total, 0);
}

/**
 * Cheapest photo-to-slot assignment. `pinned` ≥ 0 fixes photo 0 in that
 * slot. Exhaustive for up to seven photos, greedy (largest cells first)
 * beyond that.
 */
function assign(costs: number[][], pinned: number): { cost: number; order: number[] } {
  const n = costs.length;
  const order = new Array<number>(n).fill(-1);
  if (n > 7) {
    const used = new Set<number>();
    let cost = 0;
    const slots = [...Array(n).keys()];
    if (pinned >= 0) { order[pinned] = 0; used.add(0); cost += costs[0][pinned]; }
    for (const slot of slots) {
      if (order[slot] >= 0) continue;
      let pick = -1;
      for (let p = 0; p < n; p++) if (!used.has(p) && (pick < 0 || costs[p][slot] < costs[pick][slot])) pick = p;
      order[slot] = pick;
      used.add(pick);
      cost += costs[pick][slot];
    }
    return { cost, order };
  }
  let best = { cost: Infinity, order: [...Array(n).keys()] };
  const used = new Array<boolean>(n).fill(false);
  const walk = (slot: number, sum: number) => {
    if (sum >= best.cost) return;
    if (slot === n) {
      best = { cost: sum, order: [...order] };
      return;
    }
    if (pinned === slot) {
      order[slot] = 0;
      walk(slot + 1, sum + costs[0][slot]);
      return;
    }
    for (let p = 0; p < n; p++) {
      if (used[p] || (pinned >= 0 && p === 0)) continue;
      used[p] = true;
      order[slot] = p;
      walk(slot + 1, sum + costs[p][slot]);
      used[p] = false;
    }
  };
  walk(0, 0);
  return best;
}

/** Automatic framing for every slot of a layout, given the photos in slot
 *  order. Cell shapes are taken at a nominal size; the gap barely moves them. */
export function autoFocusAll(layout: Layout, shape: ShapeId, gap: number, photos: ArrangePhoto[]): Focus[] {
  const { width, height } = canvasSize(shape, 1000);
  const rects = layoutRects(layout, width, height, gap * Math.min(width, height), photos.map((p) => p.aspect));
  return photos.map((p, slot) => {
    const r = rects[slot];
    return r ? autoFocus(p.faces, p.aspect, r.w / r.h) : defaultFocus(p.faces, p.aspect);
  });
}

export function hitTest(rects: Rect[], x: number, y: number): number {
  return rects.findIndex((r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h);
}

/** Layout trees must have exactly one leaf per photo. Exposed for tests. */
export function slotCount(layout: Layout, count: number): number {
  return layout.tree ? leafCount(layout.tree) : count;
}

function clamp(v: number, lo: number, hi: number) {
  return hi < lo ? (lo + hi) / 2 : Math.min(hi, Math.max(lo, v));
}
function clamp01(v: number) {
  return clamp(v, 0, 1);
}
