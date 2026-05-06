<script lang="ts">
  import { onMount } from "svelte";
  import { documents } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { PhotoSummaryDto } from "../lib/api/types";

  let items = $state<PhotoSummaryDto[]>([]);
  let q = $state("");
  let error = $state<string | null>(null);
  let debounceId: number | undefined;

  async function load() {
    try {
      const page = await documents.list(null, null, 200);
      items = page.items;
      browseContext.set("documents", items.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
  }

  async function searchNow() {
    if (!q.trim()) return load();
    try {
      const page = await documents.search(q.trim());
      items = page.items;
      browseContext.set("documents:search", items.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
  }

  function onInput() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = window.setTimeout(searchNow, 250);
  }

  onMount(load);
</script>

<PageHeader title="Documents">
  <span class="count mono">{items.length}<span class="muted"> items</span></span>
  <input
    bind:value={q}
    oninput={onInput}
    placeholder="Search OCR text…"
    class="search"
  />
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if items.length === 0}
    <div class="empty">
      <p>No documents detected yet. Document analysis runs as part of the indexing pipeline.</p>
    </div>
  {:else}
    <div class="grid">
      {#each items as p (p.id)}
        <a class="cell" href="#/photo?id={p.id}">
          {#if p.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
          {/if}
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-4) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .count { font-size: var(--t-sm); color: var(--ink); }
  .search {
    width: 240px;
    padding: 6px var(--s-3);
    font-size: var(--t-sm);
  }
  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.55; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 4px;
  }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .cell:hover {
    filter: brightness(1.06);
    box-shadow: 0 0 0 2px var(--accent-ghost);
    z-index: 1;
  }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
