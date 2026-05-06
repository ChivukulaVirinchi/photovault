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
    <ul class="list">
      {#each groups as g (g.id)}
        <li>
          <a href="#/duplicate?id={g.id}">
            <span class="title">Group of <strong>{g.member_count}</strong></span>
            <span class="arrow" aria-hidden="true">→</span>
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
  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 720px;
  }
  .list a {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    justify-content: space-between;
    padding: var(--s-3) var(--s-4);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: inherit;
    text-decoration: none;
    transition: border-color var(--t-fast) var(--ease),
                background var(--t-fast) var(--ease);
  }
  .list a:hover {
    background: var(--bg-card);
    border-color: var(--accent);
  }
  .title {
    font-size: var(--t-base);
    color: var(--ink);
  }
  .title strong { font-weight: 600; }
  .arrow { color: var(--accent); }
</style>
