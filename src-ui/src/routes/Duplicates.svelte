<script lang="ts">
  import { onMount } from "svelte";
  import { duplicates } from "../lib/api/all";

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
    try {
      await duplicates.run(true);
    } catch (e) { error = JSON.stringify(e); running = false; }
  }

  onMount(load);
</script>

<main class="dups">
  <header>
    <h2>Duplicates</h2>
    <span class="muted">{(wasted / 1024 / 1024).toFixed(1)} MB potentially wasted</span>
    <button onclick={run} disabled={running}>{running ? "Scanning…" : "Scan now"}</button>
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if groups.length === 0}
    <p class="muted">No duplicate groups detected.</p>
  {:else}
    <ul>
      {#each groups as g}
        <li>
          <a href="#/duplicate?id={g.id}">
            <strong>Group #{g.id}</strong>
            <span class="muted">{g.member_count} copies</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  .dups { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 16px; }
  h2 { margin: 0; }
  ul { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  li a { display: flex; gap: 14px; padding: 12px 14px; background: #131316; border-radius: 6px; color: inherit; }
  li a:hover { background: #1a1a1f; text-decoration: none; }
</style>
