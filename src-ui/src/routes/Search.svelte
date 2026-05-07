<script lang="ts">
  import { onMount } from "svelte";
  import { search } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { SearchResults } from "../lib/api/all";

  interface Props {
    /// Pre-fill the search bar from the URL (e.g. #/search?q=Goa).
    /// Lets Insights / Map / etc. deep-link into a filtered list.
    initialQuery?: string;
  }
  let { initialQuery = "" }: Props = $props();

  // svelte-ignore state_referenced_locally
  let q = $state(initialQuery);
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
      if (results) browseContext.set(`search:${q.trim()}`, results.photo_ids);
    } catch (e) { error = JSON.stringify(e); }
    finally { loading = false; }
  }

  function onInput() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = window.setTimeout(run, 250);
  }

  onMount(() => {
    inputEl?.focus();
    if (q.trim()) run();
  });
</script>

<PageHeader title="Search" />

<div class="search-row">
  <div class="bar">
    <svg class="lookup" width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <circle cx="6" cy="6" r="4" stroke="currentColor" stroke-width="1.4"/>
      <path d="M9.2 9.2L12 12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
    </svg>
    <input
      bind:this={inputEl}
      bind:value={q}
      oninput={onInput}
      placeholder='Try a name, place, or "Goa 2023"…'
    />
    {#if loading}<span class="loading mono">…</span>{/if}
  </div>
</div>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if results}
    {#if results.people.length > 0}
      <section>
        <h3 class="section-title">People</h3>
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
        <h3 class="section-title">Albums</h3>
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
        <h3 class="section-title">Places</h3>
        <ul class="places">
          {#each results.places as pl}
            <li>
              <a class="place-link" href={`#/search?q=${encodeURIComponent(pl.city ?? pl.country ?? "")}`}>
                <span class="city">{pl.city}</span>
                {#if pl.country}<span class="country">, {pl.country}</span>{/if}
                <span class="muted small mono">{pl.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.photos.length > 0}
      <section>
        <h3 class="section-title">Photos · {results.photos.length}</h3>
        <div class="pv-photo-grid">
          {#each results.photos.slice(0, 200) as p (p.photo_id)}
            <a class="pv-photo-cell" href="#/photo?id={p.photo_id}" title="#{p.photo_id}">
              {#if p.thumbnail_path}
                <img
                  src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""}
                  alt=""
                  loading="lazy"
                />
              {/if}
            </a>
          {/each}
        </div>
      </section>
    {/if}
    {#if results.people.length === 0 && results.albums.length === 0 && results.places.length === 0 && results.photos.length === 0}
      <div class="empty">
        <p>Nothing matches that yet.</p>
      </div>
    {/if}
  {:else if !loading}
    <div class="empty">
      <p>Type to search across people, albums, places, and OCR text.</p>
    </div>
  {/if}
</div>

<style>
  .search-row { padding: var(--s-4) var(--s-7) 0; }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: var(--s-2) var(--s-4);
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .bar:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ghost);
  }
  .lookup { color: var(--ink-muted); flex-shrink: 0; }
  .bar input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 0;
    font-size: var(--t-lg);
    color: var(--ink);
  }
  .bar input:focus { outline: none; box-shadow: none; }
  .bar input::placeholder {
    color: var(--ink-faint);
  }
  .loading { color: var(--ink-muted); }

  .page { padding: var(--s-5) var(--s-7); flex: 1; overflow-y: auto; }
  section { margin-bottom: var(--s-6); }
  .section-title {
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0 0 var(--s-3);
  }

  .row {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .row li {
    background: var(--bg-paper);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    transition: border-color var(--t-fast) var(--ease);
  }
  .row li:hover { border-color: var(--accent); }
  .row a {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 6px var(--s-3) 6px 6px;
    color: inherit;
    text-decoration: none;
  }
  .row img, .placeholder {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .placeholder { background: var(--bg-elev); }

  .places {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .places li { padding: 0; }
  .place-link {
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-paper);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    color: inherit;
    text-decoration: none;
    transition: border-color var(--t-fast) var(--ease),
                background var(--t-fast) var(--ease);
  }
  .place-link:hover {
    background: var(--bg-card);
    border-color: var(--accent);
  }
  .city { font-size: var(--t-base); font-weight: 600; }
  .country { color: var(--ink-muted); }
  .places .muted { margin-left: auto; }

  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
  }
  .empty p {
    color: var(--ink-muted);
    font-style: italic;
  }
  .small { font-size: var(--t-xs); }
</style>
