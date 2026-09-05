import type { Page, PhotoSummaryDto } from "../api/types";
import { call } from "../api";
import { recentMemories, rememberPhoto } from "../surpriseHistory";
import {
  moveIndex,
  resolveStartIndex,
  shouldLoadMore,
  uniquePhotoIds,
} from "../slideshowQueue";

export type SlideshowKind = "timeline" | "album" | "memory" | "photo" | "surprise";

export interface SlideshowStart {
  kind: SlideshowKind;
  label: string;
  ids: number[];
  startId?: number | null;
  nextCursor?: string | null;
  hasMore?: boolean;
  loadMore?: (cursor: string | null) => Promise<Page<PhotoSummaryDto>>;
}

export class SlideshowStore {
  starting = $state(false);
  private memoryLibrary: string | null = null;
  private normalIntervalMs = 5000;
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
  private session = 0;
  private pendingLoad: Promise<number> | null = null;

  start(opts: SlideshowStart) {
    this.session += 1;
    this.pendingLoad = null;
    this.starting = false;
    this.memoryLibrary = null;
    this.intervalMs = opts.kind === "surprise" ? 12000 : this.normalIntervalMs;
    const ids = uniquePhotoIds(opts.ids);
    this.ids = ids;
    this.index = resolveStartIndex(ids, opts.startId);
    this.kind = opts.kind;
    this.label = opts.label;
    this.nextCursor = opts.nextCursor ?? null;
    this.hasMore = Boolean(opts.hasMore && opts.loadMore);
    this.loadingMore = false;
    this.loader = opts.loadMore ?? null;
    this.playing = ids.length > 1 || this.hasMore;
    this.active = ids.length > 0;
  }

  close() {
    this.session += 1;
    this.pendingLoad = null;
    this.active = false;
    this.playing = false;
    this.ids = [];
    this.index = -1;
    this.nextCursor = null;
    this.hasMore = false;
    this.loadingMore = false;
    this.loader = null;
    this.starting = false;
    this.memoryLibrary = null;
  }

  async surprise(libraryKey: string, albumId: number | null, label: string, isCurrent: () => boolean) {
    this.close();
    const session = this.session;
    this.starting = true;
    const fetchBatch = async () => {
      const exclude = [...recentMemories(libraryKey), ...this.ids].slice(-256);
      return call<PhotoSummaryDto[]>("memories_surprise", { album_id: albumId, exclude_ids: exclude });
    };
    try {
      const items = await fetchBatch();
      if (session !== this.session || !isCurrent()) return null;
      if (!items.length) return 0;
      this.start({
        kind: "surprise", label, ids: items.map((p) => p.id), hasMore: items.length > 1,
        loadMore: async () => {
          const items = await fetchBatch();
          return { items, total: null, next_cursor: null, has_more: items.length > 0 };
        },
      });
      this.memoryLibrary = libraryKey;
      return items.length;
    } finally {
      if (session === this.session) this.starting = false;
    }
  }

  presented(id: number) {
    if (this.kind === "surprise" && this.memoryLibrary) rememberPhoto(this.memoryLibrary, id);
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
    if (!Number.isFinite(ms)) return;
    this.intervalMs = Math.max(1500, Math.min(15000, ms));
    if (this.kind !== "surprise") this.normalIntervalMs = this.intervalMs;
  }

  toggleLoop() {
    this.loop = !this.loop;
  }

  async next() {
    if (!this.active) return;
    const session = this.session;
    let appended = 0;
    if (this.index >= this.ids.length - 1 && this.hasMore) {
      appended = await this.loadMoreNow();
    } else {
      void this.ensureMoreAhead();
    }
    if (session !== this.session || !this.active) return;
    if (this.index >= this.ids.length - 1 && this.hasMore && appended === 0) {
      this.playing = false;
      return;
    }
    const next = moveIndex(this.index, this.ids.length, "next", this.kind === "surprise" ? false : this.loop);
    if (!this.loop && next === this.index && !this.hasMore) {
      this.playing = false;
    }
    this.index = next;
    if (this.kind === "surprise" && !this.hasMore && next === this.ids.length - 1) this.playing = false;
    if (this.kind === "surprise" && this.index > 200) {
      const trim = this.index - 100;
      this.ids = this.ids.slice(trim);
      this.index -= trim;
    }
  }

  prev() {
    if (!this.active) return;
    this.index = moveIndex(this.index, this.ids.length, "prev", this.kind === "surprise" ? false : this.loop);
  }

  async ensureMoreAhead() {
    if (!shouldLoadMore(this.index, this.ids.length) || !this.hasMore) return;
    await this.loadMoreNow();
  }

  async loadMoreNow(): Promise<number> {
    if (this.pendingLoad) return this.pendingLoad;
    const pending = this.fetchMore();
    this.pendingLoad = pending;
    try { return await pending; }
    finally { if (this.pendingLoad === pending) this.pendingLoad = null; }
  }

  private async fetchMore(): Promise<number> {
    if (!this.loader || this.loadingMore || !this.hasMore) return 0;
    const session = this.session;
    const loader = this.loader;
    const cursor = this.nextCursor;
    this.loadingMore = true;
    try {
      const page = await loader(cursor);
      if (session !== this.session || !this.active) return 0;
      const existing = new Set(this.ids);
      const fresh = page.items.map((p) => p.id).filter((id) => this.kind === "surprise" || !existing.has(id));
      if (fresh.length > 0) this.ids = [...this.ids, ...fresh];
      this.nextCursor = page.next_cursor;
      this.hasMore = page.has_more;
      return fresh.length;
    } catch {
      if (session === this.session) this.hasMore = false;
      return 0;
    } finally {
      if (session === this.session) this.loadingMore = false;
    }
  }
}

export const slideshow = new SlideshowStore();
