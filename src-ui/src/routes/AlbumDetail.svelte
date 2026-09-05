<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { commandErrorMessage } from "../lib/api";
  import { albums, trash } from "../lib/api/all";
  import { system } from "../lib/api/system";
  import { toasts } from "../lib/stores/toast.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { marqueeSelect } from "../lib/actions/marqueeSelect";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import { Check, Download, FolderOpen, Play, X } from "lucide-svelte";
  import type { AlbumDto, PhotoSummaryDto } from "../lib/api/types";
  import { slideshow } from "../lib/stores/slideshow.svelte";
  import SurpriseButton from "../lib/components/SurpriseButton.svelte";

  interface Props { id: number }
  let { id }: Props = $props();

  let album = $state<AlbumDto | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let albumPhotoIds = $state<number[]>([]);
  let renaming = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);
  let loadingMore = $state(false);
  let exporting = $state(false);
  let exportResult = $state<AlbumExportComplete | null>(null);
  let confirmingDelete = $state(false);
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let actionBusy = $state(false);
  let loadSeq = 0;
  let mounted = true;
  let scrollRestored = false;
  const isSmartAlbum = $derived(album?.is_virtual ?? false);
  const scrollStorageKey = $derived(`smriti:album-scroll:${id}`);
  const selectedVisibleIds = $derived(selection.listIn(photos.map((p) => p.id)));
  const ALL_IDS_NAV_LIMIT = 5000;

  interface AlbumExportComplete {
    job_id: string;
    album_id: number;
    folder_path: string;
    exported: number;
    skipped_missing: number;
    failed: number;
    message: string;
  }

  function onCellClick(e: MouseEvent, photoId: number) {
    const ids = albumPhotoIds.length > 0 ? albumPhotoIds : photos.map((p) => p.id);
    const handled = handleCellClick(e, photoId, ids);
    if (!handled) browseContext.set(`album:${id}`, ids);
  }
  function patchThumbnail(photoId: number, thumbnailPath: string) {
    photos = photos.map((p) => (
      p.id === photoId ? { ...p, thumbnail_path: thumbnailPath } : p
    ));
  }
  async function bulkTrash() {
    if (actionBusy) return;
    const ids = selection.listIn(photos.map((p) => p.id));
    if (ids.length === 0) return;
    const seq = loadSeq;
    const albumId = id;
    const dropSet = new Set(ids);
    const snapshot = photos
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    const idSnapshot = albumPhotoIds
      .map((photoId, idx) => ({ idx, photoId }))
      .filter((e) => dropSet.has(e.photoId));
    try {
      actionBusy = true;
      const result = await trash.trashPhotos(ids);
      if (!mounted || seq !== loadSeq || albumId !== id) return;
      if (result.count === 0) {
        toasts.info("No selected photos needed trashing");
        return;
      }
      photoVisibility.markTrashed(ids);
      photos = photos.filter((p) => !dropSet.has(p.id));
      albumPhotoIds = albumPhotoIds.filter((photoId) => !dropSet.has(photoId));
      browseContext.remove(ids);
      selection.clear();
      toasts.undoable(
        `${result.count} ${result.count === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          if (!mounted || albumId !== id) return;
          photoVisibility.markRestored(ids);
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
          const nextIds = albumPhotoIds.slice();
          for (const e of idSnapshot) {
            if (!nextIds.includes(e.photoId)) {
              const at = Math.min(e.idx, nextIds.length);
              nextIds.splice(at, 0, e.photoId);
            }
          }
          albumPhotoIds = nextIds;
          browseContext.set(`album:${albumId}`, nextIds);
        },
      );
    } catch (e) {
      if (mounted && seq === loadSeq && albumId === id) toasts.error(`Couldn't move to trash: ${commandErrorMessage(e)}`);
    } finally {
      if (mounted) actionBusy = false;
    }
  }

  /// Remove the selected photos from THIS album only — doesn't trash
  /// them, doesn't touch other albums they're in. Undoable: the toast
  /// re-adds them and restores the original index ordering.
  async function removeFromAlbum() {
    if (isSmartAlbum || actionBusy) return;
    const ids = selection.listIn(photos.map((p) => p.id));
    if (ids.length === 0) return;
    const seq = loadSeq;
    const albumId = id;
    const dropSet = new Set(ids);
    const snapshot = photos
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    const idSnapshot = albumPhotoIds
      .map((photoId, idx) => ({ idx, photoId }))
      .filter((e) => dropSet.has(e.photoId));
    try {
      actionBusy = true;
      const result = await albums.removePhotos(albumId, ids);
      if (!mounted || seq !== loadSeq || albumId !== id) return;
      if (result.count === 0) {
        toasts.info("No selected photos were in this album");
        return;
      }
      photos = photos.filter((p) => !dropSet.has(p.id));
      albumPhotoIds = albumPhotoIds.filter((photoId) => !dropSet.has(photoId));
      browseContext.remove(ids);
      selection.clear();
      toasts.undoable(
        `${result.count} ${result.count === 1 ? "photo" : "photos"} removed from album`,
        async () => {
          await albums.addPhotos(albumId, ids);
          if (!mounted || albumId !== id) return;
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
          const nextIds = albumPhotoIds.slice();
          for (const e of idSnapshot) {
            if (!nextIds.includes(e.photoId)) {
              const at = Math.min(e.idx, nextIds.length);
              nextIds.splice(at, 0, e.photoId);
            }
          }
          albumPhotoIds = nextIds;
          browseContext.set(`album:${albumId}`, nextIds);
        },
      );
    } catch (e) {
      if (mounted && seq === loadSeq && albumId === id) toasts.error(`Couldn't remove from album: ${commandErrorMessage(e)}`);
    } finally {
      if (mounted) actionBusy = false;
    }
  }
  function onGlobalKey(e: KeyboardEvent) {
    if (e.key === "Escape" && (exportResult || confirmingDelete)) {
      exportResult = null;
      confirmingDelete = false;
      e.preventDefault();
      return;
    }
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (selection.active()) {
      if (e.key === "Escape") { selection.clear(); e.preventDefault(); }
      else if (e.key === "Delete" || e.key === "Backspace") { bulkTrash(); e.preventDefault(); }
      else if ((e.key === "a" || e.key === "A") && !e.metaKey && !e.ctrlKey) {
        showAddDialog = true; e.preventDefault();
      }
    }
  }

  function readSavedScroll() {
    const raw = (() => { try { return sessionStorage.getItem(scrollStorageKey); } catch { return null; } })();
    const y = raw ? Number(raw) : 0;
    return Number.isFinite(y) && y > 0 ? y : 0;
  }

  function saveScroll() {
    if (!scrollEl) return;
    try { sessionStorage.setItem(scrollStorageKey, String(scrollEl.scrollTop)); } catch {}
  }

  async function restoreSavedScroll() {
    if (scrollRestored || !scrollEl) return;
    const target = readSavedScroll();
    scrollRestored = true;
    if (target <= 0) return;
    await tick();
    for (let i = 0; mounted && scrollEl && scrollEl.scrollHeight - scrollEl.clientHeight < target && hasMore && nextCursor && i < 30; i++) {
      const before = photos.length;
      await loadMorePhotos();
      await tick();
      if (photos.length === before) break;
    }
    requestAnimationFrame(() => {
      if (mounted && scrollEl) scrollEl.scrollTop = target;
    });
  }

  onMount(() => {
    mounted = true;
    window.addEventListener("keydown", onGlobalKey);
    let unlisten: UnlistenFn | null = null;
    let disposed = false;
    listen<AlbumExportComplete>("album_export:complete", (event) => {
      if (event.payload.album_id !== id) return;
      exporting = false;
      exportResult = event.payload;
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    }).catch(() => {});
    return () => {
      saveScroll();
      mounted = false;
      loadSeq += 1;
      disposed = true;
      window.removeEventListener("keydown", onGlobalKey);
      unlisten?.();
    };
  });

  $effect(() => {
    const el = scrollEl;
    if (!el) return;
    const onScroll = () => {
      saveScroll();
      if (!hasMore || loadingMore) return;
      const remaining = el.scrollHeight - el.scrollTop - el.clientHeight;
      if (remaining < 900) void loadMorePhotos();
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  });

  async function load() {
    const seq = ++loadSeq;
    const albumId = id;
    error = null;
    scrollRestored = false;
    loadingMore = false;
    album = null;
    photos = [];
    albumPhotoIds = [];
    nextCursor = null;
    hasMore = false;
    exportResult = null;
    selection.clear();
    try {
      const [nextAlbum, page] = await Promise.all([
        albums.get(albumId),
        albums.photos(albumId, null, 500),
      ]);
      if (!mounted || seq !== loadSeq) return;
      let allIds = page.items.map((p) => p.id);
      if (nextAlbum.photo_count <= ALL_IDS_NAV_LIMIT) {
        allIds = await albums.photoIds(albumId);
        if (!mounted || seq !== loadSeq) return;
      }
      album = nextAlbum;
      editName = nextAlbum.name;
      photos = page.items;
      albumPhotoIds = allIds;
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      browseContext.set(`album:${albumId}`, allIds);
      void restoreSavedScroll();
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function loadMorePhotos() {
    if (!mounted || loadingMore || !hasMore || !nextCursor) return;
    const seq = loadSeq;
    const albumId = id;
    const cursor = nextCursor;
    loadingMore = true;
    try {
      const page = await albums.photos(albumId, cursor, 500);
      if (!mounted || seq !== loadSeq || albumId !== id) return;
      photos = photos.concat(page.items);
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      browseContext.extend(page.items.map((p) => p.id));
    } catch (e) {
      if (mounted && seq === loadSeq && albumId === id) {
        toasts.error(`Couldn't load more album photos: ${commandErrorMessage(e)}`);
      }
    } finally {
      if (mounted && seq === loadSeq && albumId === id) loadingMore = false;
    }
  }

  async function rename() {
    if (!album || isSmartAlbum || actionBusy) return;
    const seq = loadSeq;
    const albumId = id;
    try {
      actionBusy = true;
      const renamed = await albums.rename(albumId, editName.trim());
      if (!mounted || seq !== loadSeq || albumId !== id) return;
      album = renamed;
      renaming = false;
    }
    catch (e) { if (mounted && seq === loadSeq && albumId === id) error = commandErrorMessage(e); }
    finally { if (mounted) actionBusy = false; }
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
    if (isSmartAlbum || actionBusy) return;
    const seq = loadSeq;
    const albumId = id;
    try {
      actionBusy = true;
      await albums.delete(albumId);
      if (!mounted || seq !== loadSeq || albumId !== id) return;
      confirmingDelete = false;
      window.location.hash = "/albums";
    } catch (e) {
      if (mounted && seq === loadSeq && albumId === id) error = commandErrorMessage(e);
    } finally {
      if (mounted) actionBusy = false;
    }
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
      ids: albumPhotoIds.length > 0 ? albumPhotoIds : photos.map((p) => p.id),
      nextCursor,
      hasMore,
      loadMore: (cursor) => albums.photos(id, cursor, 200),
    });
  }

  async function exportAlbum() {
    if (!album || album.photo_count === 0 || exporting || actionBusy) return;
    const seq = loadSeq;
    const albumId = id;
    try {
      actionBusy = true;
      exporting = true;
      exportResult = null;
      const job = await albums.export(albumId);
      if (!mounted || seq !== loadSeq || albumId !== id) return;
      jobs.register(job.job_id, "albumExport");
      toasts.success("Album export started");
    } catch (e) {
      if (mounted && seq === loadSeq && albumId === id) {
        exporting = false;
        toasts.error(`Couldn't export album: ${commandErrorMessage(e)}`);
      }
    } finally {
      if (mounted) actionBusy = false;
    }
  }

  async function openExportFolder() {
    if (!exportResult) return;
    try {
      await system.openPath(exportResult.folder_path);
    } catch (e) {
      toasts.error(`Couldn't open export folder: ${commandErrorMessage(e)}`);
    }
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
        <button class="primary" onclick={rename} disabled={actionBusy}>Save</button>
        <button class="ghost" onclick={() => (renaming = false)} disabled={actionBusy}>Cancel</button>
      {:else}
        <button class="ghost icon-action" onclick={startAlbumSlideshow} disabled={photos.length === 0} title="Start slideshow" aria-label="Start album slideshow">
          <Play size={15} strokeWidth={2} />
        </button>
        <SurpriseButton albumId={id} label={a.name} disabled={a.photo_count === 0} />
        <button class="ghost export-action" onclick={exportAlbum} disabled={a.photo_count === 0 || exporting || actionBusy} title="Export album originals">
          <Download size={14} strokeWidth={1.9} />
          {exporting ? "Exporting" : "Export"}
        </button>
        {#if !isSmartAlbum}
          <button class="ghost" onclick={() => (renaming = true)} disabled={actionBusy}>Rename</button>
          <button class="danger" onclick={() => (confirmingDelete = true)} disabled={actionBusy}>Delete</button>
        {/if}
      {/if}
    {/snippet}
  </DetailHeader>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page-scroll" bind:this={scrollEl} use:marqueeSelect={{ getAllIds: () => photos.map((p) => p.id) }}>
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
  {#if loadingMore}
    <p class="loading-more mono">Loading more…</p>
  {/if}
</div>

{#if exportResult}
  <div class="modal-backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) exportResult = null; }}>
    <div class="export-modal" role="dialog" aria-modal="true" aria-labelledby="export-title">
      <header>
        <div>
          <h2 id="export-title">Album exported</h2>
          <p>{exportResult.message}</p>
        </div>
        <button class="icon-action ghost" onclick={() => (exportResult = null)} aria-label="Close">
          <X size={15} strokeWidth={2} />
        </button>
      </header>
      <div class="export-path mono" title={exportResult.folder_path}>{exportResult.folder_path}</div>
      {#if exportResult.failed > 0 || exportResult.skipped_missing > 0}
        <p class="export-note">
          {exportResult.skipped_missing} missing, {exportResult.failed} failed.
        </p>
      {/if}
      <footer>
        <button class="primary" onclick={openExportFolder}>
          <FolderOpen size={15} strokeWidth={2} />
          Open folder
        </button>
        <button class="ghost" onclick={() => (exportResult = null)}>Done</button>
      </footer>
    </div>
  </div>
{/if}

{#if confirmingDelete && album}
  <div class="modal-backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) confirmingDelete = false; }}>
    <div class="export-modal" role="dialog" aria-modal="true" aria-labelledby="delete-album-title">
      <header>
        <div>
          <h2 id="delete-album-title">Delete album?</h2>
          <p>{album.name}</p>
        </div>
        <button class="icon-action ghost" onclick={() => (confirmingDelete = false)} aria-label="Close">
          <X size={15} strokeWidth={2} />
        </button>
      </header>
      <p class="export-note">Photos and videos stay in the library. Only this album is removed.</p>
      <footer>
        <button class="danger" onclick={deleteAlbum} disabled={actionBusy}>Delete album</button>
        <button class="ghost" onclick={() => (confirmingDelete = false)} disabled={actionBusy}>Cancel</button>
      </footer>
    </div>
  </div>
{/if}

{#if selectedVisibleIds.length > 0}
  <SelectionBar
    count={selectedVisibleIds.length}
    onAddToAlbum={() => (showAddDialog = true)}
    onRemoveFromAlbum={isSmartAlbum ? undefined : removeFromAlbum}
    onTrash={bulkTrash}
    onCancel={() => selection.clear()}
  />
{/if}

{#if showAddDialog}
  <AddToAlbumDialog
    photoIds={selectedVisibleIds}
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
  .loading-more {
    margin: var(--s-4) 0 0;
    text-align: center;
    color: var(--ink-muted);
    font-size: var(--t-xs);
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
  .export-action,
  .export-modal footer button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    background: rgba(0,0,0,0.48);
    display: grid;
    place-items: center;
    padding: var(--s-4);
  }
  .export-modal {
    width: min(520px, 94vw);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: 0 22px 60px rgba(0,0,0,0.48);
    padding: var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    max-height: calc(100vh - 2 * var(--s-4));
    overflow-y: auto;
  }
  .export-modal header,
  .export-modal footer {
    display: flex;
    align-items: center;
    gap: var(--s-3);
  }
  .export-modal header > div {
    flex: 1;
    min-width: 0;
  }
  .export-modal h2 {
    margin: 0;
    font-size: var(--t-lg);
  }
  .export-modal p {
    margin: 4px 0 0;
    color: var(--ink-muted);
    font-size: var(--t-sm);
  }
  .export-path {
    padding: var(--s-3);
    background: var(--bg);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-sm);
    color: var(--ink-muted);
    font-size: var(--t-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .export-note {
    color: var(--hot, #d05a4a) !important;
  }
  .export-modal footer {
    justify-content: flex-end;
  }
</style>
