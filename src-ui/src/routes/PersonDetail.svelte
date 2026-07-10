<script lang="ts">
  import { onMount, tick } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { people, trash, type FaceDetailDto } from "../lib/api/all";
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
  import MergePersonDialog from "../lib/components/MergePersonDialog.svelte";
  import ReassignFaceDialog from "../lib/components/ReassignFaceDialog.svelte";
  import KSimilarDialog from "../lib/components/KSimilarDialog.svelte";
  import FaceCell from "../lib/components/FaceCell.svelte";
  import { Check, Search } from "lucide-svelte";
  import type { PersonDto, PhotoSummaryDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let person = $state<PersonDto | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let personPhotoIds = $state<number[]>([]);
  let editing = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let showMergeDialog = $state(false);
  let showReassignDialog = $state<number | null>(null);
  let showKSimilarDialog = $state(false);
  let unconfirmedFaces = $state<FaceDetailDto[]>([]);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);
  let loadingMore = $state(false);
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let verifyBusy = $state(false);
  let actionBusy = $state(false);
  let loadSeq = 0;
  let mounted = true;
  let scrollRestored = false;
  const scrollStorageKey = $derived(`smriti:person-scroll:${id}`);
  const selectedVisibleIds = $derived(selection.listIn(photos.map((p) => p.id)));
  const ALL_IDS_NAV_LIMIT = 5000;

  async function notAPerson() {
    if (!person || actionBusy) return;
    const seq = loadSeq;
    const personId = id;
    const label = person.name ?? `Person ${person.id}`;
    if (!confirm(`Remove ${label} as a person?\n\nFaces stay in their photos and may be re-clustered next time face detection runs.`)) return;
    try {
      actionBusy = true;
      await people.delete(person.id);
      if (!mounted || seq !== loadSeq || personId !== id) return;
      window.location.hash = "/people";
    } catch (e) { if (mounted && seq === loadSeq && personId === id) error = commandErrorMessage(e); }
    finally { if (mounted) actionBusy = false; }
  }

  function onCellClick(e: MouseEvent, photoId: number) {
    const ids = personPhotoIds.length > 0 ? personPhotoIds : photos.map((p) => p.id);
    const handled = handleCellClick(e, photoId, ids);
    if (!handled) browseContext.set(`person:${id}`, ids);
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
    const personId = id;
    const dropSet = new Set(ids);
    const snapshot = photos
      .map((p, idx) => ({ idx, photo: p }))
      .filter((e) => dropSet.has(e.photo.id));
    const idSnapshot = personPhotoIds
      .map((photoId, idx) => ({ idx, photoId }))
      .filter((e) => dropSet.has(e.photoId));
    try {
      actionBusy = true;
      const result = await trash.trashPhotos(ids);
      if (!mounted || seq !== loadSeq || personId !== id) return;
      if (result.count === 0) {
        toasts.info("No selected photos needed trashing");
        return;
      }
      photoVisibility.markTrashed(ids);
      photos = photos.filter((p) => !dropSet.has(p.id));
      personPhotoIds = personPhotoIds.filter((photoId) => !dropSet.has(photoId));
      browseContext.remove(ids);
      selection.clear();
      toasts.undoable(
        `${result.count} ${result.count === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          if (!mounted || personId !== id) return;
          photoVisibility.markRestored(ids);
          const next = photos.slice();
          for (const e of snapshot) {
            const at = Math.min(e.idx, next.length);
            next.splice(at, 0, e.photo);
          }
          photos = next;
          const nextIds = personPhotoIds.slice();
          for (const e of idSnapshot) {
            if (!nextIds.includes(e.photoId)) {
              const at = Math.min(e.idx, nextIds.length);
              nextIds.splice(at, 0, e.photoId);
            }
          }
          personPhotoIds = nextIds;
          browseContext.set(`person:${personId}`, nextIds);
        },
      );
    } catch (e) {
      if (mounted && seq === loadSeq && personId === id) toasts.error(`Couldn't move to trash: ${commandErrorMessage(e)}`);
    } finally {
      if (mounted) actionBusy = false;
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
    return () => {
      saveScroll();
      mounted = false;
      loadSeq += 1;
      window.removeEventListener("keydown", onGlobalKey);
    };
  });

  async function load() {
    const seq = ++loadSeq;
    const personId = id;
    error = null;
    scrollRestored = false;
    loadingMore = false;
    verifyBusy = false;
    actionBusy = false;
    person = null;
    photos = [];
    personPhotoIds = [];
    unconfirmedFaces = [];
    showAddDialog = false;
    showMergeDialog = false;
    showReassignDialog = null;
    showKSimilarDialog = false;
    nextCursor = null;
    hasMore = false;
    selection.clear();
    try {
      const [nextPerson, photoPage, facePage] = await Promise.all([
        people.get(personId),
        people.photosByPerson(personId, null, 500),
        people.faceList(personId, "unconfirmed", null, 12),
      ]);
      if (!mounted || seq !== loadSeq) return;
      let allIds = photoPage.items.map((p) => p.id);
      if (nextPerson.photo_count <= ALL_IDS_NAV_LIMIT) {
        allIds = await people.photoIds(personId);
        if (!mounted || seq !== loadSeq) return;
      }
      person = nextPerson;
      editName = nextPerson.name ?? "";
      photos = photoPage.items;
      personPhotoIds = allIds;
      nextCursor = photoPage.next_cursor;
      hasMore = photoPage.has_more;
      unconfirmedFaces = facePage.items;
      browseContext.set(`person:${personId}`, allIds);
      void restoreSavedScroll();
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function loadMorePhotos() {
    if (!mounted || loadingMore || !hasMore || !nextCursor) return;
    const seq = loadSeq;
    const personId = id;
    const cursor = nextCursor;
    loadingMore = true;
    try {
      const page = await people.photosByPerson(personId, cursor, 500);
      if (!mounted || seq !== loadSeq || personId !== id) return;
      photos = photos.concat(page.items);
      nextCursor = page.next_cursor;
      hasMore = page.has_more;
      browseContext.extend(page.items.map((p) => p.id));
    } catch (e) {
      if (mounted && seq === loadSeq && personId === id) {
        toasts.error(`Couldn't load more photos: ${commandErrorMessage(e)}`);
      }
    } finally {
      if (mounted && seq === loadSeq && personId === id) loadingMore = false;
    }
  }

  function onPhotoScroll() {
    saveScroll();
    if (!scrollEl || !hasMore || loadingMore) return;
    const remaining = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    if (remaining < 900) void loadMorePhotos();
  }

  async function confirmFace(faceId: number) {
    if (verifyBusy) return;
    const seq = loadSeq;
    const personId = id;
    verifyBusy = true;
    try {
      await people.faceConfirm(faceId);
      if (!mounted || seq !== loadSeq || personId !== id) return;
      unconfirmedFaces = unconfirmedFaces.filter((f) => f.face_id !== faceId);
    } catch (e) { if (mounted && seq === loadSeq && personId === id) error = commandErrorMessage(e); }
    finally { if (mounted && seq === loadSeq && personId === id) verifyBusy = false; }
  }

  async function rejectFace(faceId: number) {
    if (verifyBusy) return;
    const seq = loadSeq;
    const personId = id;
    verifyBusy = true;
    try {
      await people.faceReject(faceId);
      if (!mounted || seq !== loadSeq || personId !== id) return;
      unconfirmedFaces = unconfirmedFaces.filter((f) => f.face_id !== faceId);
    } catch (e) { if (mounted && seq === loadSeq && personId === id) error = commandErrorMessage(e); }
    finally { if (mounted && seq === loadSeq && personId === id) verifyBusy = false; }
  }

  async function reassignFace(faceId: number) {
    showReassignDialog = faceId;
  }

  async function save() {
    if (!person || actionBusy) return;
    const seq = loadSeq;
    const personId = id;
    try {
      actionBusy = true;
      const renamed = await people.rename(personId, editName.trim() || null);
      if (!mounted || seq !== loadSeq || personId !== id) return;
      person = renamed;
      editing = false;
    } catch (e) { if (mounted && seq === loadSeq && personId === id) error = commandErrorMessage(e); }
    finally { if (mounted) actionBusy = false; }
  }

  function onEditKey(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); save(); }
    else if (e.key === "Escape") {
      e.preventDefault();
      editName = person?.name ?? "";
      editing = false;
    }
  }

  $effect(() => { void id; load(); });
</script>

{#if person}
  {@const p = person}
  <div class="hero">
    <div class="portrait">
      {#if p.representative_thumbnail_path}
        <img src={thumbUrl(libraryStore.driveRoot, p.representative_thumbnail_path) ?? ""} alt="" />
      {/if}
    </div>
    <div class="hero-body">
      <DetailHeader backHref="#/people" backLabel="People">
        {#snippet title()}
          {#if editing}
            <!-- svelte-ignore a11y_autofocus -->
            <input bind:value={editName} onkeydown={onEditKey} placeholder="Name them" autofocus />
          {:else}
            <h1>{p.name ?? "Unnamed"}</h1>
          {/if}
        {/snippet}
        {#snippet subtitle()}
          <span class="mono">{p.photo_count} photos</span>
        {/snippet}
        {#snippet actions()}
          {#if editing}
            <button class="primary" onclick={save} disabled={actionBusy}>Save</button>
            <button class="ghost" onclick={() => (editing = false)} disabled={actionBusy}>Cancel</button>
          {:else}
            <button class="ghost" onclick={() => (editing = true)} disabled={actionBusy}>
              {p.name ? "Rename" : "Name them"}
            </button>
            <button class="ghost" onclick={() => (showMergeDialog = true)} disabled={actionBusy}>Merge…</button>
            <button class="ghost" onclick={() => (showKSimilarDialog = true)} disabled={actionBusy}>
              <Search size={14} strokeWidth={1.75} />
              Find more like this
            </button>
            <button class="danger" onclick={notAPerson} disabled={actionBusy}>Not a person</button>
          {/if}
        {/snippet}
      </DetailHeader>
    </div>
  </div>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

{#if unconfirmedFaces.length > 0}
  <div class="verify-strip">
    <span class="verify-header">
      <span class="label">Verify these</span>
      <span class="hint">These faces might not belong to this person</span>
    </span>
    <div class="verify-scroll">
      {#each unconfirmedFaces as f (f.face_id)}
        <FaceCell
          face={{ face_id: f.face_id, user_confirmed: f.user_confirmed, thumbnail_path: f.thumbnail_path }}
          onConfirm={confirmFace}
          onReject={rejectFace}
          onReassign={reassignFace}
          busy={verifyBusy}
        />
      {/each}
    </div>
  </div>
{/if}

<div class="page-scroll" bind:this={scrollEl} onscroll={onPhotoScroll} use:marqueeSelect={{ getAllIds: () => photos.map((p) => p.id) }}>
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

{#if showMergeDialog && person}
  <MergePersonDialog
    source={person}
    onclose={() => (showMergeDialog = false)}
    onsuccess={(merged) => {
      history.replaceState(null, "", `#/person?id=${merged.id}`);
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    }}
  />
{/if}

{#if showReassignDialog != null}
  <ReassignFaceDialog
    faceId={showReassignDialog}
    onclose={() => (showReassignDialog = null)}
    onsuccess={() => { showReassignDialog = null; load(); }}
  />
{/if}

{#if showKSimilarDialog}
  <KSimilarDialog
    clusterId={id}
    onclose={() => {
      showKSimilarDialog = false;
      load();
    }}
  />
{/if}

<style>
  .hero {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: stretch;
    border-bottom: 1px solid var(--line-soft);
  }
  .hero-body :global(.detail-header) {
    border-bottom: none;
  }
  .portrait {
    width: 88px;
    height: 88px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg-card);
    border: 1px solid var(--line);
    margin: var(--s-4) 0 var(--s-4) var(--s-7);
    align-self: center;
    flex-shrink: 0;
  }
  .portrait img { width: 100%; height: 100%; object-fit: cover; }

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

  .verify-strip {
    padding: var(--s-3) var(--s-7);
    border-bottom: 1px solid var(--line-soft);
    background: color-mix(in oklab, var(--bg-card) 50%, var(--bg));
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .verify-header {
    display: flex;
    align-items: baseline;
    gap: var(--s-3);
  }
  .verify-header .label {
    font-family: var(--font-mono);
    font-size: var(--t-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--accent);
    font-weight: 600;
  }
  .verify-header .hint {
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
  .verify-scroll {
    display: flex;
    gap: var(--s-2);
    overflow-x: auto;
    padding-bottom: 4px;
  }

  @media (max-width: 720px) {
    .hero { grid-template-columns: 1fr; }
    .portrait {
      margin: var(--s-4) auto 0;
      width: 72px; height: 72px;
    }
  }
</style>
