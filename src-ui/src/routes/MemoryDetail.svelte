<script lang="ts">
  import { memories } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import type { PhotoSummaryDto } from "../lib/api/types";
  import type { MemoryCard } from "../lib/api/all";

  interface Props { id: string }
  let { id }: Props = $props();

  let card = $state<MemoryCard | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let savedAlbumId = $state<number | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try {
      const r = await memories.detail(id);
      card = r.card;
      photos = r.photos;
    } catch (e) { error = JSON.stringify(e); }
  }

  async function saveAsAlbum() {
    if (!card) return;
    const a = await memories.saveAsAlbum(card.id);
    savedAlbumId = a.id;
  }

  $effect(() => { void id; load(); });
</script>

<div class="masthead">
  <a class="back" href="#/memories">← Memories</a>
  {#if card}
    <span class="eyebrow"><span class="num">{card.kind.toUpperCase()}</span><span class="ornament"></span></span>
    <h1>{card.title}</h1>
    <p class="subtitle">{card.photo_count} photographs</p>
    {#if savedAlbumId}
      <a class="saved" href="#/album?id={savedAlbumId}">Saved as album →</a>
    {:else}
      <button class="primary" onclick={saveAsAlbum}>Save as album</button>
    {/if}
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

<div class="grid stagger">
  {#each photos as p, i (p.id)}
    <a class="cell" href="#/photo?id={p.id}" style="--i: {Math.min(i, 30)}">
      {#if p.thumbnail_path}
        <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
      {/if}
    </a>
  {/each}
</div>

<style>
  .masthead {
    padding: var(--s-7) var(--s-7) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: flex-start;
  }
  .back {
    font-family: var(--font-mono);
    font-size: var(--t-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-muted);
  }
  h1 { font-size: var(--t-3xl); }
  .saved {
    font-family: var(--font-display);
    font-style: italic;
    font-size: var(--t-base);
    color: var(--accent);
  }

  .grid {
    padding: var(--s-5) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 6px;
  }
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
