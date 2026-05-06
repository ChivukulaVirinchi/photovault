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

<main class="detail">
  <header>
    <a href="#/memories">← Memories</a>
    {#if card}
      <h2>{card.title}</h2>
      <span class="muted">{card.photo_count} photos</span>
      {#if savedAlbumId}
        <a href="#/album?id={savedAlbumId}">Saved as album →</a>
      {:else}
        <button onclick={saveAsAlbum}>Save as album</button>
      {/if}
    {/if}
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="grid">
    {#each photos as p}
      <a class="cell" href="#/photo?id={p.id}">
        {#if p.thumbnail_path}
          <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
        {/if}
      </a>
    {/each}
  </div>
</main>

<style>
  .detail { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 20px; flex-wrap: wrap; }
  h2 { margin: 0; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 6px; }
  .cell { aspect-ratio: 1; background: #131316; border-radius: 4px; overflow: hidden; }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
