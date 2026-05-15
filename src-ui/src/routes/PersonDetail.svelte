<script lang="ts">
  import { onMount } from "svelte";
  import { people, trash, type FaceDetailDto } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
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
  let editing = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let showMergeDialog = $state(false);
  let showReassignDialog = $state<number | null>(null);
  let showKSimilarDialog = $state(false);
  let unconfirmedFaces = $state<FaceDetailDto[]>([]);
  let verifyBusy = $state(false);

  async function notAPerson() {
    if (!person) return;
    const label = person.name ?? `Person ${person.id}`;
    if (!confirm(`Remove ${label} as a person?\n\nFaces stay in their photos and may be re-clustered next time face detection runs.`)) return;
    try {
      await people.delete(person.id);
      window.location.hash = "/people";
    } catch (e) { error = JSON.stringify(e); }
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
      person = await people.get(id);
      editName = person.name ?? "";
      const [photoPage, facePage] = await Promise.all([
        people.photosByPerson(id),
        people.faceList(id, "unconfirmed", null, 12),
      ]);
      photos = photoPage.items;
      unconfirmedFaces = facePage.items;
      browseContext.set(`person:${id}`, photos.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
  }

  async function confirmFace(faceId: number) {
    if (verifyBusy) return;
    verifyBusy = true;
    try {
      await people.faceConfirm(faceId);
      unconfirmedFaces = unconfirmedFaces.filter((f) => f.face_id !== faceId);
    } catch (e) { error = String(e); }
    finally { verifyBusy = false; }
  }

  async function rejectFace(faceId: number) {
    if (verifyBusy) return;
    verifyBusy = true;
    try {
      await people.faceReject(faceId);
      unconfirmedFaces = unconfirmedFaces.filter((f) => f.face_id !== faceId);
    } catch (e) { error = String(e); }
    finally { verifyBusy = false; }
  }

  async function reassignFace(faceId: number) {
    showReassignDialog = faceId;
  }

  async function save() {
    if (!person) return;
    try {
      person = await people.rename(id, editName.trim() || null);
      editing = false;
    } catch (e) { error = JSON.stringify(e); }
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
          {#if p.face_count != null}
            <span class="mono dim">{p.face_count} faces</span>
          {/if}
        {/snippet}
        {#snippet actions()}
          {#if editing}
            <button class="primary" onclick={save}>Save</button>
            <button class="ghost" onclick={() => (editing = false)}>Cancel</button>
          {:else}
            <button class="ghost" onclick={() => (editing = true)}>
              {p.name ? "Rename" : "Name them"}
            </button>
            <button class="ghost" onclick={() => (showMergeDialog = true)}>Merge…</button>
            <button class="ghost" onclick={() => (showKSimilarDialog = true)}>
              <Search size={14} strokeWidth={1.75} />
              Find more like this
            </button>
            <button class="danger" onclick={notAPerson}>Not a person</button>
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

<div class="page-scroll">
  <div class="pv-photo-grid">
    {#each photos as p (p.id)}
      <a
        class="pv-photo-cell"
        class:selected={selection.has(p.id)}
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

{#if showMergeDialog && person}
  <MergePersonDialog
    source={person}
    onclose={() => (showMergeDialog = false)}
    onsuccess={(merged) => {
      window.location.hash = `/person?id=${merged.id}`;
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
    onclose={() => (showKSimilarDialog = false)}
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
  .dim { color: var(--ink-faint); }

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
