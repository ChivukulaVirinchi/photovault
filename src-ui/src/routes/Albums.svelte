<script lang="ts">
  import { onMount } from "svelte";
  import { albums } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { AlbumDto, AlbumSuggestionDto } from "../lib/api/types";

  let list = $state<AlbumDto[]>([]);
  let suggestions = $state<AlbumSuggestionDto[]>([]);
  let creating = $state(false);
  let newName = $state("");
  let error = $state<string | null>(null);

  async function load() {
    try {
      list = await albums.list();
      suggestions = await albums.suggestions.list();
    } catch (e) { error = JSON.stringify(e); }
  }

  async function createAlbum() {
    if (!newName.trim()) return;
    try {
      await albums.create(newName.trim());
      newName = "";
      creating = false;
      await load();
    } catch (e) { error = JSON.stringify(e); }
  }

  async function runDetection() {
    try { await albums.suggestions.runDetection(); await load(); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function acceptSuggestion(id: number) {
    try { await albums.suggestions.accept(id); await load(); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function dismissSuggestion(id: number) {
    try { await albums.suggestions.dismiss(id); await load(); }
    catch (e) { error = JSON.stringify(e); }
  }

  onMount(load);
</script>

<PageHeader
  num="03"
  label="ALBUMS"
  title="Your collections."
  subtitle="Handpicked or auto-found from trips and events."
>
  <button class="ghost" onclick={runDetection}>Detect suggestions</button>
  {#if creating}
    <input bind:value={newName} placeholder="Album name" autofocus />
    <button class="primary" onclick={createAlbum}>Create</button>
    <button class="ghost" onclick={() => (creating = false)}>Cancel</button>
  {:else}
    <button class="primary" onclick={() => (creating = true)}>New album</button>
  {/if}
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if suggestions.length > 0}
    <section class="suggestions">
      <span class="eyebrow"><span class="ornament"></span>SUGGESTIONS · {suggestions.length}</span>
      <p class="muted">Patterns we noticed — accept to save, dismiss to ignore.</p>
      <div class="suggest-grid stagger">
        {#each suggestions as s, i (s.id)}
          <article class="suggestion" style="--i: {i}">
            {#if s.cover_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, s.cover_thumbnail_path) ?? ""} alt="" />
            {/if}
            <div class="body">
              <span class="kind mono">{s.kind.toUpperCase()}</span>
              <strong class="title">{s.title}</strong>
              <span class="muted small">{s.photo_ids.length} photos</span>
              <div class="row">
                <button class="primary" onclick={() => acceptSuggestion(s.id)}>Accept</button>
                <button class="ghost" onclick={() => dismissSuggestion(s.id)}>Dismiss</button>
              </div>
            </div>
          </article>
        {/each}
      </div>
    </section>
    <hr class="rule" />
  {/if}

  {#if list.length === 0 && suggestions.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NO ALBUMS YET</span>
      <p class="quiet">Make one, or run detection to find what already groups itself.</p>
    </div>
  {:else}
    <div class="grid stagger">
      {#each list as a, i (a.id)}
        <a class="card" href="#/album?id={a.id}" style="--i: {i}">
          <div class="cover">
            {#if a.cover_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="muted small">empty</span>
            {/if}
            <span class="meta-chip mono">{a.photo_count}</span>
          </div>
          <strong class="title">{a.name}</strong>
          {#if a.date_range_start && a.date_range_end}
            <span class="dates mono small muted">
              {new Date(a.date_range_start).toLocaleDateString()}
              — {new Date(a.date_range_end).toLocaleDateString()}
            </span>
          {/if}
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-6) var(--s-7); flex: 1; overflow-y: auto; }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
    display: flex; flex-direction: column; gap: var(--s-3); align-items: center;
  }
  .quiet {
    font-family: var(--font-display);
    font-style: italic;
    font-size: var(--t-lg);
    color: var(--ink-soft);
    max-width: 38ch;
  }

  .suggestions { margin-bottom: var(--s-5); }
  .suggestions p { margin-top: var(--s-2); margin-bottom: var(--s-4); }
  .suggest-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s-4);
  }
  .suggestion {
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .suggestion img {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    display: block;
  }
  .suggestion .body {
    padding: var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .suggestion .kind {
    font-size: 9px;
    color: var(--accent);
    letter-spacing: 0.18em;
  }
  .suggestion .title {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 500;
  }
  .suggestion .row { display: flex; gap: var(--s-2); margin-top: var(--s-2); }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: var(--s-5);
  }
  .card {
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    transition: transform var(--t-base-d) var(--ease);
  }
  .card:hover { text-decoration: none; transform: translateY(-3px); }
  .cover {
    aspect-ratio: 4 / 5;
    background: var(--bg-card);
    border-radius: var(--r-md);
    overflow: hidden;
    box-shadow: var(--shadow-soft);
    position: relative;
    display: flex; align-items: center; justify-content: center;
    border: 1px solid var(--line);
  }
  .cover img { width: 100%; height: 100%; object-fit: cover; }
  .meta-chip {
    position: absolute;
    bottom: var(--s-3); right: var(--s-3);
    background: rgba(0,0,0,0.5);
    backdrop-filter: blur(8px);
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 11px;
    color: var(--ink);
    border: 1px solid rgba(255,255,255,0.06);
  }
  .title {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 500;
    font-variation-settings: "opsz" 24;
  }
  .small { font-size: var(--t-xs); }
</style>
