<script lang="ts">
  import { onMount } from "svelte";
  import { bursts } from "../lib/api/all";
  import PageHeader from "../lib/components/PageHeader.svelte";

  let groups = $state<Awaited<ReturnType<typeof bursts.list>>>([]);
  let running = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try { groups = await bursts.list(); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function run() {
    running = true;
    try { await bursts.run(); }
    catch (e) { error = JSON.stringify(e); running = false; }
  }

  onMount(load);

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleString("en", {
      day: "numeric", month: "short", year: "numeric",
      hour: "numeric", minute: "2-digit",
    });
  }
</script>

<PageHeader
  num="08"
  label="BURSTS"
  title="Rapid-fire moments."
  subtitle="Series of nearly-identical photos taken seconds apart. Keep the sharpest, let the rest go."
>
  <button class="primary" onclick={run} disabled={running}>
    {running ? "Detecting…" : "Detect bursts"}
  </button>
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if groups.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NONE DETECTED</span>
      <p class="quiet">No burst groups yet. Run detection — it'll look for shots taken in quick succession.</p>
    </div>
  {:else}
    <ul class="list stagger">
      {#each groups as g, i (g.id)}
        <li style="--i: {i}">
          <a href="#/burst?id={g.id}">
            <span class="when">{fmtTime(g.start_time)}</span>
            <span class="size mono">{g.photo_count} shots</span>
            <span class="arrow">→</span>
          </a>
        </li>
      {/each}
    </ul>
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
  .list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  .list a {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: var(--s-4);
    padding: var(--s-4) var(--s-5);
    background: var(--bg-card);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    color: inherit;
    transition: background var(--t-fast) var(--ease),
                transform var(--t-fast) var(--ease);
  }
  .list a:hover {
    background: var(--bg-elev);
    border-color: var(--line);
    text-decoration: none;
    transform: translateX(3px);
  }
  .when {
    font-family: var(--font-display);
    font-size: var(--t-base);
    font-weight: 500;
  }
  .size { font-size: var(--t-xs); color: var(--ink-muted); }
  .arrow { color: var(--accent); }
</style>
