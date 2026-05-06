<script lang="ts">
  import { onMount } from "svelte";
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
    try {
      album = await albums.rename(id, editName.trim());
      renaming = false;
    } catch (e) { error = JSON.stringify(e); }
  }

  async function deleteAlbum() {
    if (!confirm("Delete album? Photos will not be trashed.")) return;
    try {
      await albums.delete(id);
      window.location.hash = "/albums";
    } catch (e) { error = JSON.stringify(e); }
  }

  $effect(() => { void id; load(); });
</script>

<main class="detail">
  <header>
    <a href="#/albums">← Albums</a>
    {#if album}
      {#if renaming}
        <input bind:value={editName} />
        <button onclick={rename}>Save</button>
        <button class="ghost" onclick={() => (renaming = false)}>Cancel</button>
      {:else}
        <h2>{album.name}</h2>
        <button onclick={() => (renaming = true)}>Rename</button>
        <button class="danger" onclick={deleteAlbum}>Delete</button>
      {/if}
      <span class="muted">{album.photo_count} photos</span>
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
  input { max-width: 300px; }
  .ghost { background: transparent; border: 1px solid #2a2a2d; }
  .danger { background: #2a1414; border: 1px solid #4a2222; color: #f87171; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    background: #131316;
    border-radius: 4px;
    overflow: hidden;
  }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
