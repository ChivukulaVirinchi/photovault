<script lang="ts">
  import { onMount } from "svelte";
  import { photos } from "../lib/api/photos";
  import { events, type ScanProgress } from "../lib/api/events";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { createVirtualScroll } from "../lib/virtualizer.svelte";
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

  const ROW_HEIGHT = 184;
  const LABEL_HEIGHT = 48;
  const COLS = 6;

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
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  function monthLabel(iso: string | null): string {
    if (!iso) return "Without date";
    const d = new Date(iso);
    return d.toLocaleString("en", { month: "long", year: "numeric" });
  }

  onMount(() => {
    loadMore();
    let unlistens: Array<() => void> = [];
    events
      .onScanProgress((p) => {
        if (p.job_id === scanJobId) scanProgress = p;
      })
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

  // Build rows: month-label rows interleaved with photo rows.
  type Row =
    | { kind: "label"; height: number; month: string }
    | { kind: "photos"; height: number; photos: PhotoSummaryDto[] };

  const rows = $derived.by((): Row[] => {
    const out: Row[] = [];
    let lastMonth = "";
    let bucket: PhotoSummaryDto[] = [];
    const flush = () => {
      while (bucket.length > 0) {
        out.push({ kind: "photos", height: ROW_HEIGHT, photos: bucket.slice(0, COLS) });
        bucket = bucket.slice(COLS);
      }
    };
    for (const p of items) {
      const m = monthLabel(p.date_taken);
      if (m !== lastMonth) {
        flush();
        out.push({ kind: "label", height: LABEL_HEIGHT, month: m });
        lastMonth = m;
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
</script>

<PageHeader title="Timeline">
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

<div class="scroll" bind:this={scrollEl}>
  <div class="inner" style="height: {v.totalHeight}px">
    {#each rows.slice(v.first, v.last) as row, idx (v.first + idx)}
      {@const i = v.first + idx}
      <div
        class="row"
        class:row-label={row.kind === "label"}
        style="transform: translateY({v.offsets[i]}px); height: {row.height}px;"
      >
        {#if row.kind === "label"}
          <span class="month">{row.month}</span>
        {:else}
          <div class="photos">
            {#each row.photos as photo (photo.id)}
              <a class="cell" href="#/photo?id={photo.id}" title="#{photo.id}">
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

<style>
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-4) var(--s-7) var(--s-7);
    contain: strict;
    height: 100%;
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
    padding-bottom: var(--s-2);
  }
  .month {
    font-size: var(--t-sm);
    font-weight: 600;
    color: var(--ink);
    letter-spacing: -0.005em;
  }
  .photos {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 4px;
    padding: 2px 0;
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
  .scan-status {
    color: var(--accent);
  }
</style>
