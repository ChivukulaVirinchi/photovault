import { describe, expect, it } from "vitest";
import {
  MAX_PHOTOS,
  MAX_AUTO_ZOOM,
  MIN_PHOTOS,
  autoArrange,
  autoFocus,
  autoFocusAll,
  canvasSize,
  defaultFocus,
  hitTest,
  layoutRects,
  layoutsFor,
  matchColor,
  panFocus,
  slotCount,
  sourceRect,
  subjectFaces,
  type ArrangePhoto,
  type Rect,
} from "./collage";

const overlaps = (a: Rect, b: Rect) =>
  a.x < b.x + b.w - 1e-6 && b.x < a.x + a.w - 1e-6 && a.y < b.y + b.h - 1e-6 && b.y < a.y + a.h - 1e-6;

describe("collage layouts", () => {
  for (let n = MIN_PHOTOS; n <= MAX_PHOTOS; n++) {
    it(`every layout for ${n} photos has one cell per photo, inside the canvas, without overlaps`, () => {
      const aspects = Array.from({ length: n }, (_, i) => (i % 2 ? 0.75 : 1.5));
      for (const layout of layoutsFor(n)) {
        expect(slotCount(layout, n), layout.id).toBe(n);
        const rects = layoutRects(layout, 1000, 1250, 10, aspects);
        expect(rects, layout.id).toHaveLength(n);
        for (const r of rects) {
          expect(r.w).toBeGreaterThan(0);
          expect(r.x).toBeGreaterThanOrEqual(10 - 1e-6);
          expect(r.y).toBeGreaterThanOrEqual(10 - 1e-6);
          expect(r.x + r.w).toBeLessThanOrEqual(990 + 1e-6);
          expect(r.y + r.h).toBeLessThanOrEqual(1240 + 1e-6);
        }
        for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
          expect(overlaps(rects[i], rects[j]), `${layout.id} ${i}/${j}`).toBe(false);
        }
      }
    });
  }

  it("ends every list with Mosaic", () => {
    expect(layoutsFor(4).at(-1)?.id).toBe("mosaic");
  });

  it("gives the feature cell to the first photo", () => {
    const layout = layoutsFor(5).find((l) => l.id === "5-centre")!;
    const rects = layoutRects(layout, 1000, 1000, 0, [1, 1, 1, 1, 1]);
    const areas = rects.map((r) => r.w * r.h);
    expect(Math.max(...areas)).toBe(areas[0]);
  });

  it("mosaic keeps photo shapes when the canvas allows it", () => {
    // Two 3:2 photos stacked fill a 3:4 canvas exactly.
    const rects = layoutRects(layoutsFor(2).at(-1)!, 300, 400, 0, [1.5, 1.5]);
    for (const r of rects) expect(r.w / r.h).toBeCloseTo(1.5, 5);
  });
});

describe("cropping", () => {
  it("centres on the faces", () => {
    const focus = defaultFocus([{ x: 0.7, y: 0.1, width: 0.1, height: 0.1 }], 1.5);
    expect(focus.x).toBeCloseTo(0.75);
    expect(focus.y).toBeCloseTo(0.15);
  });

  it("never reads outside the image", () => {
    const r = sourceRect(4000, 3000, 100, 400, { x: 0.99, y: 0.99, zoom: 1 });
    expect(r.sx + r.sw).toBeCloseTo(4000);
    expect(r.sy + r.sh).toBeLessThanOrEqual(3000);
    expect(r.sw / r.sh).toBeCloseTo(100 / 400);
  });

  it("dragging right reveals more of the left of the photo, within bounds", () => {
    const start = { x: 0.5, y: 0.5, zoom: 1 };
    const moved = panFocus(start, 40, 0, 4000, 1000, 100, 100);
    expect(moved.x).toBeLessThan(0.5);
    const far = panFocus(start, 100_000, 0, 4000, 1000, 100, 100);
    expect(far.x).toBeCloseTo(0.125); // half of the visible width (1000/4000/2)
  });
});

describe("helpers", () => {
  it("sizes the canvas from the long side", () => {
    expect(canvasSize("story", 3000)).toEqual({ width: 1688, height: 3000 });
    expect(canvasSize("landscape", 3000)).toEqual({ width: 3000, height: 2000 });
  });

  it("hit-tests cells", () => {
    const rects = [{ x: 0, y: 0, w: 10, h: 10 }, { x: 20, y: 0, w: 10, h: 10 }];
    expect(hitTest(rects, 25, 5)).toBe(1);
    expect(hitTest(rects, 15, 5)).toBe(-1);
  });

  it("lifts the matched colour toward white", () => {
    expect(matchColor([[0, 0, 0]])).toBe("rgb(166, 166, 166)");
  });
});

describe("automatic arrangement", () => {
  const land: ArrangePhoto = { aspect: 3 / 2, faces: [] };
  const tall: ArrangePhoto = { aspect: 2 / 3, faces: [] };

  it("puts portraits side by side and landscapes on top of each other", () => {
    expect(autoArrange([tall, tall]).layoutId).toBe("2-side");
    const stacked = autoArrange([land, land]);
    expect(["2-stack", "mosaic"]).toContain(stacked.layoutId);
  });

  it("gives the best photo the feature cell and every photo one slot", () => {
    for (let n = 2; n <= 9; n++) {
      const photos = Array.from({ length: n }, (_, i) => (i % 2 ? tall : land));
      const { layoutId, order } = autoArrange(photos);
      expect([...order].sort((a, b) => a - b)).toEqual([...Array(n).keys()]);
      const layout = layoutsFor(n).find((l) => l.id === layoutId)!;
      if (layout.tree && layout.name.startsWith("Feature")) expect(order[0]).toBe(0);
    }
  });

  it("moves a portrait into the tall cell", () => {
    // Landscape best photo plus one portrait and one landscape.
    const { layoutId, order } = autoArrange([land, tall, land]);
    if (layoutId === "3-left") {
      // Slots 1 and 2 are the stacked cells on the right; neither is tall,
      // so this layout should not have been chosen for a portrait.
      throw new Error("chose a layout that crops the portrait hard");
    }
    expect(order).toHaveLength(3);
  });

  it("is stable for the same input", () => {
    const photos = [land, tall, land, land, tall];
    expect(autoArrange(photos)).toEqual(autoArrange(photos));
  });
});

describe("automatic framing", () => {
  it("ignores small faces in the background", () => {
    const faces = [
      { x: 0.4, y: 0.3, width: 0.2, height: 0.25 },
      { x: 0.9, y: 0.1, width: 0.03, height: 0.04 },
    ];
    expect(subjectFaces(faces)).toHaveLength(1);
  });

  it("zooms in on a small face, within the limit, keeping it in frame", () => {
    const face = { x: 0.48, y: 0.3, width: 0.04, height: 0.05 };
    const focus = autoFocus([face], 3 / 2, 1);
    expect(focus.zoom).toBeGreaterThan(1);
    expect(focus.zoom).toBeLessThanOrEqual(MAX_AUTO_ZOOM);
    const { sx, sy, sw, sh } = sourceRect(1500, 1000, 100, 100, focus);
    expect(sx).toBeLessThanOrEqual(face.x * 1500);
    expect(sx + sw).toBeGreaterThanOrEqual((face.x + face.width) * 1500);
    expect(sy).toBeLessThanOrEqual(face.y * 1000);
    expect(sy + sh).toBeGreaterThanOrEqual((face.y + face.height) * 1000);
  });

  it("leaves a large face unzoomed and photos without faces at the default", () => {
    expect(autoFocus([{ x: 0.3, y: 0.2, width: 0.4, height: 0.5 }], 1, 1).zoom).toBe(1);
    expect(autoFocus([], 3 / 2, 1)).toEqual(defaultFocus(undefined, 3 / 2));
  });

  it("keeps faces near the edge of a wide photo in frame", () => {
    const face = { x: 0.05, y: 0.4, width: 0.1, height: 0.15 };
    const focus = autoFocus([face], 2, 0.8);
    const { sx, sw } = sourceRect(2000, 1000, 80, 100, focus);
    expect(sx).toBeLessThanOrEqual(face.x * 2000);
    expect(sx + sw).toBeGreaterThanOrEqual((face.x + face.width) * 2000);
  });

  it("frames every slot of a layout", () => {
    const layout = layoutsFor(3)[0];
    const focus = autoFocusAll(layout, "square", 0.015, [
      { aspect: 1.5, faces: [] },
      { aspect: 1, faces: [] },
      { aspect: 0.75, faces: [] },
    ]);
    expect(focus).toHaveLength(3);
    expect(focus.every((f) => f.zoom === 1)).toBe(true);
  });
});
