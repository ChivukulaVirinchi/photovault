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

<PageHeader title="Albums">
  <span class="count mono">{list.length}<span class="muted"> albums</span></span>
  <button class="ghost" onclick={runDetection}>Detect</button>
  {#if creating}
    <input bind:value={newName} placeholder="Album name" />
    <button class="primary" onclick={createAlbum}>Create</button>
    <button class="ghost" onclick={() => (creating = false)}>Cancel</button>
  {:else}
    <button class="primary" onclick={() => (creating = true)}>New album</button>
  {/if}
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if suggestions.length > 0}
    <section class="suggestions">
      <div class="section-head">
        <h3 class="section-title">Suggestions</h3>
        <span class="hint">Patterns we noticed — accept to save, dismiss to ignore.</span>
      </div>
      <div class="suggest-grid">
        {#each suggestions as s (s.id)}
          <article class="suggestion">
            {#if s.cover_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, s.cover_thumbnail_path) ?? ""} alt="" />
            {/if}
            <div class="body">
              <span class="kind mono">{s.kind}</span>
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
    {#if list.length > 0}<hr class="hairline" />{/if}
  {/if}

  {#if list.length === 0 && suggestions.length === 0}
    <div class="empty">
      <p>Make one, or run detection to find what already groups itself.</p>
      <div class="row">
        <button class="primary" onclick={() => (creating = true)}>New album</button>
        <button class="ghost" onclick={runDetection}>Detect</button>
      </div>
    </div>
  {:else if list.length > 0}
    <div class="grid">
      {#each list as a (a.id)}
        <a class="card" href="#/album?id={a.id}">
          <div class="cover">
            {#if a.cover_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="placeholder small">empty</span>
            {/if}
            <span class="meta-chip mono">{a.photo_count}</span>
          </div>
          <strong class="title">{a.name}</strong>
          {#if a.date_range_start && a.date_range_end}
            <span class="dates mono small">
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
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .count { font-size: var(--t-sm); color: var(--ink); }

  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    align-items: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.55; }
  .empty .row { display: flex; gap: var(--s-2); }

  .suggestions { margin-bottom: var(--s-4); }
  .section-head {
    display: flex;
    align-items: baseline;
    gap: var(--s-3);
    margin-bottom: var(--s-3);
  }
  .section-title {
    font-size: var(--t-sm);
    font-weight: 600;
    color: var(--ink);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: 0;
  }
  .hint {
    font-size: var(--t-sm);
    color: var(--ink-muted);
    font-style: italic;
  }

  .suggest-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--s-3);
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
    padding: var(--s-3) var(--s-4) var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .suggestion .kind {
    font-size: var(--t-xs);
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
  }
  .suggestion .title {
    font-size: var(--t-base);
    font-weight: 600;
  }
  .suggestion .row { display: flex; gap: var(--s-2); margin-top: var(--s-2); }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--s-4);
  }
  .card {
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: 6px;
    text-decoration: none;
  }
  .cover {
    aspect-ratio: 4 / 5;
    background: var(--bg-card);
    border-radius: var(--r-md);
    overflow: hidden;
    position: relative;
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .card:hover .cover {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-ghost);
  }
  .cover img { width: 100%; height: 100%; object-fit: cover; }
  .placeholder { color: var(--ink-faint); }
  .meta-chip {
    position: absolute;
    bottom: var(--s-2);
    right: var(--s-2);
    background: color-mix(in oklab, var(--bg) 72%, transparent);
    backdrop-filter: blur(6px);
    padding: 3px 9px;
    border-radius: 999px;
    font-size: var(--t-xs);
    color: var(--ink);
    border: 1px solid var(--line);
  }
  .title {
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
    margin-top: 4px;
  }
  .dates {
    color: var(--ink-faint);
  }
  .small { font-size: var(--t-xs); }
</style>
