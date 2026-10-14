import { describe, expect, it } from "vitest";
import { centerX, displayAspect, indexAtX, justifyRows } from "./justifiedLayout";

const photo = (width: number | null, height: number | null, orientation = 1) => ({ width, height, orientation });

describe("displayAspect", () => {
  it("uses width / height and swaps for rotated orientations", () => {
    expect(displayAspect(photo(4000, 3000))).toBeCloseTo(4 / 3);
    expect(displayAspect(photo(4000, 3000, 6))).toBeCloseTo(3 / 4);
  });
  it("renders unknown sizes square and clamps extremes", () => {
    expect(displayAspect(photo(null, null))).toBe(1);
    expect(displayAspect(photo(10000, 1000))).toBe(2.5);
    expect(displayAspect(photo(100, 1000))).toBe(0.5);
  });
});

describe("justifyRows", () => {
  it("fills full rows exactly to the container width", () => {
    const items = Array.from({ length: 10 }, (_, i) => photo(i % 2 ? 3000 : 4000, i % 2 ? 4000 : 3000));
    const rows = justifyRows(items, 1000, 200, 4);
    const full = rows.slice(0, -1);
    expect(full.length).toBeGreaterThan(0);
    for (const row of full) {
      const total = row.widths.reduce((s, w) => s + w, 0) + 4 * (row.widths.length - 1);
      expect(total).toBe(1000);
      expect(row.height).toBeGreaterThan(100);
      expect(row.height).toBeLessThanOrEqual(200);
    }
    expect(rows.flatMap((r) => r.items)).toHaveLength(10);
  });

  it("leaves the last row at the target height without stretching", () => {
    const rows = justifyRows([photo(4000, 3000)], 1000, 200, 4);
    expect(rows).toHaveLength(1);
    expect(rows[0].height).toBe(200);
    expect(rows[0].widths[0]).toBe(Math.round((4 / 3) * 200));
  });

  it("returns nothing for empty input or no width", () => {
    expect(justifyRows([], 1000, 200, 4)).toEqual([]);
    expect(justifyRows([photo(1, 1)], 0, 200, 4)).toEqual([]);
  });
});

describe("keyboard geometry", () => {
  it("finds the item under a horizontal position", () => {
    const widths = [100, 200, 100];
    expect(indexAtX(widths, 4, 50)).toBe(0);
    expect(indexAtX(widths, 4, 150)).toBe(1);
    expect(indexAtX(widths, 4, 1000)).toBe(2);
    expect(centerX(widths, 4, 1)).toBe(100 + 4 + 100);
  });
});
