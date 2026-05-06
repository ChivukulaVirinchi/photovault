<script lang="ts">
  import { onMount } from "svelte";
  import { search } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { SearchResults } from "../lib/api/all";

  let q = $state("");
  let results = $state<SearchResults | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let debounceId: number | undefined;
  let inputEl: HTMLInputElement | undefined;

  async function run() {
    if (!q.trim()) { results = null; return; }
    loading = true;
    try {
      results = await search.query(q.trim());
    } catch (e) { error = JSON.stringify(e); }
    finally { loading = false; }
  }

  function onInput() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = window.setTimeout(run, 250);
  }

  onMount(() => inputEl?.focus());
</script>

<PageHeader
  num="05"
  label="SEARCH"
  title="Find anything."
  subtitle='Try "Goa 2023", a person you’ve named, or a place you’ve been.'
/>

<div class="search-row">
  <div class="bar">
    <span class="prompt mono">▸</span>
    <input
      bind:this={inputEl}
      bind:value={q}
      oninput={onInput}
      placeholder="type to search…"
    />
    {#if loading}<span class="loading mono">…</span>{/if}
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if results}
    {#if results.people.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span>PEOPLE</span>
        <ul class="row">
          {#each results.people as p}
            <li>
              <a href="#/person?id={p.cluster_id}">
                {#if p.face_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, p.face_thumbnail_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder"></span>
                {/if}
                <strong>{p.name}</strong>
                <span class="muted small mono">{p.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.albums.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span>ALBUMS</span>
        <ul class="row">
          {#each results.albums as a}
            <li>
              <a href="#/album?id={a.album_id}">
                {#if a.cover_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder"></span>
                {/if}
                <strong>{a.name}</strong>
                <span class="muted small mono">{a.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.places.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span>PLACES</span>
        <ul class="places">
          {#each results.places as pl}
            <li>
              <span class="city">{pl.city}</span>
              {#if pl.country}<span class="country"><em>, {pl.country}</em></span>{/if}
              <span class="muted small mono">{pl.photo_count}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.photo_ids.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span>PHOTOS · {results.photo_ids.length}</span>
        <div class="grid stagger">
          {#each results.photo_ids.slice(0, 200) as pid, i}
            <a class="cell" href="#/photo?id={pid}" style="--i: {Math.min(i, 30)}">
              <span class="muted small mono">#{pid}</span>
            </a>
          {/each}
        </div>
      </section>
    {/if}
    {#if results.people.length === 0 && results.albums.length === 0 && results.places.length === 0 && results.photo_ids.length === 0}
      <div class="empty">
        <p class="quiet">Nothing matches that just yet.</p>
      </div>
    {/if}
  {:else if !loading}
    <div class="empty">
      <p class="quiet">Quietness lives here until you ask it something.</p>
    </div>
  {/if}
</div>

<style>
  .search-row { padding: var(--s-5) var(--s-7) 0; }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: var(--s-3) var(--s-4);
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .bar:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ghost);
  }
  .prompt { color: var(--accent); font-weight: 500; }
  .bar input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 0;
    font-size: var(--t-xl);
    font-family: var(--font-display);
    font-weight: 400;
    color: var(--ink);
  }
  .bar input:focus { outline: none; box-shadow: none; }
  .bar input::placeholder {
    color: var(--ink-faint);
    font-style: italic;
  }
  .loading { color: var(--ink-muted); }

  .page { padding: var(--s-5) var(--s-7); flex: 1; overflow-y: auto; }
  section { margin-bottom: var(--s-7); }
  section .eyebrow { margin-bottom: var(--s-3); display: inline-flex; }

  .row { list-style: none; padding: 0; margin: 0; display: flex; gap: var(--s-2); flex-wrap: wrap; }
  .row li { background: var(--bg-card); border-radius: var(--r-md); border: 1px solid var(--line); }
  .row a {
    display: flex; align-items: center; gap: var(--s-3);
    padding: var(--s-2) var(--s-3);
    color: inherit;
  }
  .row a:hover { text-decoration: none; background: var(--bg-elev); }
  .row img, .placeholder { width: 36px; height: 36px; border-radius: 50%; object-fit: cover; flex-shrink: 0; }
  .placeholder { background: var(--bg-elev); }

  .places { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: var(--s-1); }
  .places li {
    padding: var(--s-3) var(--s-4);
    background: var(--bg-card);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    display: flex; align-items: baseline; gap: var(--s-2);
  }
  .city { font-family: var(--font-display); font-size: var(--t-lg); font-weight: 500; }
  .country em { font-style: italic; color: var(--ink-muted); }
  .places .muted { margin-left: auto; }

  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 6px; }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    display: flex; align-items: center; justify-content: center;
    color: var(--ink-faint);
  }

  .empty {
    padding: var(--s-9);
    text-align: center;
  }
  .quiet {
    font-family: var(--font-display);
    font-style: italic;
    font-size: var(--t-xl);
    color: var(--ink-muted);
  }
  .small { font-size: var(--t-xs); }
</style>
