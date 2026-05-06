<script lang="ts">
  import { onMount } from "svelte";
  import { people } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { PersonDto } from "../lib/api/types";

  let clusters = $state<PersonDto[]>([]);
  let processing = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try { clusters = await people.list({ minPhotos: 2 }); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function startFaceProcessing() {
    processing = true;
    try { await people.startProcessing(); }
    catch (e) { error = JSON.stringify(e); processing = false; }
  }

  onMount(load);
</script>

<PageHeader
  num="02"
  label="PEOPLE"
  title="The faces in your life."
  subtitle="Faces are detected on-device, never sent anywhere. Name them and they'll find each other."
>
  <button class="primary" onclick={startFaceProcessing} disabled={processing}>
    {processing ? "Processing…" : "Find faces"}
  </button>
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if clusters.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NO FACES YET</span>
      <p class="quiet">Run face detection to start finding the people in your library.</p>
    </div>
  {:else}
    <div class="grid stagger">
      {#each clusters as c, i (c.id)}
        <a class="card" href="#/person?id={c.id}" style="--i: {i}">
          <div class="frame">
            {#if c.representative_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="muted small">no face</span>
            {/if}
          </div>
          <div class="caption">
            <strong class="name">{c.name ?? "Unnamed"}</strong>
            <span class="count mono">{c.photo_count} photos</span>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-6) var(--s-7); flex: 1; overflow-y: auto; }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
    display: flex; flex-direction: column; gap: var(--s-3); align-items: center;
  }
  .quiet {
    font-family: var(--font-display);
    font-style: italic;
    font-size: var(--t-lg);
    color: var(--ink-soft);
    max-width: 38ch;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: var(--s-5);
  }
  .card {
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    transition: transform var(--t-base-d) var(--ease);
  }
  .card:hover { text-decoration: none; transform: translateY(-3px); }
  .frame {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: 50%;
    overflow: hidden;
    box-shadow: var(--shadow-soft);
    position: relative;
    display: flex; align-items: center; justify-content: center;
    border: 1px solid var(--line);
  }
  .frame::after {
    content: "";
    position: absolute; inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 4px var(--bg-paper), inset 0 0 0 5px var(--line);
    pointer-events: none;
  }
  .frame img { width: 100%; height: 100%; object-fit: cover; }
  .caption {
    text-align: center;
    display: flex; flex-direction: column; gap: 2px;
  }
  .name {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 500;
    font-variation-settings: "opsz" 24;
  }
  .count {
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }
  .small { font-size: var(--t-xs); }
</style>
