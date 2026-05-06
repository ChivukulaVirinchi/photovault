<script lang="ts">
  import { onMount } from "svelte";
  import { memories } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { MemoryCard } from "../lib/api/all";

  let cards = $state<MemoryCard[]>([]);
  let error = $state<string | null>(null);

  async function load() {
    try { cards = await memories.today(); }
    catch (e) { error = JSON.stringify(e); }
  }

  function todayLabel(): string {
    return new Date().toLocaleDateString("en", {
      weekday: "long", day: "numeric", month: "long",
    });
  }

  onMount(load);
</script>

<PageHeader title="Memories">
  <span class="when mono">{todayLabel()}</span>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if cards.length === 0}
    <div class="empty">
      <p>
        Nothing today. Memories surface once your library has roughly three months of photos to look back on.
      </p>
    </div>
  {:else}
    <div class="cards">
      {#each cards as c (c.id)}
        <a class="card" href="#/memory?id={c.id}">
          {#if c.hero_thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, c.hero_thumbnail_path) ?? ""} alt="" />
          {/if}
          <div class="overlay">
            <div class="caption">
              <span class="kind mono">{c.kind.replaceAll("_", " ")}</span>
              <h3>{c.title}</h3>
              <span class="count mono">{c.photo_count} photos</span>
            </div>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .when { font-size: var(--t-sm); color: var(--ink-muted); }
  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    max-width: 44ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.6; }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s-4);
  }
  .card {
    aspect-ratio: 3 / 4;
    background: var(--bg-card);
    border-radius: var(--r-md);
    overflow: hidden;
    color: inherit;
    position: relative;
    text-decoration: none;
    transition: box-shadow var(--t-base-d) var(--ease);
    border: 1px solid var(--line);
  }
  .card:hover {
    box-shadow: 0 0 0 2px var(--accent-ghost);
  }
  .card img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform var(--t-slow) var(--ease);
  }
  .card:hover img { transform: scale(1.03); }
  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    padding: var(--s-5);
    background: linear-gradient(
      to top,
      rgba(0, 0, 0, 0.82) 0%,
      rgba(0, 0, 0, 0.35) 38%,
      transparent 72%
    );
  }
  .caption {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: #ebedf0;
  }
  .kind {
    font-size: var(--t-xs);
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-weight: 600;
  }
  h3 {
    font-family: var(--font-display);
    font-size: var(--t-2xl);
    font-weight: 500;
    color: #fff;
    line-height: 1.1;
    font-variation-settings: "opsz" 60;
    margin: 0;
  }
  .count {
    font-size: var(--t-xs);
    color: rgba(235, 237, 240, 0.7);
  }
</style>
