import type { Page, PhotoSummaryDto } from "../api/types";
import {
  moveIndex,
  resolveStartIndex,
  shouldLoadMore,
  uniquePhotoIds,
} from "../slideshowQueue";

export type SlideshowKind = "timeline" | "album" | "memory" | "photo";

export interface SlideshowStart {
  kind: SlideshowKind;
  label: string;
  ids: number[];
  startId?: number | null;
  nextCursor?: string | null;
  hasMore?: boolean;
  loadMore?: (cursor: string | null) => Promise<Page<PhotoSummaryDto>>;
}

class SlideshowStore {
  active = $state(false);
  ids = $state<number[]>([]);
  index = $state(-1);
  kind = $state<SlideshowKind>("timeline");
  label = $state("");
  playing = $state(false);
  intervalMs = $state(5000);
  loop = $state(true);
  hasMore = $state(false);
  nextCursor = $state<string | null>(null);
  loadingMore = $state(false);
  private loader: ((cursor: string | null) => Promise<Page<PhotoSummaryDto>>) | null = null;

  start(opts: SlideshowStart) {
    const ids = uniquePhotoIds(opts.ids);
    this.ids = ids;
    this.index = resolveStartIndex(ids, opts.startId);
    this.kind = opts.kind;
    this.label = opts.label;
    this.nextCursor = opts.nextCursor ?? null;
    this.hasMore = Boolean(opts.hasMore && opts.loadMore);
    this.loader = opts.loadMore ?? null;
    this.playing = ids.length > 1 || this.hasMore;
    this.active = ids.length > 0;
  }

  close() {
    this.active = false;
    this.playing = false;
    this.ids = [];
    this.index = -1;
    this.nextCursor = null;
    this.hasMore = false;
    this.loader = null;
  }

  currentId(): number | null {
    if (!this.active || this.index < 0) return null;
    return this.ids[this.index] ?? null;
  }

  position(): { index: number; total: number } | null {
    if (!this.active || this.index < 0) return null;
    return { index: this.index + 1, total: this.ids.length };
  }

  setPlaying(value: boolean) {
    this.playing = value && this.active && (this.ids.length > 1 || this.hasMore);
  }

  togglePlaying() {
    this.setPlaying(!this.playing);
  }

  setInterval(ms: number) {
    this.intervalMs = Math.max(1500, Math.min(15000, ms));
  }

  toggleLoop() {
    this.loop = !this.loop;
  }

  async next() {
    if (!this.active) return;
    if (this.index >= this.ids.length - 1 && this.hasMore) {
      await this.loadMoreNow();
    } else {
      void this.ensureMoreAhead();
    }
    const next = moveIndex(this.index, this.ids.length, "next", this.loop);
    if (!this.loop && next === this.index && !this.hasMore) {
      this.playing = false;
    }
    this.index = next;
  }

  prev() {
    if (!this.active) return;
    this.index = moveIndex(this.index, this.ids.length, "prev", this.loop);
  }

  async ensureMoreAhead() {
    if (!shouldLoadMore(this.index, this.ids.length) || !this.hasMore) return;
    await this.loadMoreNow();
  }

  async loadMoreNow() {
    if (!this.loader || this.loadingMore || !this.hasMore) return;
    this.loadingMore = true;
    try {
      const page = await this.loader(this.nextCursor);
      const existing = new Set(this.ids);
      const fresh = page.items.map((p) => p.id).filter((id) => !existing.has(id));
      if (fresh.length > 0) this.ids = [...this.ids, ...fresh];
      this.nextCursor = page.next_cursor;
      this.hasMore = page.has_more;
    } catch {
      this.hasMore = false;
    } finally {
      this.loadingMore = false;
    }
  }
}

export const slideshow = new SlideshowStore();
