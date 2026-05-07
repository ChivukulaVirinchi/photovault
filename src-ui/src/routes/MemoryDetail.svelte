<script lang="ts">
  import { onMount } from "svelte";
  import { memories, trash } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import { Check } from "lucide-svelte";
  import type { PhotoSummaryDto } from "../lib/api/types";
  import type { MemoryCard } from "../lib/api/all";

  interface Props { id: string }
  let { id }: Props = $props();

  let card = $state<MemoryCard | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let savedAlbumId = $state<number | null>(null);
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);

  function onCellClick(e: MouseEvent, photoId: number) {
    handleCellClick(e, photoId, photos.map((p) => p.id));
  }
  async function bulkTrash() {
    const ids = selection.list();
    if (ids.length === 0) return;
    const dropSet = new Set(ids);
    const snapshot = photos
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    try {
      await trash.trashPhotos(ids);
      photos = photos.filter((p) => !dropSet.has(p.id));
      selection.clear();
      toasts.undoable(
        `${ids.length} ${ids.length === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
        },
      );
    } catch (e) { toasts.error(`Couldn't move to trash: ${e}`); }
  }
  function onGlobalKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (selection.active()) {
      if (e.key === "Escape") { selection.clear(); e.preventDefault(); }
      else if (e.key === "Delete" || e.key === "Backspace") { bulkTrash(); e.preventDefault(); }
      else if ((e.key === "a" || e.key === "A") && !e.metaKey && !e.ctrlKey) {
        showAddDialog = true; e.preventDefault();
      }
    }
  }
  onMount(() => {
    window.addEventListener("keydown", onGlobalKey);
    return () => window.removeEventListener("keydown", onGlobalKey);
  });

  async function load() {
    try {
      const r = await memories.detail(id);
      card = r.card;
      photos = r.photos;
      browseContext.set(`memory:${id}`, photos.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
  }

  async function saveAsAlbum() {
    if (!card) return;
    const a = await memories.saveAsAlbum(card.id);
    savedAlbumId = a.id;
  }

  $effect(() => { void id; load(); });
</script>

{#if card}
  {@const c = card}
  <DetailHeader backHref="#/memories" backLabel="Memories">
    {#snippet title()}
      <h1>{c.title}</h1>
    {/snippet}
    {#snippet subtitle()}
      <span class="mono">{c.photo_count} photos</span>
      <span class="kind">{c.kind}</span>
    {/snippet}
    {#snippet actions()}
      {#if savedAlbumId}
        <a class="saved-link" href="#/album?id={savedAlbumId}">Saved as album →</a>
      {:else}
        <button class="primary" onclick={saveAsAlbum}>Save as album</button>
      {/if}
    {/snippet}
  </DetailHeader>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page-scroll">
  <div class="pv-photo-grid">
    {#each photos as p (p.id)}
      <a
        class="pv-photo-cell"
        class:selected={selection.has(p.id)}
        href="#/photo?id={p.id}"
        onclick={(e) => onCellClick(e, p.id)}
      >
        {#if p.thumbnail_path}
          <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
        {/if}
        {#if selection.has(p.id)}
          <span class="check" aria-hidden="true">
            <Check size={14} strokeWidth={2.5} />
          </span>
        {/if}
      </a>
    {/each}
  </div>
</div>

{#if selection.active()}
  <SelectionBar
    count={selection.size()}
    onAddToAlbum={() => (showAddDialog = true)}
    onTrash={bulkTrash}
    onCancel={() => selection.clear()}
  />
{/if}

{#if showAddDialog}
  <AddToAlbumDialog
    photoIds={selection.list()}
    onclose={() => (showAddDialog = false)}
    onsuccess={() => selection.clear()}
  />
{/if}

<style>
  .kind {
    text-transform: lowercase;
    color: var(--ink-faint);
  }
  .saved-link {
    font-size: var(--t-sm);
    color: var(--accent);
    text-decoration: none;
    border-bottom: 1px solid var(--accent-soft);
    padding-bottom: 2px;
  }
  .saved-link:hover { border-bottom-color: var(--accent); }

  .page-scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-5) var(--s-7) var(--s-8);
  }
  .pv-photo-cell .check {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--bg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 2px 6px rgba(0,0,0,0.4);
    pointer-events: none;
  }
</style>
