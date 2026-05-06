<script lang="ts">
  import { onMount } from "svelte";
  import { people } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import type { PersonDto } from "../lib/api/types";

  let clusters = $state<PersonDto[]>([]);
  let processing = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try {
      clusters = await people.list({ minPhotos: 2 });
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  async function startFaceProcessing() {
    processing = true;
    try {
      await people.startProcessing();
    } catch (e) {
      error = JSON.stringify(e);
      processing = false;
    }
  }

  onMount(load);
</script>

<main class="people">
  <header>
    <h2>People</h2>
    <button onclick={startFaceProcessing} disabled={processing}>
      {processing ? "Processing…" : "Find faces"}
    </button>
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if clusters.length === 0}
    <p class="muted">No face clusters yet. Run face processing.</p>
  {:else}
    <div class="grid">
      {#each clusters as c}
        <a class="card" href="#/person?id={c.id}">
          {#if c.representative_thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""} alt="" />
          {:else}
            <span class="muted small">no face</span>
          {/if}
          <div class="meta">
            <strong>{c.name ?? "Unnamed"}</strong>
            <span class="muted small">{c.photo_count} photos</span>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</main>

<style>
  .people { padding: 20px; flex: 1; overflow-y: auto; }
  header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; }
  h2 { margin: 0; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 16px;
  }
  .card {
    background: #131316;
    border-radius: 8px;
    overflow: hidden;
    color: inherit;
    transition: transform 80ms;
  }
  .card:hover { transform: translateY(-2px); text-decoration: none; }
  .card img {
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    display: block;
  }
  .meta { padding: 10px 12px; display: flex; flex-direction: column; gap: 2px; }
  .small { font-size: 12px; }
</style>
