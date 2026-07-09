<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { memories, trash } from "../lib/api/all";
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
  import type { PhotoSummaryDto } from "../lib/api/types";
  import type { MemoryCard } from "../lib/api/all";
  import { slideshow } from "../lib/stores/slideshow.svelte";

  interface Props { id: string }
  let { id }: Props = $props();

  let card = $state<MemoryCard | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let savedAlbumId = $state<number | null>(null);
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let actionBusy = $state(false);
  let loadSeq = 0;
  let mounted = true;
  const scrollStorageKey = $derived(`smriti:memory-scroll:${id}`);
  const selectedVisibleIds = $derived(selection.listIn(photos.map((p) => p.id)));

  function onCellClick(e: MouseEvent, photoId: number) {
    handleCellClick(e, photoId, photos.map((p) => p.id));
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
    const memoryId = id;
    const dropSet = new Set(ids);
    const snapshot = photos
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    try {
      actionBusy = true;
      const result = await trash.trashPhotos(ids);
      if (!mounted || seq !== loadSeq || memoryId !== id) return;
      if (result.count === 0) {
        toasts.info("No selected photos needed trashing");
        return;
      }
      photoVisibility.markTrashed(ids);
      photos = photos.filter((p) => !dropSet.has(p.id));
      browseContext.remove(ids);
      selection.clear();
      toasts.undoable(
        `${result.count} ${result.count === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          if (!mounted || memoryId !== id) return;
          photoVisibility.markRestored(ids);
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
          browseContext.set(`memory:${id}`, photos.map((p) => p.id));
        },
      );
    } catch (e) {
      if (mounted && seq === loadSeq && memoryId === id) toasts.error(`Couldn't move to trash: ${commandErrorMessage(e)}`);
    } finally {
      if (mounted && seq === loadSeq && memoryId === id) actionBusy = false;
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
    mounted = true;
    window.addEventListener("keydown", onGlobalKey);
    return () => {
      mounted = false;
      loadSeq += 1;
      window.removeEventListener("keydown", onGlobalKey);
    };
  });

  $effect(() => {
    if (!scrollEl) return;
    const raw = (() => { try { return sessionStorage.getItem(scrollStorageKey); } catch { return null; } })();
    if (raw) {
      const y = Number(raw);
      if (Number.isFinite(y) && y > 0) requestAnimationFrame(() => { if (mounted && scrollEl) scrollEl.scrollTop = y; });
    }
    const onScroll = () => {
      try { sessionStorage.setItem(scrollStorageKey, String(scrollEl?.scrollTop ?? 0)); } catch {}
    };
    scrollEl.addEventListener("scroll", onScroll, { passive: true });
    return () => scrollEl?.removeEventListener("scroll", onScroll);
  });

  async function load() {
    const seq = ++loadSeq;
    const memoryId = id;
    error = null;
    card = null;
    photos = [];
    savedAlbumId = null;
    showAddDialog = false;
    actionBusy = false;
    selection.clear();
    try {
      const r = await memories.detail(memoryId);
      if (!mounted || seq !== loadSeq) return;
      card = r.card;
      photos = r.photos;
      browseContext.set(`memory:${memoryId}`, photos.map((p) => p.id));
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function saveAsAlbum() {
    if (!card || actionBusy) return;
    const seq = loadSeq;
    const memoryId = id;
    try {
      actionBusy = true;
      const a = await memories.saveAsAlbum(card.id);
      if (mounted && seq === loadSeq && memoryId === id) savedAlbumId = a.id;
    } catch (e) {
      if (mounted && seq === loadSeq && memoryId === id) error = commandErrorMessage(e);
    } finally {
      if (mounted && seq === loadSeq && memoryId === id) actionBusy = false;
    }
  }

  function startMemorySlideshow() {
    if (!card || photos.length === 0) return;
    slideshow.start({
      kind: "memory",
      label: card.title,
      ids: photos.map((p) => p.id),
    });
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
      <button class="ghost icon-action" onclick={startMemorySlideshow} disabled={photos.length === 0} title="Start slideshow" aria-label="Start memory slideshow">
        <Play size={15} strokeWidth={2} />
      </button>
      {#if savedAlbumId}
        <a class="saved-link" href="#/album?id={savedAlbumId}">Saved as album →</a>
      {:else}
        <button class="primary" onclick={saveAsAlbum} disabled={actionBusy}>Save as album</button>
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

{#if selectedVisibleIds.length > 0}
  <SelectionBar
    count={selectedVisibleIds.length}
    onAddToAlbum={() => (showAddDialog = true)}
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
  .icon-action {
    width: 32px;
    height: 32px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

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
