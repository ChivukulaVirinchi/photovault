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

<PageHeader title="People">
  <span class="count mono">{clusters.length}<span class="muted"> people</span></span>
  <button class="primary" onclick={startFaceProcessing} disabled={processing}>
    {processing ? "Processing…" : "Find faces"}
  </button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if clusters.length === 0}
    <div class="empty">
      <p>No faces yet. Run face detection to start finding the people in your library.</p>
      <button class="primary" onclick={startFaceProcessing} disabled={processing}>
        {processing ? "Processing…" : "Find faces"}
      </button>
    </div>
  {:else}
    <div class="grid">
      {#each clusters as c (c.id)}
        <a class="card" href="#/person?id={c.id}">
          <div class="frame">
            {#if c.representative_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="placeholder small">no face</span>
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
  .page {
    padding: var(--s-5) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
  }
  .count {
    font-size: var(--t-sm);
    color: var(--ink);
  }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    align-items: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p {
    color: var(--ink-soft);
    line-height: 1.55;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: var(--s-6) var(--s-5);
  }
  .card {
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    text-align: center;
    text-decoration: none;
  }
  .frame {
    aspect-ratio: 1;
    min-width: 0;
    background: var(--bg-card);
    border-radius: 50%;
    overflow: hidden;
    position: relative;
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .card:hover .frame {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .frame img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .placeholder {
    color: var(--ink-faint);
  }
  .caption {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 0 var(--s-1);
  }
  .name {
    font-family: var(--font-display);
    font-size: var(--t-base);
    font-weight: 500;
    font-variation-settings: "opsz" 18;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption .count {
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }
  .small { font-size: var(--t-xs); }
</style>
