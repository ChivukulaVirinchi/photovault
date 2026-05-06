<script lang="ts">
  import { onMount } from "svelte";
  import { people, trash } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import MergePersonDialog from "../lib/components/MergePersonDialog.svelte";
  import { Check } from "lucide-svelte";
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
      const page = await people.photosByPerson(id);
      photos = page.items;
      browseContext.set(`person:${id}`, photos.map((p) => p.id));
    } catch (e) { error = JSON.stringify(e); }
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
            <button class="danger" onclick={notAPerson}>Not a person</button>
          {/if}
        {/snippet}
      </DetailHeader>
    </div>
  </div>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="grid">
  {#each photos as p (p.id)}
    <a
      class="cell"
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

  .grid {
    padding: var(--s-4) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    min-width: 0;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    position: relative;
    display: block;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .cell:hover {
    filter: brightness(1.06);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .cell.selected { box-shadow: inset 0 0 0 3px var(--accent); }
  .cell.selected img { filter: brightness(0.85); }
  .cell .check {
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
  .cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .dim { color: var(--ink-faint); }

  @media (max-width: 720px) {
    .hero { grid-template-columns: 1fr; }
    .portrait {
      margin: var(--s-4) auto 0;
      width: 72px; height: 72px;
    }
  }
</style>
