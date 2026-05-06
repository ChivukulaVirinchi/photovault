<script lang="ts">
  import { onMount } from "svelte";
  import { bursts } from "../lib/api/all";

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
</script>

<main class="bursts">
  <header>
    <h2>Bursts</h2>
    <button onclick={run} disabled={running}>{running ? "Detecting…" : "Detect bursts"}</button>
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if groups.length === 0}
    <p class="muted">No burst groups detected.</p>
  {:else}
    <ul>
      {#each groups as g}
        <li><a href="#/burst?id={g.id}">
          <strong>{new Date(g.start_time).toLocaleString()}</strong>
          <span class="muted">{g.photo_count} shots</span>
        </a></li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  .bursts { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 16px; }
  h2 { margin: 0; }
  ul { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  li a { display: flex; gap: 14px; padding: 12px 14px; background: #131316; border-radius: 6px; color: inherit; }
  li a:hover { background: #1a1a1f; text-decoration: none; }
</style>
