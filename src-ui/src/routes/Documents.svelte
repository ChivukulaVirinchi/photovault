<script lang="ts">
  import { onMount } from "svelte";
  import { documents } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
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
    } catch (e) { error = JSON.stringify(e); }
  }

  async function searchNow() {
    if (!q.trim()) return load();
    try {
      const page = await documents.search(q.trim());
      items = page.items;
    } catch (e) { error = JSON.stringify(e); }
  }

  function onInput() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = window.setTimeout(searchNow, 250);
  }

  onMount(load);
</script>

<PageHeader
  num="09"
  label="DOCUMENTS"
  title="Receipts, screenshots, business cards."
  subtitle="Photos that look more like documents than memories. Search the text inside them."
>
  <input
    bind:value={q}
    oninput={onInput}
    placeholder="Search OCR text…"
  />
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if items.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NONE FOUND</span>
      <p class="quiet">No documents detected yet. Document analysis runs as part of the indexing pipeline.</p>
    </div>
  {:else}
    <div class="grid stagger">
      {#each items as p, i (p.id)}
        <a class="cell" href="#/photo?id={p.id}" style="--i: {Math.min(i, 30)}">
          {#if p.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
          {/if}
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7); flex: 1; overflow-y: auto; }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
    display: flex; flex-direction: column; gap: var(--s-3); align-items: center;
  }
  .quiet {
    font-family: var(--font-display);
    font-style: italic;
    font-size: var(--t-lg);
    color: var(--ink-soft);
    max-width: 42ch;
  }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 6px; }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    transition: transform var(--t-fast) var(--ease);
  }
  .cell:hover { transform: scale(1.018); }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
