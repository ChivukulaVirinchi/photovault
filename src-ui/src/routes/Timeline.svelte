<script lang="ts">
  import { onMount } from "svelte";
  import { photos } from "../lib/api/photos";
  import { events, type ScanProgress } from "../lib/api/events";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { createVirtualScroll } from "../lib/virtualizer.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { PhotoSummaryDto } from "../lib/api/types";

  /// Zoom levels — Apple-Photos-style. `day` is default; `all` is the
  /// densest packed view with no headers.
  type ZoomLevel = "day" | "month" | "year" | "all";

  const TILE_PX: Record<ZoomLevel, number> = {
    day: 156, month: 112, year: 72, all: 36,
  };
  const GAP_PX: Record<ZoomLevel, number> = {
    day: 4, month: 3, year: 2, all: 1,
  };
  const LABEL_PX: Record<ZoomLevel, number> = {
    day: 36, month: 32, year: 28, all: 0,
  };
  const ZOOM_ORDER: ZoomLevel[] = ["all", "year", "month", "day"];

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

  let scanProgress = $state<ScanProgress | null>(null);
  let scanJobId = $state<string | null>(null);

  // Scrubber state
  let scrubHover = $state(false);
  let scrubDragging = $state(false);
  let scrubY = $state(0);                // mouse Y while hovering the track
  let scrollTop = $state(0);

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
    try { scanJobId = (await library.startScan(false)).job_id; }
    catch (e) { error = JSON.stringify(e); }
  }

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
      case "all":   return "all";
    }
  }
  function bucketLabel(iso: string | null, lvl: ZoomLevel): string {
    if (!iso) return "Without date";
    const d = new Date(iso);
    switch (lvl) {
      case "day":   return d.toLocaleString("en", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
      case "month": return d.toLocaleString("en", { month: "long", year: "numeric" });
      case "year":  return `${d.getFullYear()}`;
      case "all":   return "";
    }
  }

  // Build rows: optional label rows interleaved with photo rows.
  type Row =
    | { kind: "label"; height: number; label: string; firstIso: string | null }
    | { kind: "photos"; height: number; photos: PhotoSummaryDto[]; firstIso: string | null };

  const rows = $derived.by((): Row[] => {
    const out: Row[] = [];
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
    const onScroll = () => { scrollTop = scrollEl!.scrollTop; };
    scrollEl.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      ro.disconnect();
      scrollEl?.removeEventListener("scroll", onScroll);
    };
  });

  onMount(() => {
    loadMore();
    let unlistens: Array<() => void> = [];
    events
      .onScanProgress((p) => { if (p.job_id === scanJobId) scanProgress = p; })
      .then((u) => unlistens.push(u));
    events
      .onScanComplete((p) => {
        if (p.job_id === scanJobId) {
          scanProgress = p;
          scanJobId = null;
          items = [];
          nextCursor = null;
          hasMore = true;
          loadMore();
        }
      })
      .then((u) => unlistens.push(u));
    return () => unlistens.forEach((u) => u());
  });

  function setZoom(z: ZoomLevel) { zoom = z; }
  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1; // wheel up = zoom in (more detail)
    const i = ZOOM_ORDER.indexOf(zoom);
    const next = Math.max(0, Math.min(ZOOM_ORDER.length - 1, i + dir));
    zoom = ZOOM_ORDER[next];
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
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          requestIfMissing(photo, idx);
          io.disconnect();
          break;
        }
      }
    }, { root: scrollEl, rootMargin: "200px" });
    io.observe(node);
    return { destroy() { io.disconnect(); } };
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
    // Find the closest label row at or before `best` for label-bearing
    // zoom levels. For "all" (no labels), derive from the photo row's
    // firstIso.
    const lvl = zoom;
    if (LABEL_PX[lvl] === 0) {
      const r = rows[best];
      if (r) return bucketLabel(r.firstIso, lvl) || `Photo ${best + 1}`;
      return "";
    }
    for (let i = best; i >= 0; i--) {
      if (rows[i]?.kind === "label") {
        return (rows[i] as { kind: "label"; label: string }).label;
      }
    }
    return rows[0] ? bucketLabel(rows[0].firstIso, lvl) : "";
  }
  const currentBucket = $derived(bucketAtY(scrollTop));
  const hoveredBucket = $derived.by(() => {
    if (!scrubHover && !scrubDragging) return "";
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
    <button class:on={zoom === "all"}   onclick={() => setZoom("all")}   aria-label="All">All</button>
    <button class:on={zoom === "year"}  onclick={() => setZoom("year")}  aria-label="Years">Year</button>
    <button class:on={zoom === "month"} onclick={() => setZoom("month")} aria-label="Months">Month</button>
    <button class:on={zoom === "day"}   onclick={() => setZoom("day")}   aria-label="Days">Day</button>
  </div>
  {#if scanJobId}
    <span class="scan-status mono">
      Scanning · {(scanProgress?.files_processed ?? 0).toLocaleString()}
      / {scanProgress?.files_found?.toLocaleString() ?? "?"}
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
  <div class="scroll" bind:this={scrollEl} onwheel={onWheel}>
    <div class="inner" style="height: {v.totalHeight}px">
      {#each rows.slice(v.first, v.last) as row, idx (v.first + idx)}
        {@const i = v.first + idx}
        <div
          class="row"
          class:row-label={row.kind === "label"}
          style="transform: translateY({v.offsets[i]}px); height: {row.height}px;"
        >
          {#if row.kind === "label"}
            <span class="label">{row.label}</span>
          {:else}
            <div
              class="photos"
              style="grid-template-columns: repeat({cols}, 1fr); gap: {GAP_PX[zoom]}px;"
            >
              {#each row.photos as photo (photo.id)}
                <a
                  class="cell"
                  class:tiny={zoom === "all"}
                  href="#/photo?id={photo.id}"
                  title="#{photo.id}"
                  use:cellAttach={photo}
                >
                  {#if photo.thumbnail_path}
                    <img
                      src={thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) ?? ""}
                      alt=""
                      loading="lazy"
                      decoding="async"
                    />
                  {/if}
                </a>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>

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
    aria-valuetext={currentBucket}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={scrollableMax > 0 ? Math.round((scrollTop / scrollableMax) * 100) : 0}
    tabindex="-1"
  >
    <div class="track"></div>
    <div class="thumb" style="top: {thumbY}px"></div>
    {#if scrubHover || scrubDragging}
      <div class="bubble" style="top: {scrubY}px">{hoveredBucket}</div>
    {:else if currentBucket && scrollTop > 4}
      <div class="bubble static" style="top: {thumbY}px">{currentBucket}</div>
    {/if}
  </div>
</div>

<style>
  .timeline-host {
    flex: 1;
    position: relative;
    overflow: hidden;
    height: 100%;
  }

  .scroll {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    padding: var(--s-4) var(--s-7) var(--s-7);
    contain: strict;
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
  }
  .cell {
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    display: block;
    aspect-ratio: 1;
    position: relative;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .cell.tiny { border-radius: 1px; }
  .cell::after {
    content: "";
    position: absolute;
    inset: 0;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.03);
    border-radius: inherit;
    pointer-events: none;
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease);
  }
  .cell:hover {
    filter: brightness(1.06);
    box-shadow: 0 0 0 2px var(--accent-ghost);
    z-index: 1;
  }
  .cell:hover::after { opacity: 1; }
  .cell:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--accent);
    z-index: 1;
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
    padding: 5px 11px;
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    font-size: var(--t-xs);
    color: var(--ink);
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 8px 24px rgba(0,0,0,0.35);
  }
  .bubble.static {
    opacity: 0;
    animation: fadein 200ms forwards;
  }
  @keyframes fadein { to { opacity: 0.85; } }
</style>
