<script lang="ts">
  import { albums } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import type { AlbumDto, PhotoSummaryDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let album = $state<AlbumDto | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let renaming = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);

  async function load() {
    try {
      album = await albums.get(id);
      editName = album.name;
      const page = await albums.photos(id);
      photos = page.items;
      browseContext.set(`album:${id}`, photos.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
  }

  async function rename() {
    if (!album) return;
    try { album = await albums.rename(id, editName.trim()); renaming = false; }
    catch (e) { error = JSON.stringify(e); }
  }

  async function deleteAlbum() {
    if (!confirm("Delete album? Photos will not be trashed.")) return;
    try { await albums.delete(id); window.location.hash = "/albums"; }
    catch (e) { error = JSON.stringify(e); }
  }

  function fmtRange(s: string | null, e: string | null): string {
    if (!s || !e) return "";
    return `${new Date(s).toLocaleDateString()} → ${new Date(e).toLocaleDateString()}`;
  }

  $effect(() => { void id; load(); });
</script>

{#if album}
  {@const a = album}
  <DetailHeader backHref="#/albums" backLabel="Albums">
    {#snippet title()}
      {#if renaming}
        <input bind:value={editName} placeholder="Album name" />
      {:else}
        <h1>{a.name}</h1>
      {/if}
    {/snippet}
    {#snippet subtitle()}
      <span class="mono">{a.photo_count} photos</span>
      {#if a.date_range_start && a.date_range_end}
        <span class="mono dim">{fmtRange(a.date_range_start, a.date_range_end)}</span>
      {/if}
    {/snippet}
    {#snippet actions()}
      {#if renaming}
        <button class="primary" onclick={rename}>Save</button>
        <button class="ghost" onclick={() => (renaming = false)}>Cancel</button>
      {:else}
        <button class="ghost" onclick={() => (renaming = true)}>Rename</button>
        <button class="danger" onclick={deleteAlbum}>Delete</button>
      {/if}
    {/snippet}
  </DetailHeader>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="grid">
  {#each photos as p (p.id)}
    <a class="cell" href="#/photo?id={p.id}">
      {#if p.thumbnail_path}
        <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
      {/if}
    </a>
  {/each}
</div>

<style>
  .grid {
    padding: var(--s-4) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 4px;
  }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .cell:hover {
    filter: brightness(1.06);
    box-shadow: 0 0 0 2px var(--accent-ghost);
    z-index: 1;
  }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
  .dim { color: var(--ink-faint); }
</style>
