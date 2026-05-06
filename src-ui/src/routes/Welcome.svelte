<script lang="ts">
  import { libraryStore } from "../lib/stores/library.svelte";
</script>

<main class="welcome">
  <h1>PhotoVault</h1>
  <p class="muted">
    Pick a drive or folder to open. PhotoVault stores its index in
    <code>.photovault/</code> on the drive itself — fully portable.
  </p>

  {#if libraryStore.error}
    <p class="error">{libraryStore.error}</p>
  {/if}

  {#if libraryStore.drives.length === 0}
    <p class="muted">No drives detected. Try refreshing.</p>
  {:else}
    <ul class="drives">
      {#each libraryStore.drives as d}
        <li>
          <button
            disabled={libraryStore.loading}
            onclick={() => libraryStore.open(d.path).catch(() => {})}
          >
            <strong>{d.name}</strong>
            <span class="muted">{d.path}</span>
            {#if d.has_photovault_db}<span class="indexed">(indexed)</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="actions">
    <button onclick={() => libraryStore.refresh()}>Refresh drives</button>
  </div>
</main>

<style>
  .welcome {
    max-width: 640px;
    margin: 8vh auto;
    padding: 0 24px;
  }
  h1 {
    font-size: 32px;
    margin-bottom: 8px;
  }
  .drives {
    list-style: none;
    padding: 0;
    margin: 24px 0;
    display: grid;
    gap: 8px;
  }
  .drives button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    text-align: left;
    padding: 12px 16px;
  }
  .indexed {
    margin-left: auto;
    font-size: 12px;
    color: #6aa9ff;
  }
</style>
