<script lang="ts">
  import { onMount } from "svelte";
  import { trash } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";

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
    if (!confirm("Empty trash? All trashed photos and their files will be deleted.")) return;
    await trash.empty();
    await load();
  }

  onMount(load);
</script>

<main class="trash">
  <header>
    <h2>Trash</h2>
    {#if stats}
      <span class="muted">{stats.count} items, {(stats.total_size / 1024 / 1024).toFixed(1)} MB</span>
    {/if}
    <div class="actions">
      <button onclick={restore} disabled={selected.size === 0}>Restore ({selected.size})</button>
      <button class="danger" onclick={deleteForever} disabled={selected.size === 0}>Delete forever</button>
      <button class="danger" onclick={emptyTrash} disabled={items.length === 0}>Empty trash</button>
    </div>
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if items.length === 0}
    <p class="muted">Trash is empty.</p>
  {:else}
    <div class="grid">
      {#each items as t}
        <button
          class="cell"
          class:sel={selected.has(t.photo_id)}
          onclick={() => toggle(t.photo_id)}
        >
          {#if t.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, t.thumbnail_path) ?? ""} alt="" loading="lazy" />
          {/if}
          <span class="check">{selected.has(t.photo_id) ? "✓" : ""}</span>
        </button>
      {/each}
    </div>
  {/if}
</main>

<style>
  .trash { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 16px; flex-wrap: wrap; }
  h2 { margin: 0; flex-shrink: 0; }
  .actions { margin-left: auto; display: flex; gap: 8px; }
  .danger { background: #2a1414; border: 1px solid #4a2222; color: #f87171; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    background: #131316;
    border-radius: 4px;
    overflow: hidden;
    padding: 0;
    border: 2px solid transparent;
    position: relative;
  }
  .cell.sel { border-color: #6aa9ff; }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
  .check {
    position: absolute;
    top: 6px;
    right: 6px;
    background: #6aa9ff;
    color: white;
    width: 24px; height: 24px;
    border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-weight: bold;
    opacity: 0;
  }
  .cell.sel .check { opacity: 1; }
</style>
