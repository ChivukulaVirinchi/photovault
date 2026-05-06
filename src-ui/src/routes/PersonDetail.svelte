<script lang="ts">
  import { onMount } from "svelte";
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
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  async function save() {
    if (!person) return;
    try {
      person = await people.rename(id, editName.trim() || null);
      editing = false;
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  $effect(() => {
    void id;
    load();
  });
</script>

<main class="detail">
  <header>
    <a href="#/people">← People</a>
    {#if person}
      {#if editing}
        <input bind:value={editName} placeholder="Name" />
        <button onclick={save}>Save</button>
        <button onclick={() => (editing = false)} class="ghost">Cancel</button>
      {:else}
        <h2>{person.name ?? "Unnamed"}</h2>
        <button onclick={() => (editing = true)}>Rename</button>
      {/if}
      <span class="muted">{person.photo_count} photos</span>
    {/if}
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="grid">
    {#each photos as p}
      <a class="cell" href="#/photo?id={p.id}">
        {#if p.thumbnail_path}
          <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
        {/if}
      </a>
    {/each}
  </div>
</main>

<style>
  .detail { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 20px; }
  h2 { margin: 0; }
  input { flex: 1; max-width: 300px; }
  .ghost { background: transparent; border: 1px solid #2a2a2d; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    background: #131316;
    border-radius: 4px;
    overflow: hidden;
  }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
</style>
