<script lang="ts">
  import { onMount } from "svelte";
  import { duplicates } from "../lib/api/all";
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

<PageHeader
  num="07"
  label="DUPLICATES"
  title="The same photo, twice."
  subtitle="Both byte-identical copies and visual near-misses. Pick which to keep — the rest go to trash."
>
  <span class="waste mono">
    {(wasted / 1024 / 1024).toFixed(0)} <span class="muted">MB potentially wasted</span>
  </span>
  <button class="primary" onclick={run} disabled={running}>
    {running ? "Scanning…" : "Scan for duplicates"}
  </button>
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if groups.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NONE FOUND</span>
      <p class="quiet">No duplicates yet. Press scan to look — it takes a moment for big libraries.</p>
    </div>
  {:else}
    <ul class="list stagger">
      {#each groups as g, i (g.id)}
        <li style="--i: {i}">
          <a href="#/duplicate?id={g.id}">
            <span class="num mono">№&nbsp;{String(g.id).padStart(3, "0")}</span>
            <span class="title">Group of <strong>{g.member_count}</strong></span>
            <span class="arrow">→</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .page { padding: var(--s-6) var(--s-7); flex: 1; overflow-y: auto; }
  .waste {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
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
  .list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  .list a {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--s-4);
    padding: var(--s-4) var(--s-5);
    background: var(--bg-card);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    color: inherit;
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease),
                transform var(--t-fast) var(--ease);
  }
  .list a:hover {
    background: var(--bg-elev);
    border-color: var(--line);
    text-decoration: none;
    transform: translateX(3px);
  }
  .num { font-size: var(--t-xs); color: var(--ink-faint); letter-spacing: 0.06em; }
  .title { font-family: var(--font-display); font-size: var(--t-lg); }
  .arrow { color: var(--accent); font-family: var(--font-mono); }
</style>
