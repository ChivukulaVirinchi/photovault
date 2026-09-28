<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { trash } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
  import { selection } from "../lib/stores/selection.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { routeCache } from "../lib/stores/routeCache.svelte";
  import { marqueeSelect } from "../lib/actions/marqueeSelect";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import { createVirtualScroll } from "../lib/virtualizer.svelte";

  let items = $state<Awaited<ReturnType<typeof trash.list>>["items"]>([]);
  let stats = $state<{ count: number; total_size: number } | null>(null);
  let error = $state<string | null>(null);
  let actionBusy = $state(false);
  let deleteErrors = $state<string[]>([]);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);
  let loadingMore = $state(false);
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let mounted = true;
  let loadSeq = 0;
  let galleryColumns = $state(1);
  const galleryRows = $derived.by(() =>
    Array.from({ length: Math.ceil(items.length / galleryColumns) }, () => ({ height: 190 })),
  );
  const galleryVirtual = createVirtualScroll({ rows: () => galleryRows, scrollEl: () => scrollEl, overscan: 3 });

  const visibleIds = $derived(items.map((t) => t.photo_id));
  const selectedTrashIds = $derived.by(() => {
    const visible = new Set(visibleIds);
    return selection.list().filter((id) => visible.has(id));
  });
  const selectedTrashCount = $derived(selectedTrashIds.length);

  async function load() {
    const seq = ++loadSeq;
    error = null;
    loadingMore = false;
    try {
      // Cached per library session; mutations invalidate via
      // photoVisibility, so this only skips the IPC on repeat visits.
      const { page, nextStats } = await routeCache.get("trash:page", async () => {
        const page = await trash.list(null, 500);
        const nextStats = await trash.stats();
        return { page, nextStats };
      });
      if (!mounted || seq !== loadSeq) return;
      const nextVisibleIds = page.items.map((t) => t.photo_id);
      items = page.items;
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      browseContext.set("trash", nextVisibleIds);
      stats = nextStats;
      const visible = new Set(nextVisibleIds);
      if (selection.list().some((id) => !visible.has(id))) {
        selection.replace(selection.list().filter((id) => visible.has(id)));
      }
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function loadMoreTrash() {
    if (!mounted || loadingMore || !hasMore || !nextCursor) return;
    const seq = loadSeq;
    const cursor = nextCursor;
    loadingMore = true;
    try {
      const page = await trash.list(cursor, 500);
      if (!mounted || seq !== loadSeq) return;
      items = items.concat(page.items);
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      browseContext.extend(page.items.map((t) => t.photo_id));
    } catch (e) {
      if (mounted && seq === loadSeq) {
        toasts.error(`Couldn't load more trash: ${commandErrorMessage(e)}`);
      }
    } finally {
      if (mounted && seq === loadSeq) loadingMore = false;
    }
  }

  function onTrashScroll() {
    if (!scrollEl || !hasMore || loadingMore) return;
    const remaining = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    if (remaining < 900) void loadMoreTrash();
  }

  function toggle(id: number) {
    selection.toggle(id);
  }

  function patchThumbnail(photoId: number, thumbnailPath: string) {
    items = items.map((t) => (
      t.photo_id === photoId ? { ...t, thumbnail_path: thumbnailPath } : t
    ));
  }

  async function restore() {
    if (selectedTrashCount === 0 || actionBusy) return;
    const seq = loadSeq;
    const ids = selectedTrashIds;
    const session = libraryStore.session;
    try {
      actionBusy = true;
      await trash.restore(ids, session);
      if (!mounted || seq !== loadSeq) return;
      photoVisibility.markRestored(ids);
      selection.clear();
      await load();
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    } finally {
      if (mounted) actionBusy = false;
    }
  }

  async function deleteForever() {
    if (selectedTrashCount === 0 || actionBusy) return;
    const ids = selectedTrashIds;
    if (!confirm(`Permanently delete ${ids.length} photos? This cannot be undone.`)) return;
    const seq = loadSeq;
    try {
      actionBusy = true;
      const result = await trash.permanentDelete(ids, libraryStore.session);
      if (!mounted || seq !== loadSeq) return;
      photoVisibility.markTrashed(ids);
      deleteErrors = result.errors;
      browseContext.remove(ids);
      selection.clear();
      if (result.committed_with_recovery_errors) {
        toasts.error("Deletion was committed, but cleanup is pending recovery. Keep this library available and retry after recovery.");
      } else if (result.errors.length > 0) {
        toasts.error(`${result.errors.length} file${result.errors.length === 1 ? "" : "s"} could not be deleted. The affected items remain available for retry.`);
      } else {
        toasts.success(`${result.db_records_deleted} photo${result.db_records_deleted === 1 ? "" : "s"} permanently deleted.`);
      }
      await load();
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    } finally {
      if (mounted) actionBusy = false;
    }
  }

  async function emptyTrash() {
    if (actionBusy) return;
    if (!confirm("Empty trash? All trashed photos and their files will be deleted from disk.")) return;
    const seq = loadSeq;
    const ids = visibleIds;
    try {
      actionBusy = true;
      const result = await trash.empty(libraryStore.session);
      if (!mounted || seq !== loadSeq) return;
      photoVisibility.markTrashed(ids);
      deleteErrors = result.errors;
      browseContext.remove(ids);
      selection.clear();
      if (result.committed_with_recovery_errors) {
        toasts.error("Deletion was committed, but cleanup is pending recovery. Keep this library available and retry after recovery.");
      } else if (result.errors.length > 0) {
        toasts.error(`${result.errors.length} file${result.errors.length === 1 ? "" : "s"} could not be deleted. Review the remaining trash items and retry.`);
      } else {
        toasts.success(`${result.db_records_deleted} photo${result.db_records_deleted === 1 ? "" : "s"} permanently deleted.`);
      }
      await load();
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    } finally {
      if (mounted) actionBusy = false;
    }
  }

  onMount(() => {
    mounted = true;
    const updateColumns = () => {
      if (scrollEl) galleryColumns = Math.max(1, Math.floor((scrollEl.clientWidth - 8) / 180));
    };
    updateColumns();
    const ro = scrollEl ? new ResizeObserver(updateColumns) : null;
    if (scrollEl && ro) ro.observe(scrollEl);
    const detachVirtual = galleryVirtual.attach();
    load();
    return () => {
      detachVirtual();
      ro?.disconnect();
      mounted = false;
      loadSeq += 1;
    };
  });
</script>

<PageHeader title="Trash">
  {#if stats}
    <span class="count mono">
      {stats.count}<span class="muted"> · {(stats.total_size / 1024 / 1024).toFixed(0)} MB</span>
    </span>
  {/if}
  <button onclick={restore} disabled={selectedTrashCount === 0 || actionBusy}>
    Restore <span class="mono">{selectedTrashCount}</span>
  </button>
  <button class="danger" onclick={deleteForever} disabled={selectedTrashCount === 0 || actionBusy}>Delete forever</button>
  <button class="danger" onclick={emptyTrash} disabled={items.length === 0 || actionBusy}>Empty</button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}
{#if deleteErrors.length > 0}
  <details class="delete-errors">
    <summary>{deleteErrors.length} deletion issue{deleteErrors.length === 1 ? "" : "s"} — show details</summary>
    <ul>{#each deleteErrors as detail}<li class="mono">{detail}</li>{/each}</ul>
  </details>
{/if}

<div class="page" bind:this={scrollEl} onscroll={onTrashScroll} use:marqueeSelect={{ getAllIds: () => visibleIds }}>
  {#if items.length === 0}
    <div class="empty">
      <p>Nothing in trash. A clean shelf.</p>
    </div>
  {:else}
    <div class="virtual-gallery" style={`height: ${galleryVirtual.totalHeight}px`}>
      {#each Array.from({ length: Math.max(0, galleryVirtual.last - galleryVirtual.first) }, (_, i) => galleryVirtual.first + i) as row}
      <div class="pv-photo-grid virtual-row" style={`top: ${galleryVirtual.offsets[row]}px`}>
      {#each items.slice(row * galleryColumns, (row + 1) * galleryColumns) as t (t.photo_id)}
        <button
          class="pv-photo-cell trash-cell"
          class:sel={selection.has(t.photo_id)}
          data-photo-id={t.photo_id}
          use:thumbnailOnVisible={{
            id: t.photo_id,
            thumbnailPath: t.thumbnail_path,
            onReady: (path) => patchThumbnail(t.photo_id, path),
          }}
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
      {/each}
    </div>
    {#if loadingMore}
      <p class="loading-more mono">Loading more…</p>
    {/if}
  {/if}
</div>

<style>
  .page { padding: var(--s-4) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .virtual-gallery { position: relative; min-height: 1px; }
  .virtual-row { position: absolute; inset-inline: 0; height: 180px; }
  .count { font-size: var(--t-sm); color: var(--ink); }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
  }
  .empty p { color: var(--ink-muted); font-style: italic; }
  .loading-more {
    margin: var(--s-4) 0 0;
    text-align: center;
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
  .trash-cell {
    padding: 0;
    border: 0;
    cursor: pointer;
  }
  .trash-cell.sel { box-shadow: inset 0 0 0 3px var(--accent); }
  .trash-cell > img { opacity: 0.55; transition: opacity var(--t-fast) var(--ease); }
  .trash-cell.sel > img { opacity: 1; }
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
  .trash-cell.sel .check { opacity: 1; transform: scale(1); }
</style>
