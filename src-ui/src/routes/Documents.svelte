<script lang="ts">
  import { onMount } from "svelte";
  import { documents } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
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
    if (!q.trim()) { return load(); }
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

<main class="docs">
  <header>
    <h2>Documents</h2>
    <input bind:value={q} oninput={onInput} placeholder="Search OCR text…" />
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="grid">
    {#each items as p}
      <a class="cell" href="#/photo?id={p.id}">
        {#if p.thumbnail_path}
          <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
        {/if}
      </a>
    {/each}
  </div>
</main>

<style>
  .docs { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 16px; }
  h2 { margin: 0; }
  input { flex: 1; max-width: 400px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 6px; }
  .cell { aspect-ratio: 1; background: #131316; border-radius: 4px; overflow: hidden; }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
