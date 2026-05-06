<script lang="ts">
  import { onMount } from "svelte";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import { photos } from "../lib/api/photos";
  import { events, type ScanProgress } from "../lib/api/events";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
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

  // Tile layout: a single virtualized vertical list of rows. Each row holds
  // ~6 photos. Real grid virtualization is M3 polish.
  const ROW_HEIGHT = 180;
  const COLS = 6;

  async function loadMore() {
    if (loading || !hasMore) return;
    loading = true;
    error = null;
    try {
      const page = await photos.list({ cursor: nextCursor, limit: 200 });
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
        // Reload first page to surface freshly-indexed photos.
        items = [];
        nextCursor = null;
        hasMore = true;
        loadMore();
      }
    }).then((u) => unlistens.push(u));
    return () => unlistens.forEach((u) => u());
  });

  // Build rows as [photo, photo, ...] groups of COLS for the virtualizer.
  const rows = $derived(
    Array.from({ length: Math.ceil(items.length / COLS) }, (_, i) =>
      items.slice(i * COLS, (i + 1) * COLS),
    ),
  );

  const virtualizer = $derived(
    scrollEl
      ? createVirtualizer<HTMLDivElement, HTMLDivElement>({
          count: rows.length,
          getScrollElement: () => scrollEl ?? null,
          estimateSize: () => ROW_HEIGHT,
          overscan: 5,
        })
      : null,
  );

  // Trigger load-more near the bottom.
  $effect(() => {
    if (!virtualizer || !$virtualizer) return;
    const v = $virtualizer;
    const last = v.getVirtualItems().at(-1);
    if (!last) return;
    if (last.index >= rows.length - 3 && hasMore && !loading) {
      loadMore();
    }
  });
</script>

<main class="timeline">
  <header>
    <h2>Timeline</h2>
    <div class="meta">
      <span class="muted">
        {libraryStore.driveRoot} — {total ?? items.length} photos
      </span>
      {#if scanJobId}
        <span class="scan-status">
          Scanning… {scanProgress?.files_processed ?? 0}
          /{scanProgress?.files_found ?? "?"}
        </span>
      {:else}
        <button onclick={startScan}>Scan now</button>
      {/if}
      <button onclick={() => libraryStore.close()}>Switch library</button>
    </div>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="scroll" bind:this={scrollEl}>
    {#if $virtualizer}
      <div class="inner" style="height: {$virtualizer.getTotalSize()}px;">
        {#each $virtualizer.getVirtualItems() as virtual (virtual.key)}
          {@const row = rows[virtual.index]}
          <div
            class="row"
            style="
              transform: translateY({virtual.start}px);
              height: {ROW_HEIGHT}px;
            "
          >
            {#each row as photo}
              <a
                class="cell"
                href="#/photo?id={photo.id}"
                title="#{photo.id}"
              >
                {#if photo.thumbnail_path}
                  <img
                    src={thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) ?? ""}
                    alt=""
                    loading="lazy"
                  />
                {:else}
                  <span class="muted small">no thumb</span>
                {/if}
              </a>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</main>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid #1f1f22;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .scan-status {
    color: #6aa9ff;
    font-size: 13px;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
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
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 4px;
    padding: 0 4px;
  }
  .cell {
    background: #131316;
    border-radius: 4px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    aspect-ratio: 1;
  }
  .cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .small {
    font-size: 11px;
  }
</style>
