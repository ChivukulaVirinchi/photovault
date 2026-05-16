import { beforeEach, describe, expect, it } from "vitest";

import { browseContext } from "./browseContext.svelte";

beforeEach(() => {
  browseContext.clear();
});

describe("browseContext", () => {
  it("returns the next id after the current one", () => {
    browseContext.set("timeline", [10, 20, 30]);
    expect(browseContext.next(10)).toBe(20);
    expect(browseContext.next(20)).toBe(30);
  });

  it("returns the previous id before the current one", () => {
    browseContext.set("timeline", [10, 20, 30]);
    expect(browseContext.prev(30)).toBe(20);
    expect(browseContext.prev(20)).toBe(10);
  });

  it("returns null when there is no neighbour", () => {
    browseContext.set("timeline", [10, 20, 30]);
    expect(browseContext.prev(10)).toBeNull();
    expect(browseContext.next(30)).toBeNull();
  });

  it("returns null for ids it doesn't know about", () => {
    browseContext.set("timeline", [10, 20, 30]);
    expect(browseContext.prev(99)).toBeNull();
    expect(browseContext.next(99)).toBeNull();
  });

  it("extend dedupes ids already in the list", () => {
    browseContext.set("timeline", [10, 20, 30]);
    browseContext.extend([20, 30, 40, 50]);
    expect(browseContext.ids).toEqual([10, 20, 30, 40, 50]);
  });

  it("position reports 1-indexed location + total", () => {
    browseContext.set("timeline", [10, 20, 30]);
    expect(browseContext.position(20)).toEqual({ index: 2, total: 3 });
  });

  it("position returns null for unknown ids", () => {
    browseContext.set("timeline", [10, 20, 30]);
    expect(browseContext.position(99)).toBeNull();
  });

  it("clear drops every id and forgets the source", () => {
    browseContext.set("timeline", [10, 20, 30]);
    browseContext.clear();
    expect(browseContext.ids).toEqual([]);
    expect(browseContext.source).toBeNull();
  });
});
