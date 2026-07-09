<script lang="ts">
  import { onMount } from "svelte";
  import { albums } from "../api/all";
  import { commandErrorMessage } from "../api";
  import type { AlbumDto } from "../api/types";
  import { FolderPlus, Search, Plus } from "lucide-svelte";

  interface Props {
    photoIds: number[];
    onclose: () => void;
    onsuccess?: (album: AlbumDto, count: number) => void;
  }
  let { photoIds, onclose, onsuccess }: Props = $props();

  let allAlbums = $state<AlbumDto[]>([]);
  let filter = $state("");
  let creating = $state(false);
  let newName = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let searchEl = $state<HTMLInputElement | undefined>(undefined);
  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  let mounted = true;
  let focusTimer: ReturnType<typeof setTimeout> | null = null;

  const filtered = $derived(
    filter.trim() === ""
      ? allAlbums
      : allAlbums.filter((a) =>
          a.name.toLowerCase().includes(filter.trim().toLowerCase()),
        ),
  );

  function requestClose() {
    if (!busy) onclose();
  }

  async function load() {
    try {
      const next = (await albums.list()).filter((a) => !a.is_virtual);
      if (!mounted) return;
      allAlbums = next;
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    }
  }

  async function pick(album: AlbumDto) {
    if (busy) return;
    busy = true;
    try {
      const r = await albums.addPhotos(album.id, photoIds);
      if (!mounted) return;
      onsuccess?.(album, r.count);
      onclose();
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
      busy = false;
    }
  }

  async function createAndAdd() {
    if (busy) return;
    const name = newName.trim();
    if (!name) return;
    busy = true;
    try {
      const a = await albums.create(name, photoIds);
      if (!mounted) return;
      onsuccess?.(a, a.photos_added ?? photoIds.length);
      onclose();
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      requestClose();
    } else if (e.key === "Enter" && !creating) {
      e.preventDefault();
      const first = filtered[0];
      if (first) pick(first);
    }
  }
  function onCreateKey(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); createAndAdd(); }
    else if (e.key === "Escape") {
      e.preventDefault();
      creating = false;
      newName = "";
      if (focusTimer != null) clearTimeout(focusTimer);
      focusTimer = setTimeout(() => searchEl?.focus(), 0);
    }
  }

  onMount(() => {
    mounted = true;
    load();
    focusTimer = setTimeout(() => searchEl?.focus(), 0);
    return () => {
      mounted = false;
      if (focusTimer != null) clearTimeout(focusTimer);
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={(e) => { if (e.target === e.currentTarget) requestClose(); }}>
  <div class="dialog" onkeydown={onKey} role="dialog" tabindex="-1" aria-modal="true" aria-label="Add to album">
    <header>
      <h3>Add {photoIds.length} {photoIds.length === 1 ? "photo" : "photos"} to album</h3>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    {#if creating}
      <div class="row create">
        <Plus size={16} strokeWidth={1.75} />
        <!-- svelte-ignore a11y_autofocus -->
        <input
          bind:this={nameEl}
          bind:value={newName}
          onkeydown={onCreateKey}
          placeholder="Name the new album"
          autofocus
        />
        <button class="primary" onclick={createAndAdd} disabled={busy || !newName.trim()}>Create</button>
        <button class="ghost" disabled={busy} onclick={() => {
          creating = false;
          newName = "";
          if (focusTimer != null) clearTimeout(focusTimer);
          focusTimer = setTimeout(() => searchEl?.focus(), 0);
        }}>Cancel</button>
      </div>
    {:else}
      <div class="search">
        <Search size={14} strokeWidth={1.75} />
        <input bind:this={searchEl} bind:value={filter} placeholder="Find an album…" />
      </div>

      <ul class="list">
        <li>
          <button class="new" onclick={() => {
            creating = true;
            if (focusTimer != null) clearTimeout(focusTimer);
            focusTimer = setTimeout(() => nameEl?.focus(), 0);
          }}>
            <FolderPlus size={16} strokeWidth={1.75} />
            <span class="name">New album…</span>
          </button>
        </li>
        {#each filtered as a (a.id)}
          <li>
            <button class="row" onclick={() => pick(a)} disabled={busy}>
              <span class="name">{a.name}</span>
              <span class="count mono">{a.photo_count}</span>
            </button>
          </li>
        {/each}
        {#if filtered.length === 0 && filter}
          <li class="empty">No album matches "{filter}".</li>
        {/if}
      </ul>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(6px);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fade 140ms var(--ease) both;
  }
  @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
  .dialog {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-lg, 12px);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55);
    width: min(440px, 92vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    animation: pop 180ms var(--ease) both;
  }
  @keyframes pop {
    from { transform: scale(0.96); opacity: 0; }
    to   { transform: scale(1); opacity: 1; }
  }
  header {
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  header h3 {
    margin: 0;
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
  }
  .error {
    margin: 0;
    padding: var(--s-2) var(--s-5);
    background: color-mix(in oklab, var(--bg-paper) 70%, var(--danger, #d96363));
    color: var(--danger, #d96363);
    font-size: var(--t-xs);
  }
  .search, .create {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-3) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  .search :global(svg), .create :global(svg) { color: var(--ink-muted); flex-shrink: 0; }
  .search input, .create input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: var(--t-sm);
    color: var(--ink);
    padding: 4px 0;
  }
  .search input:focus, .create input:focus { outline: none; }
  .list {
    list-style: none;
    margin: 0;
    padding: 6px 0;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
  }
  .list li.empty {
    padding: var(--s-4);
    color: var(--ink-muted);
    font-size: var(--t-sm);
    text-align: center;
    font-style: italic;
  }
  .row, .new {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    width: 100%;
    padding: 8px var(--s-5);
    background: transparent;
    border: none;
    color: var(--ink);
    font-size: var(--t-sm);
    text-align: left;
    cursor: pointer;
    font: inherit;
    transition: background var(--t-fast) var(--ease);
  }
  .row:hover, .row:focus, .new:hover, .new:focus {
    background: var(--bg-card);
    outline: none;
  }
  .new {
    color: var(--accent);
    border-bottom: 1px solid var(--line-soft);
    margin-bottom: 4px;
    padding-bottom: 10px;
  }
  .new :global(svg) { color: var(--accent); }
  .name { flex: 1; }
  .count { color: var(--ink-muted); font-size: var(--t-xs); }
  .row:disabled { opacity: 0.5; cursor: wait; }
</style>
