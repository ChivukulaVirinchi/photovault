<script lang="ts">
  import { people } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import type { PersonDto, PhotoSummaryDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let person = $state<PersonDto | null>(null);
  let photos = $state<PhotoSummaryDto[]>([]);
  let editing = $state(false);
  let editName = $state("");
  let error = $state<string | null>(null);

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
            <input bind:value={editName} placeholder="Name them" />
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
          {/if}
        {/snippet}
      </DetailHeader>
    </div>
  </div>
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
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
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

  @media (max-width: 720px) {
    .hero { grid-template-columns: 1fr; }
    .portrait {
      margin: var(--s-4) auto 0;
      width: 72px; height: 72px;
    }
  }
</style>
