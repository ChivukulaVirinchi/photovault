<script lang="ts">
  import { libraryStore } from "../lib/stores/library.svelte";

  function shortRoot(p: string): string {
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? p;
  }
</script>

<main class="welcome">
  <div class="canvas">
    <div class="frame">
      <div class="masthead-block">
        <span class="eyebrow">
          <span class="num">№&nbsp;01</span>
          <span class="ornament"></span>
          <span>OPEN A LIBRARY</span>
        </span>
        <h1>
          A quiet place
          <span class="italic">for your photographs.</span>
        </h1>
        <p class="subtitle">
          Pick a drive or folder. We'll keep its index in
          <code class="mono">.photovault/</code> on the drive itself —
          fully portable, never uploaded.
        </p>
      </div>

      {#if libraryStore.error}
        <p class="error">{libraryStore.error}</p>
      {/if}

      {#if libraryStore.drives.length === 0}
        <div class="empty">
          <p class="muted">No drives detected.</p>
          <button onclick={() => libraryStore.refresh()}>Refresh</button>
        </div>
      {:else}
        <ul class="drives stagger">
          {#each libraryStore.drives as d, i}
            <li style="--i: {i}">
              <button
                class="drive"
                disabled={libraryStore.loading}
                onclick={() => libraryStore.open(d.path).catch(() => {})}
              >
                <span class="drive-num mono">{String(i + 1).padStart(2, "0")}</span>
                <span class="drive-body">
                  <strong>{shortRoot(d.path)}</strong>
                  <span class="drive-path mono">{d.path}</span>
                </span>
                {#if d.has_photovault_db}
                  <span class="badge">indexed</span>
                {:else}
                  <span class="badge fresh">new</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="actions">
        <button class="ghost" onclick={() => libraryStore.refresh()}>
          Refresh drives
        </button>
      </div>

      <footer class="colophon">
        <span class="mono">PHOTOVAULT &nbsp;·&nbsp; OFFLINE-FIRST &nbsp;·&nbsp; YOUR PHOTOS, YOUR DRIVE</span>
      </footer>
    </div>
  </div>
</main>

<style>
  .welcome {
    height: 100vh;
    overflow: auto;
    background: var(--bg);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .canvas {
    width: 100%;
    max-width: 920px;
    padding: var(--s-7) var(--s-5);
  }
  .frame {
    background: var(--bg-paper);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-xl);
    padding: var(--s-8) var(--s-7);
    box-shadow: var(--shadow-soft);
    position: relative;
  }
  /* Two corner ornaments — magazine spread feel */
  .frame::before, .frame::after {
    content: "";
    position: absolute;
    width: 18px;
    height: 18px;
    border: 1px solid var(--line);
  }
  .frame::before {
    top: var(--s-4); left: var(--s-4);
    border-right: none; border-bottom: none;
  }
  .frame::after {
    bottom: var(--s-4); right: var(--s-4);
    border-left: none; border-top: none;
  }

  .masthead-block { margin-bottom: var(--s-6); }
  .masthead-block h1 {
    font-size: var(--t-display);
    margin: var(--s-3) 0 var(--s-4);
    line-height: 0.98;
    font-variation-settings: "opsz" 144, "SOFT" 30, "WONK" 0;
  }
  .italic {
    display: block;
    font-style: italic;
    font-weight: 300;
    font-variation-settings: "opsz" 144, "SOFT" 80, "WONK" 1;
    color: var(--ink-soft);
  }

  .empty {
    padding: var(--s-7) var(--s-5);
    text-align: center;
    border: 1px dashed var(--line);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: center;
  }

  .drives {
    list-style: none;
    margin: var(--s-5) 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .drive {
    width: 100%;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--s-4);
    padding: var(--s-4) var(--s-5);
    background: var(--bg-card);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    text-align: left;
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease),
                transform var(--t-fast) var(--ease);
  }
  .drive:hover {
    background: var(--bg-elev);
    border-color: var(--line);
    transform: translateX(3px);
  }
  .drive-num {
    font-size: var(--t-sm);
    color: var(--ink-faint);
    letter-spacing: 0.05em;
  }
  .drive-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .drive-body strong {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 500;
    font-variation-settings: "opsz" 24;
  }
  .drive-path {
    font-size: var(--t-xs);
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--accent-ghost);
    color: var(--accent);
  }
  .badge.fresh {
    background: transparent;
    color: var(--ink-faint);
    border: 1px solid var(--line);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--s-4);
  }

  .colophon {
    margin-top: var(--s-7);
    padding-top: var(--s-5);
    border-top: 1px solid var(--line-soft);
    text-align: center;
    color: var(--ink-faint);
    font-size: 10px;
    letter-spacing: 0.16em;
  }

  code.mono {
    background: var(--bg-card);
    padding: 1px 8px;
    border-radius: 4px;
    font-size: 0.92em;
    color: var(--accent);
  }
</style>
