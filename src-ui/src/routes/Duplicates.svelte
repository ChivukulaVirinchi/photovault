<script lang="ts">
  import { onMount } from "svelte";
  import { duplicates } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";

  let groups = $state<Awaited<ReturnType<typeof duplicates.list>>>([]);
  let wasted = $state(0);
  let running = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try {
      groups = await duplicates.list();
      const w = await duplicates.wastedSpace();
      wasted = w.bytes;
    } catch (e) { error = JSON.stringify(e); }
  }

  async function run() {
    running = true;
    try { await duplicates.run(true); }
    catch (e) { error = JSON.stringify(e); running = false; }
  }

  onMount(load);
</script>

<PageHeader title="Duplicates">
  <span class="waste mono">
    {(wasted / 1024 / 1024).toFixed(0)}<span class="muted"> MB potentially wasted</span>
  </span>
  <button class="primary" onclick={run} disabled={running}>
    {running ? "Scanning…" : "Scan"}
  </button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if groups.length === 0}
    <div class="empty">
      <p>No duplicates yet. Press scan — it takes a moment for big libraries.</p>
      <button class="primary" onclick={run} disabled={running}>
        {running ? "Scanning…" : "Scan"}
      </button>
    </div>
  {:else}
    <ul class="grid">
      {#each groups as g (g.id)}
        <li>
          <a href="#/duplicate?id={g.id}" aria-label="Duplicate group of {g.member_count}">
            {#if g.cover_thumbnail_path}
              <img
                src={thumbUrl(libraryStore.driveRoot, g.cover_thumbnail_path) ?? ""}
                alt=""
                loading="lazy"
                decoding="async"
                onerror={(e) => ((e.target as HTMLImageElement).style.display = "none")}
              />
            {/if}
            <span class="badge mono">{g.member_count}×</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .waste {
    font-size: var(--t-sm);
    color: var(--ink);
  }
  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    align-items: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.55; }
  .grid {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--s-3);
  }
  .grid li {
    aspect-ratio: 1;
    position: relative;
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .grid li:hover {
    border-color: var(--accent);
    box-shadow: 0 6px 22px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .grid a {
    position: absolute;
    inset: 0;
    display: block;
    text-decoration: none;
    color: inherit;
  }
  .grid img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .grid .badge {
    position: absolute;
    top: var(--s-2);
    right: var(--s-2);
    background: rgba(0, 0, 0, 0.66);
    color: #fff;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: var(--t-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
    z-index: 1;
  }
</style>
