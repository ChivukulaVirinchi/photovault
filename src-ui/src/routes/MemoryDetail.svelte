<script lang="ts">
  import { memories } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
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

{#if card}
  {@const c = card}
  <DetailHeader backHref="#/memories" backLabel="Memories">
    {#snippet title()}
      <h1>{c.title}</h1>
    {/snippet}
    {#snippet subtitle()}
      <span class="mono">{c.photo_count} photos</span>
      <span class="kind">{c.kind}</span>
    {/snippet}
    {#snippet actions()}
      {#if savedAlbumId}
        <a class="saved-link" href="#/album?id={savedAlbumId}">Saved as album →</a>
      {:else}
        <button class="primary" onclick={saveAsAlbum}>Save as album</button>
      {/if}
    {/snippet}
  </DetailHeader>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="grid">
  {#each photos as p (p.id)}
    <a class="cell" href="#/photo?id={p.id}">
      {#if p.thumbnail_path}
        <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
      {/if}
    </a>
  {/each}
</div>

<style>
  .kind {
    text-transform: lowercase;
    color: var(--ink-faint);
  }
  .saved-link {
    font-size: var(--t-sm);
    color: var(--accent);
    text-decoration: none;
    border-bottom: 1px solid var(--accent-soft);
    padding-bottom: 2px;
  }
  .saved-link:hover { border-bottom-color: var(--accent); }

  .grid {
    padding: var(--s-4) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
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
