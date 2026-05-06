<script lang="ts">
  import { albums } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import type { AlbumDto, PhotoSummaryDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let album = $state<AlbumDto | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let renaming = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);

  async function load() {
    try {
      album = await albums.get(id);
      editName = album.name;
      const page = await albums.photos(id);
      photos = page.items;
    } catch (e) { error = JSON.stringify(e); }
  }

  async function rename() {
    if (!album) return;
    try { album = await albums.rename(id, editName.trim()); renaming = false; }
    catch (e) { error = JSON.stringify(e); }
  }

  async function deleteAlbum() {
    if (!confirm("Delete album? Photos will not be trashed.")) return;
    try { await albums.delete(id); window.location.hash = "/albums"; }
    catch (e) { error = JSON.stringify(e); }
  }

  $effect(() => { void id; load(); });
</script>

<div class="masthead">
  <a class="back" href="#/albums">← Albums</a>
  {#if album}
    <span class="eyebrow">
      <span class="num">№&nbsp;{String(album.id).padStart(3, "0")}</span>
      <span class="ornament"></span>
      <span>ALBUM</span>
    </span>
    {#if renaming}
      <input bind:value={editName} autofocus />
      <div class="row">
        <button class="primary" onclick={rename}>Save</button>
        <button class="ghost" onclick={() => (renaming = false)}>Cancel</button>
      </div>
    {:else}
      <h1>{album.name}</h1>
      <p class="subtitle">
        {album.photo_count} photos
        {#if album.date_range_start && album.date_range_end}
          · <span class="mono">{new Date(album.date_range_start).toLocaleDateString()}
          → {new Date(album.date_range_end).toLocaleDateString()}</span>
        {/if}
      </p>
      <div class="row">
        <button class="ghost" onclick={() => (renaming = true)}>Rename</button>
        <button class="danger" onclick={deleteAlbum}>Delete album</button>
      </div>
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
  input { max-width: 360px; font-size: var(--t-xl); font-family: var(--font-display); }
  .row { display: flex; gap: var(--s-2); }

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
