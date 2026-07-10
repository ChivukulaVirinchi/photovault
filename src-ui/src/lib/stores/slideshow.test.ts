import { beforeEach, describe, expect, it, vi } from "vitest";

// The store imports types from `../api/types`; nothing runs at
// module load that needs IPC. The async `loadMoreNow` path only
// runs when we provide a loader.
import { slideshow } from "./slideshow.svelte";

beforeEach(() => {
  slideshow.close();
  // `close()` deliberately leaves `loop` and `intervalMs` alone —
  // the user might re-open the slideshow and expect their last
  // preference. For tests we want a clean slate, so explicitly
  // restore the defaults.
  if (!slideshow.loop) slideshow.toggleLoop();
  slideshow.setInterval(5_000);
});

describe("slideshow store — start / close", () => {
  it("becomes active and plays when started with multiple photos", () => {
    slideshow.start({ kind: "timeline", label: "Timeline", ids: [1, 2, 3] });
    expect(slideshow.active).toBe(true);
    expect(slideshow.ids).toEqual([1, 2, 3]);
    expect(slideshow.playing).toBe(true);
    expect(slideshow.currentId()).toBe(1);
  });

  it("starts at the requested id when one is provided", () => {
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [10, 20, 30],
      startId: 20,
    });
    expect(slideshow.currentId()).toBe(20);
  });

  it("falls back to the first id when startId is missing from the queue", () => {
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [10, 20, 30],
      startId: 99,
    });
    expect(slideshow.currentId()).toBe(10);
  });

  it("does not play when started with a single id and no loader", () => {
    slideshow.start({ kind: "photo", label: "Viewer", ids: [7] });
    expect(slideshow.active).toBe(true);
    expect(slideshow.playing).toBe(false);
  });

  it("close resets every visible field", () => {
    slideshow.start({ kind: "timeline", label: "Timeline", ids: [1, 2, 3] });
    slideshow.close();
    expect(slideshow.active).toBe(false);
    expect(slideshow.ids).toEqual([]);
    expect(slideshow.playing).toBe(false);
    expect(slideshow.currentId()).toBeNull();
  });
});

describe("slideshow store — navigation", () => {
  it("next advances forward and loops back when loop is on", async () => {
    slideshow.start({ kind: "timeline", label: "Timeline", ids: [1, 2, 3] });
    expect(slideshow.currentId()).toBe(1);
    await slideshow.next();
    expect(slideshow.currentId()).toBe(2);
    await slideshow.next();
    expect(slideshow.currentId()).toBe(3);
    await slideshow.next(); // wraps back to 1 because loop defaults to true
    expect(slideshow.currentId()).toBe(1);
  });

  it("prev moves backwards", () => {
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [1, 2, 3],
      startId: 3,
    });
    slideshow.prev();
    expect(slideshow.currentId()).toBe(2);
    slideshow.prev();
    expect(slideshow.currentId()).toBe(1);
  });

  it("toggleLoop flips the loop flag", () => {
    slideshow.start({ kind: "timeline", label: "Timeline", ids: [1, 2, 3] });
    expect(slideshow.loop).toBe(true);
    slideshow.toggleLoop();
    expect(slideshow.loop).toBe(false);
  });

  it("with loop off, next at the end pauses playback", async () => {
    slideshow.start({ kind: "timeline", label: "Timeline", ids: [1, 2] });
    slideshow.toggleLoop(); // loop = false
    await slideshow.next(); // 1 -> 2
    await slideshow.next(); // 2 -> stays at 2, playing flips off
    expect(slideshow.currentId()).toBe(2);
    expect(slideshow.playing).toBe(false);
  });
});

describe("slideshow store — interval clamping", () => {
  it("setInterval enforces the 1.5 s – 15 s bounds", () => {
    slideshow.setInterval(100); // way below floor
    expect(slideshow.intervalMs).toBe(1500);
    slideshow.setInterval(99_999); // way above ceiling
    expect(slideshow.intervalMs).toBe(15_000);
    slideshow.setInterval(5_000);
    expect(slideshow.intervalMs).toBe(5_000);
  });

  it("ignores non-finite interval input", () => {
    slideshow.setInterval(4_000);
    slideshow.setInterval(Number.NaN);
    expect(slideshow.intervalMs).toBe(4_000);
  });
});

describe("slideshow store — paginated loadMore", () => {
  it("calls the loader when nearing the end and appends fresh ids", async () => {
    const loadMore = vi.fn().mockResolvedValueOnce({
      items: [{ id: 4 } as never, { id: 5 } as never],
      next_cursor: "c2",
      has_more: false,
      total: 5,
    });
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [1, 2, 3],
      nextCursor: "c1",
      hasMore: true,
      loadMore,
    });

    // We're already inside the prefetch threshold (3 ids loaded, threshold 8).
    await slideshow.ensureMoreAhead();
    expect(loadMore).toHaveBeenCalledOnce();
    expect(slideshow.ids).toEqual([1, 2, 3, 4, 5]);
    expect(slideshow.hasMore).toBe(false);
  });

  it("dedupes ids returned by loader against the existing queue", async () => {
    const loadMore = vi.fn().mockResolvedValueOnce({
      items: [{ id: 2 } as never, { id: 3 } as never, { id: 4 } as never],
      next_cursor: null,
      has_more: false,
      total: 4,
    });
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [1, 2, 3],
      hasMore: true,
      loadMore,
    });

    await slideshow.ensureMoreAhead();
    expect(slideshow.ids).toEqual([1, 2, 3, 4]);
  });

  it("does not wrap from the loaded end when pagination returns no fresh ids", async () => {
    const loadMore = vi.fn().mockResolvedValueOnce({
      items: [{ id: 2 } as never],
      next_cursor: "still-more",
      has_more: true,
      total: 3,
    });
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [1, 2],
      startId: 2,
      hasMore: true,
      nextCursor: "c1",
      loadMore,
    });

    await slideshow.next();

    expect(loadMore).toHaveBeenCalledOnce();
    expect(slideshow.currentId()).toBe(2);
    expect(slideshow.playing).toBe(false);
    expect(slideshow.hasMore).toBe(true);
  });

  it("skips loadMore when there's no loader configured", async () => {
    slideshow.start({ kind: "timeline", label: "Timeline", ids: [1, 2, 3] });
    await slideshow.ensureMoreAhead();
    // No throw, no append.
    expect(slideshow.ids).toEqual([1, 2, 3]);
  });

  it("ignores loadMore results after the slideshow is closed", async () => {
    type PageShape = {
      items: Array<{ id: number }>;
      next_cursor: string | null;
      has_more: boolean;
      total: number;
    };
    let resolve!: (value: PageShape) => void;
    const loadMore = vi.fn(
      () =>
        new Promise<PageShape>((r) => {
          resolve = r;
        }),
    );
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [1, 2, 3],
      hasMore: true,
      loadMore: loadMore as never,
    });

    const pending = slideshow.ensureMoreAhead();
    expect(slideshow.loadingMore).toBe(true);
    slideshow.close();
    expect(slideshow.loadingMore).toBe(false);
    resolve({ items: [{ id: 4 }], next_cursor: null, has_more: false, total: 4 });
    await pending;

    expect(slideshow.active).toBe(false);
    expect(slideshow.ids).toEqual([]);
    expect(slideshow.loadingMore).toBe(false);
  });

  it("resets loadingMore when a new slideshow starts after an in-flight load", async () => {
    type PageShape = {
      items: Array<{ id: number }>;
      next_cursor: string | null;
      has_more: boolean;
      total: number;
    };
    let resolve!: (value: PageShape) => void;
    const loadMore = vi.fn(
      () =>
        new Promise<PageShape>((r) => {
          resolve = r;
        }),
    );
    slideshow.start({
      kind: "timeline",
      label: "Timeline",
      ids: [1, 2, 3],
      hasMore: true,
      loadMore: loadMore as never,
    });

    const pending = slideshow.ensureMoreAhead();
    expect(slideshow.loadingMore).toBe(true);

    slideshow.start({ kind: "photo", label: "Single", ids: [9] });
    expect(slideshow.currentId()).toBe(9);
    expect(slideshow.loadingMore).toBe(false);

    resolve({ items: [{ id: 4 }], next_cursor: null, has_more: false, total: 4 });
    await pending;

    expect(slideshow.currentId()).toBe(9);
    expect(slideshow.ids).toEqual([9]);
    expect(slideshow.loadingMore).toBe(false);
  });
});
