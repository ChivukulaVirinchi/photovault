// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { renderShareCard, type CardTheme } from "./shareCard";
import type { InsightsData } from "./api/all";

describe("share card month hit regions", () => {
  it("returns the exact painted cells with full month, year and count labels", () => {
    const painted: number[][] = [];
    const context = new Proxy({
      measureText: (text: string) => ({ width: text.length * 8 }),
      createLinearGradient: () => ({ addColorStop() {} }),
      createRadialGradient: () => ({ addColorStop() {} }),
      roundRect: (...args: number[]) => painted.push(args),
    }, { get(target, prop) { return Reflect.get(target, prop) ?? (() => {}); } });
    const canvas = document.createElement("canvas");
    vi.spyOn(canvas, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
    const theme: CardTheme = { paper: "#111", ink: "#eee", inkSoft: "#ddd", inkMuted: "#aaa", inkFaint: "#777", line: "#333", accent: "#fff", heat: ["#111", "#222", "#333", "#444", "#555"] };
    const data = { total_photos: 123, available_years: [2024, 2023], months_by_year: { "2024-06": 123 }, people_count: 0, album_count: 0, city_count: 0, country_count: 0, heatmap_year: 2024 } as unknown as InsightsData;
    const cells = renderShareCard(canvas, data, theme);
    expect(cells).toHaveLength(24);
    expect(cells[0].label).toBe("January 2023 — 0 photos");
    const june = cells.find(cell => cell.label === "June 2024 — 123 photos")!;
    expect(june).toBeDefined();
    expect(painted.some(([x, y, width, height]) => x === june.x && y === june.y && width === june.width && height === june.height)).toBe(true);
    expect(canvas.width).toBe(2160);
    expect(cells.every(cell => cell.x + cell.width <= 1080 && cell.y + cell.height <= 1350)).toBe(true);
  });
});
