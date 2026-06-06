<script lang="ts">
  import { onMount } from "svelte";
  import { albums, trash } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { marqueeSelect } from "../lib/actions/marqueeSelect";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import { Check, Play } from "lucide-svelte";
  import type { AlbumDto, PhotoSummaryDto } from "../lib/api/types";
  import { slideshow } from "../lib/stores/slideshow.svelte";

  interface Props { id: number }
  let { id }: Props = $props();

  let album = $state<AlbumDto | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let renaming = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);
  const isSmartAlbum = $derived(album?.is_virtual ?? false);

  function onCellClick(e: MouseEvent, photoId: number) {
    handleCellClick(e, photoId, photos.map((p) => p.id));
  }
  function patchThumbnail(photoId: number, thumbnailPath: string) {
    photos = photos.map((p) => (
      p.id === photoId ? { ...p, thumbnail_path: thumbnailPath } : p
    ));
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
      photoVisibility.markTrashed(ids);
      photos = photos.filter((p) => !dropSet.has(p.id));
      browseContext.remove(ids);
      selection.clear();
      toasts.undoable(
        `${ids.length} ${ids.length === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          photoVisibility.markRestored(ids);
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
          browseContext.set(`album:${id}`, photos.map((p) => p.id));
        },
      );
    } catch (e) { toasts.error(`Couldn't move to trash: ${e}`); }
  }

  /// Remove the selected photos from THIS album only — doesn't trash
  /// them, doesn't touch other albums they're in. Undoable: the toast
  /// re-adds them and restores the original index ordering.
  async function removeFromAlbum() {
    if (isSmartAlbum) return;
    const ids = selection.list();
    if (ids.length === 0) return;
    const dropSet = new Set(ids);
    const snapshot = photos
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    try {
      await albums.removePhotos(id, ids);
      photos = photos.filter((p) => !dropSet.has(p.id));
      selection.clear();
      toasts.undoable(
        `${ids.length} ${ids.length === 1 ? "photo" : "photos"} removed from album`,
        async () => {
          await albums.addPhotos(id, ids);
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
        },
      );
    } catch (e) {
      toasts.error(`Couldn't remove from album: ${e}`);
    }
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
      album = await albums.get(id);
      editName = album.name;
      const page = await albums.photos(id);
      photos = page.items;
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      browseContext.set(`album:${id}`, photos.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
  }

  async function rename() {
    if (!album || isSmartAlbum) return;
    try { album = await albums.rename(id, editName.trim()); renaming = false; }
    catch (e) { error = JSON.stringify(e); }
  }

  function onRenameKey(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); rename(); }
    else if (e.key === "Escape") {
      e.preventDefault();
      editName = album?.name ?? "";
      renaming = false;
    }
  }

  async function deleteAlbum() {
    if (isSmartAlbum) return;
    if (!confirm("Delete album? Photos will not be trashed.")) return;
    try { await albums.delete(id); window.location.hash = "/albums"; }
    catch (e) { error = JSON.stringify(e); }
  }

  function fmtRange(s: string | null, e: string | null): string {
    if (!s || !e) return "";
    return `${new Date(s).toLocaleDateString()} → ${new Date(e).toLocaleDateString()}`;
  }

  function startAlbumSlideshow() {
    if (!album || photos.length === 0) return;
    slideshow.start({
      kind: "album",
      label: album.name,
      ids: photos.map((p) => p.id),
      nextCursor,
      hasMore,
      loadMore: (cursor) => albums.photos(id, cursor, 200),
    });
  }

  $effect(() => { void id; load(); });
</script>

{#if album}
  {@const a = album}
  <DetailHeader backHref="#/albums" backLabel="Albums">
    {#snippet title()}
      {#if renaming}
        <!-- svelte-ignore a11y_autofocus -->
        <input bind:value={editName} onkeydown={onRenameKey} placeholder="Album name" autofocus />
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
        <button class="ghost icon-action" onclick={startAlbumSlideshow} disabled={photos.length === 0} title="Start slideshow" aria-label="Start album slideshow">
          <Play size={15} strokeWidth={2} />
        </button>
        {#if !isSmartAlbum}
          <button class="ghost" onclick={() => (renaming = true)}>Rename</button>
          <button class="danger" onclick={deleteAlbum}>Delete</button>
        {/if}
      {/if}
    {/snippet}
  </DetailHeader>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page-scroll" use:marqueeSelect={{ getAllIds: () => photos.map((p) => p.id) }}>
  <div class="pv-photo-grid">
    {#each photos as p (p.id)}
      <a
        class="pv-photo-cell"
        class:selected={selection.has(p.id)}
        data-photo-id={p.id}
        href="#/photo?id={p.id}"
        use:thumbnailOnVisible={{
          id: p.id,
          thumbnailPath: p.thumbnail_path,
          mediaType: p.media_type,
          onReady: (path) => patchThumbnail(p.id, path),
        }}
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
    onRemoveFromAlbum={isSmartAlbum ? undefined : removeFromAlbum}
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
  .page-scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-4) var(--s-7) var(--s-7);
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
  .dim { color: var(--ink-faint); }
  .icon-action {
    width: 32px;
    height: 32px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
</style>
