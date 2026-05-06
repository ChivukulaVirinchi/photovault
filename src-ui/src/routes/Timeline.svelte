<script lang="ts">
  import { onMount } from "svelte";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import { photos } from "../lib/api/photos";
  import { events, type ScanProgress } from "../lib/api/events";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { PhotoSummaryDto } from "../lib/api/types";

  let items = $state<PhotoSummaryDto[]>([]);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(true);
  let total = $state<number | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let scanProgress = $state<ScanProgress | null>(null);
  let scanJobId = $state<string | null>(null);

  const ROW_HEIGHT = 196;
  const COLS = 6;
  const GAP = 6;

  async function loadMore() {
    if (loading || !hasMore) return;
    loading = true;
    error = null;
    try {
      const page = await photos.list({ cursor: nextCursor, limit: 240 });
      items = items.concat(page.items);
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
    try {
      const r = await library.startScan(false);
      scanJobId = r.job_id;
    } catch (e) { error = JSON.stringify(e); }
  }

  // Group photos by month for editorial section labels.
  function monthLabel(iso: string | null): string {
    if (!iso) return "Without date";
    const d = new Date(iso);
    return d.toLocaleString("en", { month: "long", year: "numeric" });
  }

  onMount(() => {
    loadMore();
    let unlistens: Array<() => void> = [];
    events.onScanProgress((p) => {
      if (p.job_id === scanJobId) scanProgress = p;
    }).then((u) => unlistens.push(u));
    events.onScanComplete((p) => {
      if (p.job_id === scanJobId) {
        scanProgress = p;
        scanJobId = null;
        items = []; nextCursor = null; hasMore = true;
        loadMore();
      }
    }).then((u) => unlistens.push(u));
    return () => unlistens.forEach((u) => u());
  });

  // Build rows of [photo, photo, ...] groups of COLS for the virtualizer.
  // Inject month-header rows when month changes.
  type Row =
    | { kind: "label"; month: string }
    | { kind: "photos"; photos: PhotoSummaryDto[] };
  const rows = $derived.by((): Row[] => {
    const out: Row[] = [];
    let lastMonth = "";
    let bucket: PhotoSummaryDto[] = [];
    const flush = () => {
      while (bucket.length > 0) {
        out.push({ kind: "photos", photos: bucket.slice(0, COLS) });
        bucket = bucket.slice(COLS);
      }
    };
    for (const p of items) {
      const m = monthLabel(p.date_taken);
      if (m !== lastMonth) {
        flush();
        out.push({ kind: "label", month: m });
        lastMonth = m;
      }
      bucket.push(p);
    }
    flush();
    return out;
  });

  const virtualizer = $derived(
    scrollEl
      ? createVirtualizer<HTMLDivElement, HTMLDivElement>({
          count: rows.length,
          getScrollElement: () => scrollEl ?? null,
          estimateSize: (i) => (rows[i]?.kind === "label" ? 64 : ROW_HEIGHT),
          overscan: 6,
        })
      : null,
  );

  $effect(() => {
    if (!virtualizer || !$virtualizer) return;
    const v = $virtualizer;
    const last = v.getVirtualItems().at(-1);
    if (!last) return;
    if (last.index >= rows.length - 4 && hasMore && !loading) loadMore();
  });
</script>

<PageHeader
  num="01"
  label="TIMELINE"
  title="Your photographs"
  subtitle="Newest first. Organised the way you took them — month by month, day by day."
>
  {#if scanJobId}
    <span class="scan-status mono">
      Scanning · {(scanProgress?.files_processed ?? 0).toLocaleString()}
      / {scanProgress?.files_found?.toLocaleString() ?? "?"}
    </span>
  {:else}
    <span class="count mono">
      {(total ?? items.length).toLocaleString()} <span class="muted">photos</span>
    </span>
    <button class="primary" onclick={startScan}>Scan now</button>
  {/if}
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="scroll" bind:this={scrollEl}>
  {#if $virtualizer}
    <div class="inner" style="height: {$virtualizer.getTotalSize()}px;">
      {#each $virtualizer.getVirtualItems() as virtual (virtual.key)}
        {@const row = rows[virtual.index]}
        <div
          class="row"
          class:row-label={row?.kind === "label"}
          style="
            transform: translateY({virtual.start}px);
            height: {row?.kind === 'label' ? 64 : ROW_HEIGHT}px;
          "
        >
          {#if row?.kind === "label"}
            <div class="month-label">
              <span class="eyebrow">
                <span class="ornament"></span>
                <span>{row.month}</span>
              </span>
            </div>
          {:else if row?.kind === "photos"}
            <div class="photos">
              {#each row.photos as photo, i}
                <a
                  class="cell"
                  href="#/photo?id={photo.id}"
                  style="--i: {i}"
                  title="#{photo.id}"
                >
                  {#if photo.thumbnail_path}
                    <img
                      src={thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) ?? ""}
                      alt=""
                      loading="lazy"
                    />
                  {:else}
                    <span class="muted small mono">no thumb</span>
                  {/if}
                </a>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-3) var(--s-7) var(--s-7);
  }
  .inner {
    position: relative;
    width: 100%;
  }
  .row {
    position: absolute;
    top: 0; left: 0; right: 0;
    padding: 0;
  }
  .row.row-label {
    display: flex;
    align-items: flex-end;
    padding-bottom: var(--s-3);
    padding-top: var(--s-4);
  }
  .month-label .eyebrow {
    color: var(--ink-soft);
  }
  .photos {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 6px;
    padding: 3px 0;
  }
  .cell {
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    display: block;
    aspect-ratio: 1;
    position: relative;
    transition: transform var(--t-fast) var(--ease),
                box-shadow var(--t-base-d) var(--ease);
  }
  .cell:hover {
    transform: scale(1.018);
    box-shadow: var(--shadow-lift);
    z-index: 1;
  }
  .cell::after {
    content: "";
    position: absolute; inset: 0;
    box-shadow: inset 0 0 0 1px rgba(255,255,255,0.04);
    border-radius: inherit;
    pointer-events: none;
  }
  .cell img {
    width: 100%; height: 100%;
    object-fit: cover;
    display: block;
  }
  .scan-status, .count {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .scan-status { color: var(--accent); }
  .small { font-size: var(--t-xs); }
</style>
