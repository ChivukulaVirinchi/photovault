<script lang="ts">
  import { onMount } from "svelte";
  import { trash } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";

  let items = $state<Awaited<ReturnType<typeof trash.list>>["items"]>([]);
  let stats = $state<{ count: number; total_size: number } | null>(null);
  let selected = $state<Set<number>>(new Set());
  let error = $state<string | null>(null);

  async function load() {
    try {
      const page = await trash.list(null, 500);
      items = page.items;
      stats = await trash.stats();
    } catch (e) { error = JSON.stringify(e); }
  }

  function toggle(id: number) {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id); else s.add(id);
    selected = s;
  }

  async function restore() {
    if (selected.size === 0) return;
    await trash.restore([...selected]);
    selected = new Set();
    await load();
  }

  async function deleteForever() {
    if (selected.size === 0) return;
    if (!confirm(`Permanently delete ${selected.size} photos? This cannot be undone.`)) return;
    await trash.permanentDelete([...selected]);
    selected = new Set();
    await load();
  }

  async function emptyTrash() {
    if (!confirm("Empty trash? All trashed photos and their files will be deleted from disk.")) return;
    await trash.empty();
    await load();
  }

  onMount(load);
</script>

<PageHeader title="Trash">
  {#if stats}
    <span class="count mono">
      {stats.count}<span class="muted"> · {(stats.total_size / 1024 / 1024).toFixed(0)} MB</span>
    </span>
  {/if}
  <button onclick={restore} disabled={selected.size === 0}>
    Restore <span class="mono">{selected.size}</span>
  </button>
  <button class="danger" onclick={deleteForever} disabled={selected.size === 0}>Delete forever</button>
  <button class="danger" onclick={emptyTrash} disabled={items.length === 0}>Empty</button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if items.length === 0}
    <div class="empty">
      <p>Nothing in trash. A clean shelf.</p>
    </div>
  {:else}
    <div class="grid">
      {#each items as t (t.photo_id)}
        <button
          class="cell"
          class:sel={selected.has(t.photo_id)}
          onclick={() => toggle(t.photo_id)}
        >
          {#if t.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, t.thumbnail_path) ?? ""} alt="" loading="lazy" />
          {/if}
          <span class="check" aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path d="M3 7L6 10L11 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-4) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .count { font-size: var(--t-sm); color: var(--ink); }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
  }
  .empty p { color: var(--ink-muted); font-style: italic; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 4px;
  }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    padding: 0;
    border: 2px solid transparent;
    position: relative;
    cursor: pointer;
    transition: border-color var(--t-fast) var(--ease),
                filter var(--t-fast) var(--ease);
  }
  .cell:hover { filter: brightness(1.06); }
  .cell.sel { border-color: var(--accent); }
  .cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.55;
    transition: opacity var(--t-fast) var(--ease);
  }
  .cell.sel img { opacity: 1; }
  .check {
    position: absolute;
    top: 8px;
    right: 8px;
    background: var(--accent);
    color: #fff;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transform: scale(0.7);
    transition: opacity var(--t-fast) var(--ease),
                transform var(--t-fast) var(--ease);
  }
  .cell.sel .check { opacity: 1; transform: scale(1); }
</style>
