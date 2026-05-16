import { describe, expect, it } from "vitest";

import {
  moveIndex,
  resolveStartIndex,
  shouldLoadMore,
  uniquePhotoIds,
} from "./slideshowQueue";

// Pure-logic tests for the slideshow queue helpers. These power
// every slideshow next/prev/auto-advance call across the UI, so
// catching a regression here pays for the whole test runner on
// every PR.
describe("uniquePhotoIds", () => {
  it("dedupes repeated ids and filters NaN", () => {
    expect(uniquePhotoIds([4, 4, 7, Number.NaN, 9, 7])).toEqual([4, 7, 9]);
  });

  it("preserves order of first occurrence", () => {
    expect(uniquePhotoIds([3, 1, 2, 1, 3])).toEqual([3, 1, 2]);
  });
});

describe("resolveStartIndex", () => {
  it("starts at the requested id", () => {
    expect(resolveStartIndex([10, 20, 30], 20)).toBe(1);
  });

  it("falls back to first id when requested id is missing", () => {
    expect(resolveStartIndex([10, 20, 30], 99)).toBe(0);
  });

  it("returns -1 for an empty queue", () => {
    expect(resolveStartIndex([], 20)).toBe(-1);
  });
});

describe("moveIndex", () => {
  it("advances forward in the middle of the queue", () => {
    expect(moveIndex(0, 3, "next", true)).toBe(1);
  });

  it("loops forward when looping is on", () => {
    expect(moveIndex(2, 3, "next", true)).toBe(0);
  });

  it("stops at end when looping is off", () => {
    expect(moveIndex(2, 3, "next", false)).toBe(2);
  });

  it("loops backward when looping is on", () => {
    expect(moveIndex(0, 3, "prev", true)).toBe(2);
  });

  it("stops at start when looping is off", () => {
    expect(moveIndex(0, 3, "prev", false)).toBe(0);
  });

  it("clamps an out-of-range index before moving", () => {
    expect(moveIndex(10, 3, "prev", false)).toBe(1);
  });
});

describe("shouldLoadMore", () => {
  it("does not prefetch when far from the end", () => {
    expect(shouldLoadMore(91, 100)).toBe(false);
  });

  it("prefetches when within the threshold of the end", () => {
    expect(shouldLoadMore(92, 100)).toBe(true);
  });

  it("does not prefetch from an empty queue", () => {
    expect(shouldLoadMore(-1, 0)).toBe(false);
  });
});
