import { beforeEach, expect, it, vi } from "vitest";
import { call } from "../api";
import { SlideshowStore } from "./slideshow.svelte";
import type { Page, PhotoSummaryDto } from "../api/types";
import { recentMemories, rememberPhoto } from "../surpriseHistory";

vi.mock("../api", () => ({ call: vi.fn() }));
const photo = (id: number) => ({ id } as PhotoSummaryDto);
beforeEach(() => {
  vi.mocked(call).mockReset();
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  });
});

it("opens a slowshow in the selected album and records only displayed photos", async () => {
  rememberPhoto("library", 9);
  vi.mocked(call).mockResolvedValue([photo(1), photo(2)]);
  const store = new SlideshowStore();
  await store.surprise("library", 42, "Trip", () => true);
  expect(call).toHaveBeenCalledWith("memories_surprise", { album_id: 42, exclude_ids: [9] });
  expect(store.intervalMs).toBe(12000);
  expect(store.playing).toBe(true);
  expect(recentMemories("library")).toEqual([9]);
  store.presented(1);
  expect(recentMemories("library")).toEqual([9, 1]);
  store.setInterval(8000);
  store.start({ kind: "album", label: "Album", ids: [1, 2] });
  expect(store.intervalMs).toBe(5000);
});

it("does not reopen after close or a library/album change during selection", async () => {
  let resolve!: (photos: PhotoSummaryDto[]) => void;
  vi.mocked(call).mockImplementation(() => new Promise((done) => { resolve = done; }));
  const store = new SlideshowStore();
  const pending = store.surprise("old-library", null, "Old", () => true);
  store.close();
  resolve([photo(1), photo(2)]);
  await pending;
  expect(store.active).toBe(false);
  expect(store.starting).toBe(false);
  vi.mocked(call).mockResolvedValue([photo(3)]);
  expect(await store.surprise("old", null, "Old", () => false)).toBeNull();
  expect(store.active).toBe(false);
});

it("shares in-flight refill and ignores it when a different show starts", async () => {
  let resolve!: (page: Page<PhotoSummaryDto>) => void;
  const loadMore = vi.fn(() => new Promise<Page<PhotoSummaryDto>>((done) => { resolve = done; }));
  const store = new SlideshowStore();
  store.start({ kind: "surprise", label: "", ids: [1, 2], hasMore: true, loadMore });
  const first = store.loadMoreNow();
  const second = store.loadMoreNow();
  expect(loadMore).toHaveBeenCalledTimes(1);
  store.start({ kind: "album", label: "New", ids: [50, 51] });
  resolve({ items: [photo(3)], total: null, next_cursor: null, has_more: true });
  await Promise.all([first, second]);
  expect(store.ids).toEqual([50, 51]);
  expect(store.index).toBe(0);
});

it("keeps a bounded back-history while continuously refilling", async () => {
  let id = 36;
  const store = new SlideshowStore();
  store.start({
    kind: "surprise", label: "", ids: Array.from({ length: 36 }, (_, i) => i + 1),
    hasMore: true,
    loadMore: async () => ({ items: Array.from({ length: 36 }, () => photo(++id)),
      total: null, next_cursor: null, has_more: true }),
  });
  for (let i = 0; i < 500; i++) await store.next();
  expect(store.currentId()).toBe(501);
  expect(store.ids.length).toBeLessThan(260);
  store.prev();
  expect(store.currentId()).toBe(500);
});

it("allows small albums to revisit earlier photos without growing forever", async () => {
  vi.mocked(call).mockResolvedValue([photo(1), photo(2)]);
  const store = new SlideshowStore();
  await store.surprise("library", 1, "Small", () => true);
  for (let i = 0; i < 20; i++) await store.next();
  expect(store.playing).toBe(true);
  expect(store.currentId()).not.toBeNull();
});

it("clears the pending state on an empty library or a failed request", async () => {
  const store = new SlideshowStore();
  vi.mocked(call).mockResolvedValue([]);
  expect(await store.surprise("empty", null, "", () => true)).toBe(0);
  expect(store.active).toBe(false);
  expect(store.starting).toBe(false);
  vi.mocked(call).mockRejectedValue(new Error("offline drive"));
  await expect(store.surprise("empty", null, "", () => true)).rejects.toThrow("offline drive");
  expect(store.starting).toBe(false);
});
