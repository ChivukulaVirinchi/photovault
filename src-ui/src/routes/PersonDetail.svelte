<script lang="ts">
  import { people } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
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

<div class="masthead">
  <a class="back" href="#/people">← People</a>
  {#if person}
    <div class="portrait">
      {#if person.representative_thumbnail_path}
        <img src={thumbUrl(libraryStore.driveRoot, person.representative_thumbnail_path) ?? ""} alt="" />
      {/if}
    </div>
    <div class="info">
      <span class="eyebrow">
        <span class="num">№&nbsp;{String(person.id).padStart(3, "0")}</span>
        <span class="ornament"></span>
        <span>PROFILE</span>
      </span>
      {#if editing}
        <input bind:value={editName} placeholder="Name them" autofocus />
        <div class="row">
          <button class="primary" onclick={save}>Save</button>
          <button class="ghost" onclick={() => (editing = false)}>Cancel</button>
        </div>
      {:else}
        <h1>{person.name ?? "Unnamed"}</h1>
        <p class="subtitle">
          {person.photo_count} photos · {person.face_count ?? "?"} faces detected
          <button class="rename" onclick={() => (editing = true)}>rename</button>
        </p>
      {/if}
    </div>
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

<div class="grid stagger">
  {#each photos as p, i (p.id)}
    <a class="cell" href="#/photo?id={p.id}" style="--i: {Math.min(i, 30)}">
      {#if p.thumbnail_path}
        <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
      {/if}
    </a>
  {/each}
</div>

<style>
  .masthead {
    padding: var(--s-7) var(--s-7) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: var(--s-5);
    align-items: center;
  }
  .back {
    font-family: var(--font-mono);
    font-size: var(--t-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-muted);
    align-self: flex-start;
    margin-top: 4px;
  }
  .portrait {
    width: 120px; height: 120px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg-card);
    box-shadow: var(--shadow-lift);
    border: 1px solid var(--line);
  }
  .portrait img { width: 100%; height: 100%; object-fit: cover; }
  .info { display: flex; flex-direction: column; gap: var(--s-2); }
  h1 { font-size: var(--t-3xl); }
  input { max-width: 360px; }
  .row { display: flex; gap: var(--s-2); }
  .rename {
    background: transparent;
    border: none;
    color: var(--accent);
    padding: 2px 8px;
    font-size: var(--t-sm);
    border-bottom: 1px solid transparent;
    cursor: pointer;
  }
  .rename:hover { border-bottom-color: var(--accent); background: transparent; }

  .grid {
    padding: var(--s-5) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    transition: transform var(--t-fast) var(--ease);
  }
  .cell:hover { transform: scale(1.018); }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
