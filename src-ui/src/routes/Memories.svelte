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

<PageHeader
  num="04"
  label="MEMORIES"
  title="A look back at this day."
  subtitle="On {todayLabel()}, here's what your past looked like."
/>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if cards.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NOTHING TODAY</span>
      <p class="quiet">
        Your library may be too young — Memories surface once you have at least three months of photographs.
      </p>
    </div>
  {:else}
    <div class="cards stagger">
      {#each cards as c, i (c.id)}
        <a class="card" href="#/memory?id={c.id}" style="--i: {i}">
          {#if c.hero_thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, c.hero_thumbnail_path) ?? ""} alt="" />
          {/if}
          <div class="overlay">
            <div class="caption">
              <span class="kind mono">{c.kind.replaceAll("_", " ").toUpperCase()}</span>
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
    max-width: 42ch;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: var(--s-4);
  }
  .card {
    aspect-ratio: 3 / 4;
    background: var(--bg-card);
    border-radius: var(--r-lg);
    overflow: hidden;
    color: inherit;
    position: relative;
    box-shadow: var(--shadow-soft);
    transition: transform var(--t-base-d) var(--ease),
                box-shadow var(--t-base-d) var(--ease);
  }
  .card:hover {
    text-decoration: none;
    transform: translateY(-4px);
    box-shadow: var(--shadow-lift);
  }
  .card img { width: 100%; height: 100%; object-fit: cover; transition: transform var(--t-slow) var(--ease); }
  .card:hover img { transform: scale(1.04); }
  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    padding: var(--s-5);
    background: linear-gradient(
      to top,
      rgba(0,0,0,0.85) 0%,
      rgba(0,0,0,0.4) 35%,
      transparent 70%
    );
  }
  .caption { display: flex; flex-direction: column; gap: 6px; color: var(--ink); }
  .kind {
    font-size: 9px;
    color: var(--accent-warm);
    letter-spacing: 0.18em;
  }
  h3 {
    font-family: var(--font-display);
    font-size: var(--t-2xl);
    font-weight: 500;
    color: white;
    line-height: 1.1;
    font-variation-settings: "opsz" 60, "SOFT" 50;
  }
  .count {
    font-size: var(--t-xs);
    color: rgba(255,255,255,0.7);
  }
</style>
