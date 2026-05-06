<script lang="ts">
  import { onMount } from "svelte";
  import { albums } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import type { AlbumDto } from "../lib/api/types";

  let list = $state<AlbumDto[]>([]);
  let suggestions = $state<unknown[]>([]);
  let creating = $state(false);
  let newName = $state("");
  let error = $state<string | null>(null);

  async function load() {
    try {
      list = await albums.list();
      suggestions = await albums.suggestions.list();
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  async function createAlbum() {
    if (!newName.trim()) return;
    try {
      await albums.create(newName.trim());
      newName = "";
      creating = false;
      await load();
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  async function runDetection() {
    try {
      await albums.suggestions.runDetection();
      await load();
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  onMount(load);
</script>

<main class="albums">
  <header>
    <h2>Albums</h2>
    <div class="actions">
      <button onclick={runDetection}>Detect suggestions</button>
      {#if creating}
        <input bind:value={newName} placeholder="Album name" />
        <button onclick={createAlbum}>Create</button>
        <button class="ghost" onclick={() => (creating = false)}>Cancel</button>
      {:else}
        <button onclick={() => (creating = true)}>New album</button>
      {/if}
    </div>
  </header>
  {#if error}<p class="error">{error}</p>{/if}

  {#if suggestions.length > 0}
    <section>
      <h3 class="muted">Suggested ({suggestions.length})</h3>
      <p class="muted small">Suggestions detected from your trips and events. Visit each to accept.</p>
    </section>
  {/if}

  <div class="grid">
    {#each list as a}
      <a class="card" href="#/album?id={a.id}">
        {#if a.cover_thumbnail_path}
          <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
        {:else}
          <div class="empty muted small">no cover</div>
        {/if}
        <div class="meta">
          <strong>{a.name}</strong>
          <span class="muted small">{a.photo_count} photos</span>
        </div>
      </a>
    {/each}
  </div>
</main>

<style>
  .albums { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; gap: 12px; }
  h2 { margin: 0; }
  .actions { display: flex; gap: 8px; }
  .ghost { background: transparent; border: 1px solid #2a2a2d; }
  section { margin-bottom: 24px; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 16px;
  }
  .card {
    background: #131316;
    border-radius: 10px;
    overflow: hidden;
    color: inherit;
  }
  .card:hover { text-decoration: none; }
  .card img, .empty {
    width: 100%;
    aspect-ratio: 4 / 3;
    object-fit: cover;
    display: block;
  }
  .empty { display: flex; align-items: center; justify-content: center; }
  .meta { padding: 12px 14px; display: flex; flex-direction: column; gap: 2px; }
  .small { font-size: 12px; }
</style>
