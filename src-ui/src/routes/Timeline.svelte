<script module lang="ts">
  import type { MemoryCard as CachedMemoryCard } from "../lib/api/all";
  import type { PhotoSummaryDto as CachedPhotoSummaryDto } from "../lib/api/types";

  type ZoomLevel = "day" | "month" | "year";

  let cachedTimeline:
    | {
        driveRoot: string | null;
        items: CachedPhotoSummaryDto[];
        nextCursor: string | null;
        hasMore: boolean;
        total: number | null;
        zoom: ZoomLevel;
        scrollTop: number;
        memoryCards: CachedMemoryCard[];
      }
    | null = null;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { photos } from "../lib/api/photos";
  import { memories, stacks, trash, type MemoryCard } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import { createVirtualScroll } from "../lib/virtualizer.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import { Check, Layers, Play } from "lucide-svelte";
  import type { PhotoSummaryDto } from "../lib/api/types";
  import { slideshow } from "../lib/stores/slideshow.svelte";

  interface Props { revealId?: number | null }
  let { revealId = null }: Props = $props();

  const currentDriveRoot = libraryStore.driveRoot;
  const currentTimelineCache =
    cachedTimeline?.driveRoot === currentDriveRoot ? cachedTimeline : null;
  const scrollStorageKey = `scroll:/timeline:${currentDriveRoot ?? "closed"}`;

  /// Zoom levels — Apple-Photos-style. `day` is default; `all` is the
  /// densest packed view with no headers.
  const TILE_PX: Record<ZoomLevel, number> = {
    day: 156, month: 112, year: 72,
  };
  const GAP_PX: Record<ZoomLevel, number> = {
    day: 4, month: 4, year: 3,
  };
  const LABEL_PX: Record<ZoomLevel, number> = {
    day: 36, month: 32, year: 28,
  };
  const ZOOM_ORDER: ZoomLevel[] = ["year", "month", "day"];
  /// Photos per page. Larger pages give the user a longer scroll
  /// runway between loadMore calls — on big libraries (50k+) the old
  /// 720 made the scrollbar feel like it was constantly catching up.
  const PAGE_LIMIT = 2000;
  /// Trigger loadMore when the virtualizer's `last` is within this
  /// many rows of the end. Bigger than the page size of label rows
  /// so we always start the next page well before the user runs out.
  const LOAD_MORE_LEAD_ROWS = 40;

  let items = $state<PhotoSummaryDto[]>(currentTimelineCache?.items ?? []);
  let nextCursor = $state<string | null>(currentTimelineCache?.nextCursor ?? null);
  let hasMore = $state(currentTimelineCache?.hasMore ?? true);
  let total = $state<number | null>(currentTimelineCache?.total ?? null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let containerW = $state(0);
  let containerH = $state(0);
  let zoom = $state<ZoomLevel>(currentTimelineCache?.zoom ?? "day");

  // Scan state derived from the global jobs store so it survives
  // page navigation. Earlier this lived in component-local $state and
  // reset on remount, making the scan look "lost" if the user moved
  // away and came back.
  const scanJob = $derived(jobs.byKind("scan"));
  const scanRunning = $derived(jobs.isRunning("scan"));

  // Pipeline-stage state for the Resume banners. Refreshed whenever a
  // scan / metadata / thumbnail job completes; computed once on mount
  // too so users who land on Timeline mid-pipeline see the right CTA.
  let pendingMetadata = $state<number>(0);
  const metadataRunning = $derived(jobs.isRunning("metadata"));
  const showResumeMetadata = $derived(
    !metadataRunning && !scanRunning && pendingMetadata > 0,
  );

  async function refreshPendingCounts() {
    try {
      const m = await library.pendingMetadataCount();
      pendingMetadata = m.pending_photos;
    } catch {
      // library not open yet, ignore
    }
  }

  async function resumeMetadata() {
    try {
      const r = await library.startMetadataExtraction();
      jobs.register(r.job_id, "metadata");
    } catch (e) {
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      toasts.error(`Couldn't resume metadata: ${msg}`);
    }
  }
  // Today's memories — surfaced as a horizontal strip above the
  // timeline. Hidden when the library has no qualifying memories.
  let memoryCards = $state<MemoryCard[]>(currentTimelineCache?.memoryCards ?? []);

  // Scrubber state
  let scrubHover = $state(false);
  let scrubDragging = $state(false);
  let scrubY = $state(0);                // mouse Y while hovering the track
  let scrollTop = $state(currentTimelineCache?.scrollTop ?? 0);

  // Multi-select dialog
  let showAddDialog = $state(false);

  // Index into `items` of the keyboard-focused cell. -1 = none.
  // Bumped on click so arrow nav picks up where the user left off; reset
  // when items reload from a fresh scan.
  let focusedIdx = $state(-1);

  function saveTimelineCache() {
    cachedTimeline = {
      driveRoot: currentDriveRoot,
      items,
      nextCursor,
      hasMore,
      total,
      zoom,
      scrollTop,
      memoryCards,
    };
  }

  function patchThumbnail(photoId: number, thumbnailPath: string) {
    items = items.map((p) => (
      p.id === photoId ? { ...p, thumbnail_path: thumbnailPath } : p
    ));
    saveTimelineCache();
  }

  // Drag-marquee state (selection by rectangle)
  let marqueeStart = $state<{ x: number; y: number } | null>(null);
  let marqueeCurrent = $state<{ x: number; y: number } | null>(null);
  /// Selection snapshot when the marquee started — restored as base
  /// each pointermove so cells that exit the rectangle deselect again.
  /// Held in a non-reactive ref because it doesn't need to render.
  let marqueeBase: Set<number> = new Set();
  const marqueeRect = $derived.by(() => {
    if (!marqueeStart || !marqueeCurrent) return null;
    const x = Math.min(marqueeStart.x, marqueeCurrent.x);
    const y = Math.min(marqueeStart.y, marqueeCurrent.y);
    const w = Math.abs(marqueeStart.x - marqueeCurrent.x);
    const h = Math.abs(marqueeStart.y - marqueeCurrent.y);
    return { x, y, w, h };
  });

  /// Number of columns for the current zoom + container width.
  const cols = $derived.by(() => {
    if (containerW <= 0) return 6;
    const tile = TILE_PX[zoom];
    const gap = GAP_PX[zoom];
    return Math.max(2, Math.floor((containerW + gap) / (tile + gap)));
  });

  /// Pixel height of a single row (square cells).
  const rowH = $derived.by(() => {
    if (containerW <= 0) return TILE_PX[zoom];
    const gap = GAP_PX[zoom];
    return Math.floor((containerW - gap * (cols - 1)) / cols);
  });

  async function loadMore(): Promise<boolean> {
    if (loading || !hasMore) return false;
    loading = true;
    error = null;
    try {
      const page = await photos.list({ cursor: nextCursor, limit: PAGE_LIMIT });
      const fresh = page.items.map((p) => p.id);
      items = items.concat(page.items);
      if (nextCursor === null) browseContext.set("timeline", fresh);
      else                     browseContext.extend(fresh);
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      if (page.total !== null) total = page.total;
      saveTimelineCache();
      return page.items.length > 0;
    } catch (e: unknown) {
      error = JSON.stringify(e);
      return false;
    } finally {
      loading = false;
    }
  }

  async function refreshFirstPage() {
    if (loading) return;
    loading = true;
    error = null;
    try {
      const page = await photos.list({ cursor: null, limit: PAGE_LIMIT });
      items = page.items;
      browseContext.set("timeline", page.items.map((p) => p.id));
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      if (page.total !== null) total = page.total;
      saveTimelineCache();
    } catch (e: unknown) {
      error = JSON.stringify(e);
    } finally {
      loading = false;
    }
  }

  async function startScan() {
    if (scanRunning) return;
    const placeholderId = `pending-scan-${Date.now()}`;
    jobs.register(placeholderId, "scan");
    toasts.success("Scanning library — feel free to navigate away.");
    try {
      const r = await library.startScan(false);
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "scan");
    } catch (e) {
      jobs.dismiss(placeholderId);
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      error = msg;
      toasts.error(`Couldn't start scan: ${msg}`);
    }
  }

  async function refreshStacks() {
    try {
      const result = await stacks.refresh();
      toasts.success(`${result.stacks_found} ${result.stacks_found === 1 ? "stack" : "stacks"} ready`);
      await refreshFirstPage();
    } catch (e) {
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      toasts.error(`Couldn't refresh stacks: ${msg}`);
    }
  }
  // When a scan completes, refresh the photo list so the user sees
  // the new arrivals without having to reload manually.
  let lastScanCompleteId = $state<string | null>(null);
  $effect(() => {
    if (scanJob && scanJob.status === "complete" && scanJob.id !== lastScanCompleteId) {
      lastScanCompleteId = scanJob.id;
      items = [];
      nextCursor = null;
      hasMore = true;
      saveTimelineCache();
      loadMore();
    }
  });

  function pad2(n: number) { return n < 10 ? `0${n}` : `${n}`; }

  /// Bucket key per zoom level. NULL date sorts into a single "without
  /// date" bucket at the end of the timeline.
  function bucketKey(iso: string | null, lvl: ZoomLevel): string {
    if (!iso) return "no-date";
    const d = new Date(iso);
    switch (lvl) {
      case "day":   return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
      case "month": return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}`;
      case "year":  return `${d.getFullYear()}`;
    }
  }
  function bucketLabel(iso: string | null, lvl: ZoomLevel): string {
    if (!iso) return "Without date";
    const d = new Date(iso);
    switch (lvl) {
      case "day":   return d.toLocaleString("en", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
      case "month": return d.toLocaleString("en", { month: "long", year: "numeric" });
      case "year":  return `${d.getFullYear()}`;
    }
  }

  // Build rows: optional label rows interleaved with photo rows.
  // Total height of the memories strip when shown. Plumbed into the
  // virtualizer as the first virtual row so it scrolls away with the
  // rest of the content.
  const MEMORIES_STRIP_HEIGHT = 340;

  type Row =
    | { kind: "memories"; height: number; cards: MemoryCard[] }
    | { kind: "label"; height: number; label: string; firstIso: string | null }
    | { kind: "photos"; height: number; photos: PhotoSummaryDto[]; firstIso: string | null };

  const rows = $derived.by((): Row[] => {
    const out: Row[] = [];
    if (memoryCards.length > 0) {
      out.push({ kind: "memories", height: MEMORIES_STRIP_HEIGHT, cards: memoryCards });
    }
    const C = cols;
    const RH = rowH;
    const LH = LABEL_PX[zoom];
    let lastKey = "";
    let bucket: PhotoSummaryDto[] = [];
    let bucketFirstIso: string | null = null;
    const flush = () => {
      while (bucket.length > 0) {
        out.push({
          kind: "photos",
          height: RH,
          photos: bucket.slice(0, C),
          firstIso: bucketFirstIso,
        });
        bucket = bucket.slice(C);
      }
    };
    for (const p of items) {
      const k = bucketKey(p.date_taken, zoom);
      if (k !== lastKey) {
        flush();
        if (LH > 0) {
          out.push({
            kind: "label",
            height: LH,
            label: bucketLabel(p.date_taken, zoom),
            firstIso: p.date_taken,
          });
        }
        lastKey = k;
        bucketFirstIso = p.date_taken;
      }
      bucket.push(p);
    }
    flush();
    return out;
  });

  const v = createVirtualScroll<Row>({
    rows: () => rows,
    scrollEl: () => scrollEl,
    overscan: 10,
  });

  const estimatedTotalHeight = $derived.by(() => {
    if (total == null || total <= items.length || cols <= 0) return v.totalHeight;
    const totalPhotoRows = Math.ceil(total / cols);
    const loadedPhotoRows = Math.ceil(items.length / cols);
    const extraPhotoRows = Math.max(0, totalPhotoRows - loadedPhotoRows);
    // Each photo bucket gets a label row. Empirically there's roughly
    // one label per ~3-5 photo rows in day-zoom; over-estimating here
    // means the scrubber drags further than necessary on the first
    // load but it's harmless once the real rows materialise (the
    // virtualizer's actual offsets take over). Under-estimating used
    // to make the scrollbar visibly snap upward each time a page came
    // in. We add one label-row's height per ~4 photo rows estimated.
    const extraLabelRows = Math.ceil(extraPhotoRows / 4);
    const photoRowH = Math.max(1, rowH + GAP_PX[zoom]);
    const labelH = LABEL_PX[zoom];
    return (
      v.totalHeight + extraPhotoRows * photoRowH + extraLabelRows * labelH
    );
  });

  $effect(() => {
    return v.attach();
  });

  $effect(() => {
    if (v.last >= rows.length - LOAD_MORE_LEAD_ROWS && hasMore && !loading) {
      loadMore();
    }
  });

  $effect(() => {
    if (!scrollEl) return;
    const ro = new ResizeObserver(() => {
      const r = scrollEl!.getBoundingClientRect();
      containerW = r.width - 14; // leave room for the scrubber gutter
      containerH = r.height;
    });
    ro.observe(scrollEl);
    const r = scrollEl.getBoundingClientRect();
    containerW = r.width - 14;
    containerH = r.height;
    const onScroll = () => {
      scrollTop = scrollEl!.scrollTop;
      saveTimelineCache();
      // Persist so that returning from PhotoDetail lands the user back
      // at the same row instead of the top of the timeline.
      try { sessionStorage.setItem(scrollStorageKey, String(scrollTop)); } catch {}
    };
    scrollEl.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      ro.disconnect();
      scrollEl?.removeEventListener("scroll", onScroll);
    };
  });

  // Restore scroll position once the virtualizer has built enough content
  // to reach the saved offset. Runs as a $effect so it re-checks every
  // time totalHeight grows (loadMore appends pages).
  let scrollRestored = $state(false);
  $effect(() => {
    if (scrollRestored || !scrollEl) return;
    const raw = (() => { try { return sessionStorage.getItem(scrollStorageKey); } catch { return null; } })();
    const target = raw ? parseInt(raw, 10) : 0;
    if (!Number.isFinite(target) || target <= 0) {
      scrollRestored = true;
      return;
    }
    if (estimatedTotalHeight >= target + 16) {
      scrollEl.scrollTop = target;
      scrollRestored = true;
    } else if (hasMore && !loading) {
      // Pre-load more pages so the virtualizer can paint the saved
      // position without flashing the top of the list.
      loadMore();
    } else if (!hasMore) {
      // Library is shorter than where we left off (photos trashed, etc.)
      // — give up, leave the user at the bottom rather than stuck in
      // an effect loop.
      scrollRestored = true;
    }
  });

  onMount(() => {
    if (items.length === 0) loadMore();
    if (currentTimelineCache?.scrollTop && scrollEl) {
      scrollEl.scrollTop = currentTimelineCache.scrollTop;
      scrollRestored = true;
    }
    if (memoryCards.length === 0) {
      memories.today().then((c) => {
        memoryCards = c;
        saveTimelineCache();
      }).catch(() => {});
    }
    refreshPendingCounts();
    window.addEventListener("keydown", onGlobalKey);

    // Streaming-scanner integration. Three Tauri events drive in-place
    // updates to the timeline so users see photos / dates / thumbnails
    // populate progressively instead of staring at an empty grid:
    //
    //   thumbnail:ready    → fetch the batch of upgraded rows and patch
    //                        their thumbnail_path in-place
    //   metadata:progress  → refresh totals so date-group headers reflow
    //                        as EXIF dates land
    //   thumbnails:complete + metadata:complete → recount pending so
    //                        the Resume banners hide
    const unlistens: Promise<UnlistenFn>[] = [];
    unlistens.push(
      listen<{ photo_ids: number[] }>("thumbnail:ready", async (e) => {
        const ids = e.payload?.photo_ids ?? [];
        if (ids.length === 0) return;
        // Only refetch rows the timeline currently has loaded; the rest
        // will get the new thumbnail_path naturally on next load.
        const have = new Set(items.map((p) => p.id));
        const wanted = ids.filter((id) => have.has(id));
        if (wanted.length === 0) return;
        try {
          const fresh = await photos.getMany(wanted);
          const byId = new Map(fresh.map((p) => [p.id, p]));
          items = items.map((p) => {
            const u = byId.get(p.id);
            if (!u) return p;
            return { ...p, thumbnail_path: u.thumbnail_path ?? p.thumbnail_path };
          });
          saveTimelineCache();
        } catch {
          // best-effort — the on-demand thumbnail path covers misses
        }
      }),
    );
    let throttle: ReturnType<typeof setTimeout> | null = null;
    unlistens.push(
      listen("metadata:progress", () => {
        // Throttle to once per second; we only need to refresh the
        // pending count, not re-fetch photo rows.
        if (throttle != null) return;
        throttle = setTimeout(() => {
          throttle = null;
          refreshPendingCounts();
        }, 1000);
      }),
    );
    unlistens.push(listen("metadata:complete", () => refreshPendingCounts()));
    let scanRefresh: ReturnType<typeof setTimeout> | null = null;
    unlistens.push(
      listen<{ files_processed?: number }>("scan:progress", () => {
        if (scanRefresh != null) return;
        scanRefresh = setTimeout(() => {
          scanRefresh = null;
          refreshFirstPage();
        }, 1200);
      }),
    );
    unlistens.push(listen("scan:complete", () => {
      refreshPendingCounts();
      refreshFirstPage();
    }));

    return () => {
      window.removeEventListener("keydown", onGlobalKey);
      if (throttle != null) clearTimeout(throttle);
      if (scanRefresh != null) clearTimeout(scanRefresh);
      Promise.all(unlistens).then((fns) => fns.forEach((f) => f()));
    };
  });

  /// Direction of the most-recent zoom change. Drives the cell
  /// keyframe animation: zooming IN (more detail, smaller buckets →
  /// larger tiles) lets cells appear to grow into place; zooming OUT
  /// makes them fade-shrink. The class lives on `.scroll` for ~280 ms,
  /// long enough for a 240 ms keyframe to play out.
  let zoomDir = $state<"in" | "out" | null>(null);
  let zoomTimer: ReturnType<typeof setTimeout> | null = null;
  function setZoom(z: ZoomLevel) {
    if (z === zoom) return;
    const oldIdx = ZOOM_ORDER.indexOf(zoom);
    const newIdx = ZOOM_ORDER.indexOf(z);
    zoomDir = newIdx > oldIdx ? "in" : "out";
    zoom = z;
    saveTimelineCache();
    if (zoomTimer) clearTimeout(zoomTimer);
    zoomTimer = setTimeout(() => (zoomDir = null), 320);
  }

  // ----------- selection -----------
  function onCellClick(e: MouseEvent, photo: PhotoSummaryDto) {
    focusedIdx = items.findIndex((p) => p.id === photo.id);
    handleCellClick(e, photo.id, items.map((p) => p.id));
  }

  // ----------- drag-marquee selection -----------
  // Coalesce pointermove updates to one per animation frame. Each update
  // does a getBoundingClientRect() per visible cell + writes to the
  // reactive selection set, which invalidates `class:selected` on every
  // cell. At 120 Hz pointer rates with hundreds of cells visible this
  // overwhelmed the renderer and looked like a hang.
  let marqueeRaf = 0;
  function onScrollPointerDown(e: PointerEvent) {
    // Only respond to primary-button drags that started on empty space
    // (not on a cell or any interactive child).
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("a.cell, .scrubber, .zoom-pill, button, input, [data-no-marquee]")) return;
    marqueeBase = e.shiftKey ? new Set(selection.ids) : new Set();
    if (!e.shiftKey) selection.clear();
    marqueeStart = { x: e.clientX, y: e.clientY };
    marqueeCurrent = { x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function onScrollPointerMove(e: PointerEvent) {
    if (!marqueeStart) return;
    marqueeCurrent = { x: e.clientX, y: e.clientY };
    if (marqueeRaf !== 0) return;
    marqueeRaf = requestAnimationFrame(() => {
      marqueeRaf = 0;
      updateMarqueeSelection();
    });
  }
  function onScrollPointerUp(e: PointerEvent) {
    if (!marqueeStart) return;
    try { (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId); } catch {}
    if (marqueeRaf !== 0) {
      cancelAnimationFrame(marqueeRaf);
      marqueeRaf = 0;
    }
    marqueeStart = null;
    marqueeCurrent = null;
  }
  function updateMarqueeSelection() {
    if (!scrollEl || !marqueeRect) return;
    const rect = marqueeRect;
    // Re-base from the snapshot, then add every visible cell whose
    // bounding rect intersects the marquee.
    const next = new Set(marqueeBase);
    const cells = scrollEl.querySelectorAll<HTMLAnchorElement>("a.cell");
    cells.forEach((cellEl) => {
      const cr = cellEl.getBoundingClientRect();
      const intersects =
        cr.left   < rect.x + rect.w &&
        cr.right  > rect.x &&
        cr.top    < rect.y + rect.h &&
        cr.bottom > rect.y;
      if (!intersects) return;
      const href = cellEl.getAttribute("href") ?? "";
      const m = href.match(/id=(\d+)/);
      if (m) next.add(Number(m[1]));
    });
    // Skip the assignment when the resulting set is identical — avoids
    // re-rendering every visible cell when the marquee crawls inside an
    // empty gap between rows.
    if (setsEqual(selection.ids, next)) return;
    selection.ids = next;
  }
  function setsEqual<T>(a: Set<T>, b: Set<T>): boolean {
    if (a.size !== b.size) return false;
    for (const x of a) if (!b.has(x)) return false;
    return true;
  }

  async function bulkTrash() {
    const ids = selection.list();
    if (ids.length === 0) return;
    // Snapshot the rows we're about to drop so undo can splice them
    // back in their original positions.
    const dropSet = new Set(ids);
    const snapshot = items
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    try {
      await trash.trashPhotos(ids);
      items = items.filter((p) => !dropSet.has(p.id));
      selection.clear();
      toasts.undoable(
        `${ids.length} ${ids.length === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          const next = items.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          items = next;
        },
      );
    } catch (e) {
      toasts.error(`Couldn't move to trash: ${e}`);
    }
  }

  function onGlobalKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (selection.active()) {
      if (e.key === "Escape") { selection.clear(); e.preventDefault(); return; }
      else if (e.key === "Delete" || e.key === "Backspace") { bulkTrash(); e.preventDefault(); return; }
      else if (e.key === "a" || e.key === "A") {
        if (!e.metaKey && !e.ctrlKey) { showAddDialog = true; e.preventDefault(); return; }
      }
    }
    // Grid navigation. Active when no selection and the user has clicked
    // on the timeline at least once (focusedIdx tracks the last cell).
    if (!scrollEl) return;
    const C = cols;
    const N = items.length;
    if (N === 0) return;
    let next = focusedIdx;
    switch (e.key) {
      case "ArrowRight": next = Math.min(N - 1, (focusedIdx < 0 ? 0 : focusedIdx + 1)); break;
      case "ArrowLeft":  next = Math.max(0, (focusedIdx < 0 ? 0 : focusedIdx - 1)); break;
      case "ArrowDown":  next = Math.min(N - 1, (focusedIdx < 0 ? 0 : focusedIdx + C)); break;
      case "ArrowUp":    next = Math.max(0, (focusedIdx < 0 ? 0 : focusedIdx - C)); break;
      case "PageDown":   next = Math.min(N - 1, (focusedIdx < 0 ? 0 : focusedIdx + C * pageRows())); break;
      case "PageUp":     next = Math.max(0, (focusedIdx < 0 ? 0 : focusedIdx - C * pageRows())); break;
      case "Home":       next = 0; break;
      case "End":        next = N - 1; break;
      case "Enter": {
        if (focusedIdx >= 0 && focusedIdx < N) {
          window.location.hash = `/photo?id=${items[focusedIdx].id}`;
          e.preventDefault();
        }
        return;
      }
      default: return;
    }
    e.preventDefault();
    focusedIdx = next;
    scrollFocusedIntoView();
  }
  function pageRows(): number {
    return Math.max(1, Math.floor((containerH - 32) / Math.max(1, rowH + GAP_PX[zoom])));
  }
  /// Locate the virtualizer row that contains `photoId`. Returns -1
  /// if not loaded yet. The row index walks the same `rows` derived
  /// the virtualizer sees, so `v.offsets[idx]` is the exact pixel
  /// offset to that row — no estimation, no off-by-label-row drift.
  function rowIndexForPhoto(photoId: number): number {
    const list = rows;
    for (let i = 0; i < list.length; i++) {
      const row = list[i];
      if (row.kind !== "photos") continue;
      for (const p of row.photos) {
        if (p.id === photoId) return i;
      }
    }
    return -1;
  }

  function scrollFocusedIntoView(opts: { smooth?: boolean } = {}) {
    if (!scrollEl || focusedIdx < 0) return;
    const id = items[focusedIdx]?.id;
    if (id == null) return;
    const behavior: ScrollBehavior = opts.smooth ? "smooth" : "auto";
    const rowIdx = rowIndexForPhoto(id);
    if (rowIdx >= 0) {
      // Authoritative path: use the virtualizer's offset table for the
      // exact y of the row, then center it in the viewport.
      const rowOffset = v.offsets[rowIdx] ?? 0;
      const rowHeight = rows[rowIdx]?.height ?? rowH;
      const target = Math.max(0, rowOffset - (containerH - rowHeight) / 2);
      scrollEl.scrollTo({ top: target, behavior });
      return;
    }
    // Row not in `rows` yet (still paginating). Wait one frame and try
    // again — by then loadMore should have grown `rows`.
    requestAnimationFrame(() => {
      if (!scrollEl) return;
      const retryIdx = rowIndexForPhoto(id);
      if (retryIdx < 0) return;
      const rowOffset = v.offsets[retryIdx] ?? 0;
      const rowHeight = rows[retryIdx]?.height ?? rowH;
      const target = Math.max(0, rowOffset - (containerH - rowHeight) / 2);
      scrollEl.scrollTo({ top: target, behavior });
    });
  }

  let revealingId = $state<number | null>(null);
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  async function revealPhotoInTimeline(photoId: number) {
    if (!Number.isFinite(photoId) || photoId <= 0 || revealingId === photoId) return;
    revealingId = photoId;
    try {
      while (items.findIndex((p) => p.id === photoId) < 0 && hasMore) {
        if (loading) {
          await sleep(80);
          continue;
        }
        const advanced = await loadMore();
        if (!advanced) break;
      }

      const idx = items.findIndex((p) => p.id === photoId);
      if (idx < 0) {
        toasts.error("That photo is not in the loaded timeline.");
        return;
      }
      focusedIdx = idx;
      selection.set(photoId);
      // Smooth scroll for "show in timeline" so the eye can track
      // where the photo lives. Plain keyboard nav stays instant.
      scrollFocusedIntoView({ smooth: true });
      try {
        history.replaceState({}, "", "#/timeline");
        window.dispatchEvent(new HashChangeEvent("hashchange"));
      } catch {}
    } finally {
      revealingId = null;
    }
  }

  $effect(() => {
    if (revealId != null) void revealPhotoInTimeline(revealId);
  });

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1; // wheel up = zoom in (more detail)
    const i = ZOOM_ORDER.indexOf(zoom);
    const next = Math.max(0, Math.min(ZOOM_ORDER.length - 1, i + dir));
    setZoom(ZOOM_ORDER[next]);
  }

  function startTimelineSlideshow() {
    const selected = selection.active()
      ? items.filter((p) => selection.has(p.id))
      : items;
    if (selected.length === 0) return;
    const scoped = selection.active();
    slideshow.start({
      kind: "timeline",
      label: scoped ? "Selected photos" : "Timeline",
      ids: selected.map((p) => p.id),
      nextCursor: scoped ? null : nextCursor,
      hasMore: scoped ? false : hasMore,
      loadMore: scoped ? undefined : (cursor) => photos.list({ cursor, limit: PAGE_LIMIT }),
    });
  }

  // ----------- on-demand thumbnail generation -----------
  // A cell that scrolls into view without a thumbnail_path triggers a
  // generation request. We cap parallelism implicitly via the server's
  // 8-permit ThumbnailService limiter.
  const inflight = new Set<number>();
  async function requestIfMissing(photo: PhotoSummaryDto, idx: number) {
    if (photo.thumbnail_path) return;
    if (inflight.has(photo.id)) return;
    inflight.add(photo.id);
    try {
      const r = await photos.requestThumbnail(photo.id);
      if (r.thumbnail_path && items[idx]?.id === photo.id) {
        // Reactivity: assign the index, not just mutate a field — Svelte
        // 5 proxies most mutations but a fresh shallow copy is the
        // surest way to trigger the re-render.
        items[idx] = { ...items[idx], thumbnail_path: r.thumbnail_path };
      }
    } catch {
      // swallow — frontend retries on the next intersection
    } finally {
      inflight.delete(photo.id);
    }
  }

  function cellAttach(node: HTMLAnchorElement, photo: PhotoSummaryDto) {
    if (photo.thumbnail_path) return;
    const idx = items.findIndex((p) => p.id === photo.id);
    if (idx < 0) return;
    // Debounced viewport-driven generation. The IO fires on intersect
    // AND de-intersect; we only fire the IPC after the cell has been
    // continuously visible for 250 ms. This prevents fast scrolls from
    // queuing a thumb request for every cell that flashed past — the
    // backend used to chew through the full scrolled-past range first
    // while the cells the user was actually looking at sat empty.
    let timer: ReturnType<typeof setTimeout> | null = null;
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          if (timer == null) {
            timer = setTimeout(() => {
              timer = null;
              requestIfMissing(photo, idx);
              io.disconnect();
            }, 250);
          }
        } else if (timer != null) {
          clearTimeout(timer);
          timer = null;
        }
      }
    }, { root: scrollEl, rootMargin: "100px" });
    io.observe(node);
    return {
      destroy() {
        if (timer != null) clearTimeout(timer);
        io.disconnect();
      },
    };
  }

  // ----------- scrubber -----------
  const trackHeight = $derived(Math.max(containerH - 24, 80));
  const scrollableMax = $derived(Math.max(0, estimatedTotalHeight - containerH));
  const thumbY = $derived.by(() => {
    if (scrollableMax <= 0) return 0;
    const ratio = Math.min(1, Math.max(0, scrollTop / scrollableMax));
    return ratio * (trackHeight - 28);
  });
  const scrubVisible = $derived(scrubHover || scrubDragging || (v.totalHeight > containerH));

  /// What bucket are we currently scrolled into? Looks up the row that
  /// covers `scrollTop`, walks back to the nearest label.
  function bucketAtY(y: number): string {
    if (rows.length === 0) return "";
    // Binary-search v.offsets for the largest offset <= y.
    let lo = 0, hi = rows.length - 1, best = 0;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (v.offsets[mid] <= y) { best = mid; lo = mid + 1; }
      else hi = mid - 1;
    }
    for (let i = best; i >= 0; i--) {
      if (rows[i]?.kind === "label") {
        return (rows[i] as { kind: "label"; label: string }).label;
      }
    }
    const head = rows[0];
    if (!head) return "";
    if (head.kind === "memories") return "";
    return bucketLabel(head.firstIso, zoom);
  }
  const draggedBucket = $derived.by(() => {
    if (!scrubDragging) return "";
    const ratio = trackHeight > 28 ? scrubY / (trackHeight - 28) : 0;
    return bucketAtY(Math.min(1, Math.max(0, ratio)) * scrollableMax);
  });

  function onScrubPointer(e: PointerEvent) {
    if (!scrollEl) return;
    const trackEl = (e.currentTarget as HTMLElement).querySelector(".track") as HTMLElement | null;
    if (!trackEl) return;
    const rect = trackEl.getBoundingClientRect();
    scrubY = Math.max(0, Math.min(rect.height - 28, e.clientY - rect.top - 14));
    if (scrubDragging) {
      const ratio = scrubY / Math.max(1, rect.height - 28);
      scrollEl.scrollTop = ratio * scrollableMax;
    }
  }
  function onScrubDown(e: PointerEvent) {
    scrubDragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onScrubPointer(e);
  }
  function onScrubUp(e: PointerEvent) {
    scrubDragging = false;
    try { (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId); } catch {}
  }
</script>

<PageHeader title="Timeline">
  <button class="icon-action" onclick={startTimelineSlideshow} disabled={items.length === 0} title="Start slideshow" aria-label="Start timeline slideshow">
    <Play size={15} strokeWidth={2} />
  </button>
  <div class="zoom-pill" role="tablist" aria-label="Zoom level">
    <button class:on={zoom === "year"}  onclick={() => setZoom("year")}  aria-label="Years">Year</button>
    <button class:on={zoom === "month"} onclick={() => setZoom("month")} aria-label="Months">Month</button>
    <button class:on={zoom === "day"}   onclick={() => setZoom("day")}   aria-label="Days">Day</button>
  </div>
  {#if scanRunning && scanJob}
    <span class="scan-status mono">
      Scanning · {scanJob.processed.toLocaleString()}
      / {scanJob.total != null ? scanJob.total.toLocaleString() : "?"}
    </span>
  {:else}
    <span class="count mono">
      {(total ?? items.length).toLocaleString()}<span class="muted"> photos</span>
    </span>
    <button class="primary" onclick={startScan}>Scan now</button>
    <button class="ghost" onclick={refreshStacks}>Refresh stacks</button>
  {/if}
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

{#if showResumeMetadata}
  <div class="resume-banners">
    {#if showResumeMetadata}
      <div class="resume-banner">
        <span class="msg">
          {pendingMetadata.toLocaleString()} photos still need EXIF + place data.
        </span>
        <button class="primary" onclick={resumeMetadata}>Resume reading metadata</button>
      </div>
    {/if}
  </div>
{/if}

<div class="timeline-host">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="scroll"
    class:zoom-in={zoomDir === "in"}
    class:zoom-out={zoomDir === "out"}
    bind:this={scrollEl}
    onwheel={onWheel}
    onpointerdown={onScrollPointerDown}
    onpointermove={onScrollPointerMove}
    onpointerup={onScrollPointerUp}
    onpointercancel={onScrollPointerUp}
  >
    <div class="inner" style="height: {Math.max(v.totalHeight, estimatedTotalHeight)}px">
      {#each rows.slice(v.first, v.last) as row, idx (v.first + idx)}
        {@const i = v.first + idx}
        <div
          class="row"
          class:row-label={row.kind === "label"}
          class:row-memories={row.kind === "memories"}
          style="transform: translateY({v.offsets[i]}px); height: {row.height}px;"
        >
          {#if row.kind === "label"}
            <span class="label">{row.label}</span>
          {:else if row.kind === "memories"}
            <section class="memories-strip" aria-label="Memories" data-no-marquee="true">
              <h3 class="strip-title">From your library</h3>
              <div class="strip-row">
                {#each row.cards as c (c.id)}
                  <a class="memory-card" href="#/memory?id={c.id}" data-no-marquee="true">
                    {#if c.hero_thumbnail_path}
                      <img src={thumbUrl(libraryStore.driveRoot, c.hero_thumbnail_path) ?? ""} alt="" loading="lazy" />
                    {/if}
                    <div class="memory-overlay">
                      <strong class="memory-title">{c.title}</strong>
                      <span class="memory-count mono">{c.photo_count} photos</span>
                    </div>
                  </a>
                {/each}
              </div>
            </section>
          {:else}
            <div
              class="photos"
              style="grid-template-columns: repeat({cols}, 1fr); gap: {GAP_PX[zoom]}px;"
            >
              {#each row.photos as photo (photo.id)}
                <a
                  class="cell"
                  class:selected={selection.has(photo.id)}
                  class:focused={focusedIdx >= 0 && items[focusedIdx]?.id === photo.id}
                  href="#/photo?id={photo.id}"
                  title="#{photo.id}"
                  use:thumbnailOnVisible={{
                    id: photo.id,
                    thumbnailPath: photo.thumbnail_path,
                    root: scrollEl,
                    onReady: (path) => patchThumbnail(photo.id, path),
                  }}
                  onclick={(e) => onCellClick(e, photo)}
                >
                  {#if photo.thumbnail_path}
                    <img
                      src={thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) ?? ""}
                      alt=""
                      loading="lazy"
                      decoding="async"
                    />
                  {/if}
                  {#if selection.has(photo.id)}
                    <span class="check" aria-hidden="true">
                      <Check size={14} strokeWidth={2.5} />
                    </span>
                  {/if}
                  {#if photo.stack}
                    <span class="stack-badge mono" title="{photo.stack.member_count} photos in stack">
                      <Layers size={13} strokeWidth={2.2} />
                      <span>{photo.stack.member_count}</span>
                    </span>
                  {/if}
                </a>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  {#if marqueeRect}
    <div
      class="marquee"
      style="left: {marqueeRect.x}px; top: {marqueeRect.y}px; width: {marqueeRect.w}px; height: {marqueeRect.h}px;"
      data-no-marquee="true"
      aria-hidden="true"
    ></div>
  {/if}

  <!-- Scrubber: floats over the right edge of the scrollable area. -->
  <div
    class="scrubber"
    class:visible={scrubVisible}
    class:dragging={scrubDragging}
    onmouseenter={() => (scrubHover = true)}
    onmouseleave={() => (scrubHover = false)}
    onpointermove={onScrubPointer}
    onpointerdown={onScrubDown}
    onpointerup={onScrubUp}
    onpointercancel={onScrubUp}
    role="slider"
    aria-label="Scroll position"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={scrollableMax > 0 ? Math.round((scrollTop / scrollableMax) * 100) : 0}
    tabindex="-1"
  >
    <div class="track"></div>
    <div class="thumb" style="top: {thumbY}px"></div>
    {#if scrubDragging && draggedBucket}
      <div class="bubble" style="top: {scrubY}px">{draggedBucket}</div>
    {/if}
  </div>
</div>

{#if selection.active()}
  <SelectionBar
    count={selection.size()}
    onAddToAlbum={() => (showAddDialog = true)}
    onTrash={bulkTrash}
    onCancel={() => selection.clear()}
  />
{/if}

{#if showAddDialog}
  <AddToAlbumDialog
    photoIds={selection.list()}
    onclose={() => (showAddDialog = false)}
    onsuccess={() => selection.clear()}
  />
{/if}

<style>
  .timeline-host {
    flex: 1;
    position: relative;
    overflow: hidden;
    min-height: 0;
  }

  /* ----- memories strip ----- */
  .memories-strip {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    /* No horizontal padding — the .scroll container already provides
       --s-7 on each side, so the first card's left edge lines up with
       the first photo column below. */
    padding: var(--s-2) 0 var(--s-3);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    overflow: hidden;
  }
  .strip-title {
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0;
    flex-shrink: 0;
  }
  .strip-row {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: var(--s-3);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
  }
  .memory-card {
    flex: 0 0 auto;
    width: 280px;
    height: 100%;
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    position: relative;
    display: flex;
    flex-direction: column;
    color: inherit;
    text-decoration: none;
    transition: transform var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease);
  }
  .memory-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 22px rgba(0,0,0,0.35);
    border-color: var(--accent);
  }
  .memory-card img {
    width: 100%;
    flex: 1;
    min-height: 0;
    object-fit: cover;
    display: block;
  }
  .memory-overlay {
    flex-shrink: 0;
    padding: var(--s-2) var(--s-3) var(--s-3);
    background: var(--bg-paper);
    border-top: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .memory-title {
    font-family: var(--font-display);
    font-size: var(--t-sm);
    font-weight: 600;
    line-height: 1.2;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .memory-count {
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }

  .scroll {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    padding: var(--s-4) var(--s-7) var(--s-7);
    contain: strict;
    /* Custom scrubber handles seeking; hide the native bar. */
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  .scroll::-webkit-scrollbar { display: none; }
  .scroll { touch-action: pan-y; }
  .marquee {
    position: fixed;
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    border: 1px solid var(--accent);
    border-radius: 2px;
    pointer-events: none;
    z-index: 6;
  }
  .inner {
    position: relative;
    width: 100%;
  }
  .row {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    contain: layout style paint;
    will-change: transform;
    transition: transform 220ms cubic-bezier(0.22, 0.61, 0.36, 1),
                height 220ms cubic-bezier(0.22, 0.61, 0.36, 1);
  }
  .row-label {
    display: flex;
    align-items: flex-end;
    padding-bottom: var(--s-1);
  }
  .label {
    font-size: var(--t-sm);
    font-weight: 600;
    color: var(--ink);
    letter-spacing: -0.005em;
  }
  .photos {
    display: grid;
    padding: 1px 0;
    transition: gap 220ms cubic-bezier(0.22, 0.61, 0.36, 1);
  }
  .cell {
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    display: block;
    aspect-ratio: 1;
    min-width: 0;
    position: relative;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .cell:hover {
    filter: brightness(1.06);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .cell:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .cell.selected {
    box-shadow: inset 0 0 0 3px var(--accent);
  }
  .cell.selected img { filter: brightness(0.85); }
  .cell.focused {
    box-shadow: 0 0 0 2px var(--accent);
    z-index: 1;
  }
  /* Cinematic zoom: cells appear to grow when the user zooms in
     (more detail, larger tiles) and shrink-fade when zooming out. The
     scale is intentionally subtle — the grid is virtualized and the
     visible row count actually changes, so a big scale would look
     glitchy. The 0.04 delta + the existing transform/height/gap
     transitions on .row and .photos combine into a coherent feel. */
  @keyframes pv-zoom-in-cell {
    from { transform: scale(0.92); opacity: 0.55; }
    to   { transform: scale(1);    opacity: 1;    }
  }
  @keyframes pv-zoom-out-cell {
    from { transform: scale(1.08); opacity: 0.55; }
    to   { transform: scale(1);    opacity: 1;    }
  }
  .scroll.zoom-in .cell {
    animation: pv-zoom-in-cell 240ms cubic-bezier(0.22, 0.61, 0.36, 1) both;
    transform-origin: center center;
  }
  .scroll.zoom-out .cell {
    animation: pv-zoom-out-cell 240ms cubic-bezier(0.22, 0.61, 0.36, 1) both;
    transform-origin: center center;
  }
  .scroll.zoom-in .label,
  .scroll.zoom-out .label {
    animation: pv-zoom-in-cell 240ms cubic-bezier(0.22, 0.61, 0.36, 1) both;
  }
  .cell .check {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--bg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 2px 6px rgba(0,0,0,0.4);
    pointer-events: none;
  }
  .stack-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    min-width: 30px;
    height: 24px;
    padding: 0 7px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.68);
    color: #fff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    font-size: var(--t-xs);
    font-weight: 700;
    line-height: 1;
    box-shadow: 0 2px 7px rgba(0,0,0,0.35);
    pointer-events: none;
  }
  .cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .scan-status,
  .count {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .scan-status { color: var(--accent); }

  .icon-action {
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: var(--r-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--ink-soft);
  }
  .icon-action:hover:not(:disabled) {
    color: var(--ink);
    background: var(--bg-card);
  }
  .icon-action:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .resume-banners {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-2) var(--s-7) 0;
  }
  .resume-banner {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-card);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-sm);
    font-size: var(--t-sm);
  }
  .resume-banner .msg {
    flex: 1;
    color: var(--ink);
  }
  .resume-banner .primary {
    background: var(--accent);
    color: var(--bg);
    border: none;
    padding: 6px 14px;
    border-radius: var(--r-sm);
    font-size: var(--t-sm);
    font-weight: 500;
    cursor: pointer;
  }
  .resume-banner .primary:hover { filter: brightness(1.05); }

  .zoom-pill {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--bg-card);
  }
  .zoom-pill button {
    background: transparent;
    border: none;
    padding: 4px 12px;
    font-size: var(--t-xs);
    color: var(--ink-muted);
    border-radius: 999px;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
  }
  .zoom-pill button:hover { color: var(--ink); }
  .zoom-pill button.on {
    background: var(--bg-elev);
    color: var(--ink);
  }

  /* ----- scrubber ----- */
  .scrubber {
    position: absolute;
    top: var(--s-4);
    bottom: var(--s-4);
    right: 4px;
    width: 14px;
    z-index: 4;
    cursor: pointer;
    opacity: 0;
    transition: opacity 220ms var(--ease);
    touch-action: none;
  }
  .scrubber.visible { opacity: 1; }
  .scrubber:hover { opacity: 1; }
  .scrubber.dragging { opacity: 1; }
  .track {
    position: absolute;
    top: 0; bottom: 0;
    left: 6px;
    width: 2px;
    background: var(--line);
    border-radius: 1px;
  }
  .scrubber:hover .track,
  .scrubber.dragging .track {
    background: var(--ink-faint);
  }
  .thumb {
    position: absolute;
    left: 2px;
    width: 10px;
    height: 28px;
    border-radius: 6px;
    background: var(--ink-soft);
    border: 1px solid var(--bg);
    pointer-events: none;
  }
  .scrubber:hover .thumb,
  .scrubber.dragging .thumb {
    background: var(--accent);
  }
  .bubble {
    position: absolute;
    right: 22px;
    transform: translateY(-50%);
    margin-top: 14px;
    padding: 6px 12px;
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    font-size: var(--t-sm);
    font-weight: 500;
    color: var(--ink);
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 8px 24px rgba(0,0,0,0.35);
  }
</style>
