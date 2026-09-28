import { describe, expect, it } from "vitest";
import { nextSearchPage, searchWindow, SEARCH_MAX_RESULTS } from "./searchWindow";

describe("Search result windows", () => {
  it("makes results past 1000 reachable without mounting the whole list", () => {
    expect(searchWindow(1200, 1200)).toEqual({ start: 1000, end: 1200 });
    expect(searchWindow(1200, 1000)).toEqual({ start: 0, end: 1000 });
    expect(searchWindow(1200, 2000)).toEqual({ start: 1000, end: 1200 });
  });
  it("covers every loaded ID exactly once across windows", () => {
    const seen: number[] = [];
    for (let end = 1000; end < 4000; end += 1000) {
      const w = searchWindow(2401, end);
      for (let id = w.start; id < w.end; id++) seen.push(id);
    }
    expect(seen).toEqual(Array.from({ length: 2401 }, (_, i) => i));
    expect(searchWindow(0, 200)).toEqual({ start: 0, end: 0 });
  });
  it("never requests beyond the accepted 20,000-result ceiling", () => {
    expect(nextSearchPage(0, 200)).toEqual({ offset: 0, limit: 200 });
    expect(nextSearchPage(19_950, 200)).toEqual({ offset: 19_950, limit: 50 });
    expect(nextSearchPage(SEARCH_MAX_RESULTS, 200)).toBeNull();
    expect(nextSearchPage(30_000, 200)).toBeNull();
  });
});
