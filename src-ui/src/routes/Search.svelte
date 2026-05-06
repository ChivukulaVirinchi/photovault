<script lang="ts">
  import { search } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import type { SearchResults } from "../lib/api/all";

  let q = $state("");
  let results = $state<SearchResults | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let debounceId: number | undefined;

  async function run() {
    if (!q.trim()) { results = null; return; }
    loading = true;
    try {
      results = await search.query(q.trim());
    } catch (e) {
      error = JSON.stringify(e);
    } finally {
      loading = false;
    }
  }

  function onInput() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = window.setTimeout(run, 250);
  }
</script>

<main class="search">
  <header>
    <h2>Search</h2>
    <input
      bind:value={q}
      oninput={onInput}
      placeholder='Try: "Goa 2023", "Mom", or "beach"'
      autofocus
    />
  </header>
  {#if loading}<p class="muted">Searching…</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if results}
    {#if results.people.length > 0}
      <section>
        <h3>People</h3>
        <ul class="row">
          {#each results.people as p}
            <li>
              <a href="#/person?id={p.cluster_id}">
                {#if p.face_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, p.face_thumbnail_path) ?? ""} alt="" />
                {/if}
                <strong>{p.name}</strong>
                <span class="muted small">{p.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.albums.length > 0}
      <section>
        <h3>Albums</h3>
        <ul class="row">
          {#each results.albums as a}
            <li>
              <a href="#/album?id={a.album_id}">
                {#if a.cover_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
                {/if}
                <strong>{a.name}</strong>
                <span class="muted small">{a.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.places.length > 0}
      <section>
        <h3>Places</h3>
        <ul class="row plain">
          {#each results.places as pl}
            <li>{pl.city}{pl.country ? `, ${pl.country}` : ""} <span class="muted small">{pl.photo_count}</span></li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.photo_ids.length > 0}
      <section>
        <h3>Photos ({results.photo_ids.length})</h3>
        <div class="grid">
          {#each results.photo_ids.slice(0, 200) as pid}
            <a class="cell" href="#/photo?id={pid}"><span class="muted small">#{pid}</span></a>
          {/each}
        </div>
      </section>
    {/if}
    {#if results.people.length === 0 && results.albums.length === 0 && results.places.length === 0 && results.photo_ids.length === 0}
      <p class="muted">No results.</p>
    {/if}
  {/if}
</main>

<style>
  .search { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 20px; }
  h2 { margin: 0; }
  input { flex: 1; max-width: 600px; padding: 10px 14px; font-size: 16px; }
  section { margin-top: 28px; }
  h3 { margin: 0 0 10px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.06em; color: #a8a8af; }
  .row { list-style: none; padding: 0; margin: 0; display: flex; gap: 8px; flex-wrap: wrap; }
  .row li { background: #131316; border-radius: 8px; }
  .row a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    color: inherit;
  }
  .row a:hover { text-decoration: none; background: #1a1a1f; }
  .row img { width: 32px; height: 32px; border-radius: 50%; object-fit: cover; }
  .row.plain li { padding: 8px 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 4px; }
  .cell {
    aspect-ratio: 1;
    background: #131316;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .small { font-size: 11px; }
</style>
