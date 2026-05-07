<script lang="ts">
  import { onMount } from "svelte";
  import { photos } from "../lib/api/photos";
  import { memories, trash, type MemoryCard } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { createVirtualScroll } from "../lib/virtualizer.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import { Check } from "lucide-svelte";
  import type { PhotoSummaryDto } from "../lib/api/types";

  /// Zoom levels — Apple-Photos-style. `day` is default; `all` is the
  /// densest packed view with no headers.
  type ZoomLevel = "day" | "month" | "year";

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

  let items = $state<PhotoSummaryDto[]>([]);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(true);
  let total = $state<number | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let containerW = $state(0);
  let containerH = $state(0);
  let zoom = $state<ZoomLevel>("day");

  // Scan state derived from the global jobs store so it survives
  // page navigation. Earlier this lived in component-local $state and
  // reset on remount, making the scan look "lost" if the user moved
  // away and came back.
  const scanJob = $derived(jobs.byKind("scan"));
  const scanRunning = $derived(jobs.isRunning("scan"));

  // Today's memories — surfaced as a horizontal strip above the
  // timeline. Hidden when the library has no qualifying memories.
  let memoryCards = $state<MemoryCard[]>([]);

  // Scrubber state
  let scrubHover = $state(false);
  let scrubDragging = $state(false);
  let scrubY = $state(0);                // mouse Y while hovering the track
  let scrollTop = $state(0);

  // Multi-select dialog
  let showAddDialog = $state(false);

  // Index into `items` of the keyboard-focused cell. -1 = none.
  // Bumped on click so arrow nav picks up where the user left off; reset
  // when items reload from a fresh scan.
  let focusedIdx = $state(-1);

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

  async function loadMore() {
    if (loading || !hasMore) return;
    loading = true;
    error = null;
    try {
      const page = await photos.list({ cursor: nextCursor, limit: 240 });
      const fresh = page.items.map((p) => p.id);
      items = items.concat(page.items);
      if (nextCursor === null) browseContext.set("timeline", fresh);
      else                     browseContext.extend(fresh);
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      if (page.total !== null) total = page.total;
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
  // When a scan completes, refresh the photo list so the user sees
  // the new arrivals without having to reload manually.
  let lastScanCompleteId = $state<string | null>(null);
  $effect(() => {
    if (scanJob && scanJob.status === "complete" && scanJob.id !== lastScanCompleteId) {
      lastScanCompleteId = scanJob.id;
      items = [];
      nextCursor = null;
      hasMore = true;
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
    overscan: 6,
  });

  $effect(() => {
    return v.attach();
  });

  $effect(() => {
    if (v.last >= rows.length - 4 && hasMore && !loading) {
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
      // Persist so that returning from PhotoDetail lands the user back
      // at the same row instead of the top of the timeline.
      try { sessionStorage.setItem("scroll:/timeline", String(scrollTop)); } catch {}
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
    const raw = (() => { try { return sessionStorage.getItem("scroll:/timeline"); } catch { return null; } })();
    const target = raw ? parseInt(raw, 10) : 0;
    if (!Number.isFinite(target) || target <= 0) {
      scrollRestored = true;
      return;
    }
    if (v.totalHeight >= target + 16) {
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
    loadMore();
    memories.today().then((c) => (memoryCards = c)).catch(() => {});
    // Scan progress + the post-scan reload now happen via the global
    // jobs store and the $effect above. No local listeners needed.
    window.addEventListener("keydown", onGlobalKey);
    return () => {
      window.removeEventListener("keydown", onGlobalKey);
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
  function scrollFocusedIntoView() {
    if (!scrollEl || focusedIdx < 0) return;
    // Compute approximate Y of the focused cell. The row index in the
    // photo bucket plus any leading label rows isn't trivially known,
    // so we read the DOM after reactivity settles. rAF lets the row
    // render before we measure.
    requestAnimationFrame(() => {
      const id = items[focusedIdx]?.id;
      if (id == null || !scrollEl) return;
      const cell = scrollEl.querySelector<HTMLAnchorElement>(`a.cell[href="#/photo?id=${id}"]`);
      if (!cell) {
        // Cell isn't rendered yet (virtualized out). Estimate offset from
        // total cells before it and scroll there; the next frame's render
        // will bring it into view.
        const rowIdx = Math.floor(focusedIdx / Math.max(1, cols));
        const targetY = rowIdx * (rowH + GAP_PX[zoom]);
        scrollEl.scrollTo({ top: Math.max(0, targetY - containerH / 2), behavior: "auto" });
        return;
      }
      const cr = cell.getBoundingClientRect();
      const sr = scrollEl.getBoundingClientRect();
      if (cr.top < sr.top + 8) {
        scrollEl.scrollBy({ top: cr.top - sr.top - 8, behavior: "smooth" });
      } else if (cr.bottom > sr.bottom - 8) {
        scrollEl.scrollBy({ top: cr.bottom - sr.bottom + 8, behavior: "smooth" });
      }
    });
  }
  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1; // wheel up = zoom in (more detail)
    const i = ZOOM_ORDER.indexOf(zoom);
    const next = Math.max(0, Math.min(ZOOM_ORDER.length - 1, i + dir));
    setZoom(ZOOM_ORDER[next]);
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
  const scrollableMax = $derived(Math.max(0, v.totalHeight - containerH));
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
  {/if}
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

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
    <div class="inner" style="height: {v.totalHeight}px">
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
                  use:cellAttach={photo}
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
