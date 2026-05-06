<script lang="ts">
  import { onMount } from "svelte";
  import { memories } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";

  let cards = $state<Awaited<ReturnType<typeof memories.today>>>([]);
  let error = $state<string | null>(null);

  async function load() {
    try {
      cards = await memories.today();
    } catch (e) { error = JSON.stringify(e); }
  }

  onMount(load);
</script>

<main class="memories">
  <header>
    <h2>Memories</h2>
    <span class="muted">A look back at this day.</span>
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if cards.length === 0}
    <p class="muted">No memories surfacing today. Library may be too new (we wait until you have at least 3 months of photos).</p>
  {:else}
    <div class="cards">
      {#each cards as c}
        <a class="card" href="#/memory?id={c.id}">
          {#if c.hero_thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, c.hero_thumbnail_path) ?? ""} alt="" />
          {/if}
          <div class="caption">
            <strong>{c.title}</strong>
            <span class="muted small">{c.photo_count} photos</span>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</main>

<style>
  .memories { flex: 1; overflow-y: auto; padding: 20px; }
  header { margin-bottom: 20px; }
  h2 { margin: 0 0 4px; }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px;
  }
  .card {
    background: #131316;
    border-radius: 12px;
    overflow: hidden;
    color: inherit;
    aspect-ratio: 4 / 3;
    position: relative;
  }
  .card:hover { text-decoration: none; }
  .card img {
    width: 100%; height: 100%; object-fit: cover;
  }
  .caption {
    position: absolute;
    bottom: 0; left: 0; right: 0;
    padding: 16px;
    background: linear-gradient(transparent, rgba(0,0,0,0.85));
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .small { font-size: 12px; }
</style>
