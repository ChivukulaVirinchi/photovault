<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  let exporting = $state(false);
  let exportResult = $state<AlbumExportComplete | null>(null);
  let confirmingDelete = $state(false);
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  const isSmartAlbum = $derived(album?.is_virtual ?? false);
  const scrollStorageKey = $derived(`smriti:album-scroll:${id}`);

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
    let unlisten: UnlistenFn | null = null;
    listen<AlbumExportComplete>("album_export:complete", (event) => {
      if (event.payload.album_id !== id) return;
      exporting = false;
      exportResult = event.payload;
    }).then((fn) => (unlisten = fn));
    return () => {
      window.removeEventListener("keydown", onGlobalKey);
      unlisten?.();
    };
  });

  $effect(() => {
    if (!scrollEl) return;
    const raw = (() => { try { return sessionStorage.getItem(scrollStorageKey); } catch { return null; } })();
    if (raw) {
      const y = Number(raw);
      if (Number.isFinite(y) && y > 0) requestAnimationFrame(() => { if (scrollEl) scrollEl.scrollTop = y; });
    }
    const onScroll = () => {
      try { sessionStorage.setItem(scrollStorageKey, String(scrollEl?.scrollTop ?? 0)); } catch {}
    };
    scrollEl.addEventListener("scroll", onScroll, { passive: true });
    return () => scrollEl?.removeEventListener("scroll", onScroll);
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
    try {
      await albums.delete(id);
      confirmingDelete = false;
      window.location.hash = "/albums";
    } catch (e) { error = JSON.stringify(e); }
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

  async function exportAlbum() {
    if (!album || album.photo_count === 0 || exporting) return;
    try {
      exporting = true;
      exportResult = null;
      const job = await albums.export(id);
      jobs.register(job.job_id, "albumExport");
      toasts.success("Album export started");
    } catch (e) {
      exporting = false;
      toasts.error(`Couldn't export album: ${e}`);
    }
  }

  async function openExportFolder() {
    if (!exportResult) return;
    try {
      await system.openPath(exportResult.folder_path);
    } catch (e) {
      toasts.error(`Couldn't open export folder: ${e}`);
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
        <button class="primary" onclick={rename}>Save</button>
        <button class="ghost" onclick={() => (renaming = false)}>Cancel</button>
      {:else}
        <button class="ghost icon-action" onclick={startAlbumSlideshow} disabled={photos.length === 0} title="Start slideshow" aria-label="Start album slideshow">
          <Play size={15} strokeWidth={2} />
        </button>
        <button class="ghost export-action" onclick={exportAlbum} disabled={a.photo_count === 0 || exporting} title="Export album originals">
          <Download size={14} strokeWidth={1.9} />
          {exporting ? "Exporting" : "Export"}
        </button>
        {#if !isSmartAlbum}
          <button class="ghost" onclick={() => (renaming = true)}>Rename</button>
          <button class="danger" onclick={() => (confirmingDelete = true)}>Delete</button>
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
        <button class="danger" onclick={deleteAlbum}>Delete album</button>
        <button class="ghost" onclick={() => (confirmingDelete = false)}>Cancel</button>
      </footer>
    </div>
  </div>
{/if}

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
