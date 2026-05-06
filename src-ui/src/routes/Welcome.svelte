<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { libraryStore } from "../lib/stores/library.svelte";

  let dragOver = $state(false);
  let droppedPath = $state<string | null>(null);
  let pickError = $state<string | null>(null);

  function shortRoot(p: string): string {
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? p;
  }

  async function browseForFolder() {
    pickError = null;
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "Choose a folder to open as a library",
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      droppedPath = shortRoot(path);
      try {
        await libraryStore.open(path);
      } catch {}
      finally { droppedPath = null; }
    } catch {
      pickError =
        "Couldn't open the folder picker. Run this in the Tauri window, not a browser tab.";
    }
  }

  async function handleDrop(paths: string[]) {
    if (paths.length === 0) return;
    const first = paths[0];
    droppedPath = shortRoot(first);
    try { await libraryStore.open(first); } catch {}
    finally { droppedPath = null; }
  }

  const extraRemembered = $derived(
    libraryStore.remembered.filter(
      (p) => !libraryStore.drives.some((d) => d.path === p),
    ),
  );

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload as DragDropEvent;
        if (payload.type === "enter" || payload.type === "over") dragOver = true;
        else if (payload.type === "leave") dragOver = false;
        else if (payload.type === "drop") {
          dragOver = false;
          handleDrop(payload.paths);
        }
      })
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });
</script>

<main class="welcome" class:dropping={dragOver}>
  <div class="canvas">
    <div class="masthead">
      <span class="eyebrow">
        <span class="num">№&nbsp;01</span>
        <span class="ornament"></span>
        <span>OPEN A LIBRARY</span>
      </span>
      <h1>Photographs.</h1>
      <span class="rule" aria-hidden="true"></span>
      <p class="subtitle">
        Pick a folder. We keep its index in <code class="mono">.photovault/</code>
        on the drive itself — fully portable, never uploaded.
      </p>
    </div>

    <div class="primary-row">
      <button class="primary big" onclick={browseForFolder} disabled={libraryStore.loading}>
        Choose a folder
      </button>
      <span class="hint mono">OR DRAG ONTO THIS WINDOW</span>
    </div>

    {#if libraryStore.error}<p class="error">{libraryStore.error}</p>{/if}
    {#if pickError}<p class="error">{pickError}</p>{/if}

    {#if extraRemembered.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span><span>RECENTLY OPENED</span></span>
        <ul class="drives stagger">
          {#each extraRemembered as p, i (p)}
            <li style="--i: {i}">
              <button
                class="drive"
                disabled={libraryStore.loading}
                onclick={() => libraryStore.open(p).catch(() => {})}
              >
                <span class="drive-num mono">{String(i + 1).padStart(2, "0")}</span>
                <span class="drive-body">
                  <strong>{shortRoot(p)}</strong>
                  <span class="drive-path mono">{p}</span>
                </span>
                <span class="badge mono">indexed</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if libraryStore.drives.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span><span>DETECTED</span></span>
        <ul class="drives stagger">
          {#each libraryStore.drives as d, i (d.path)}
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
                  <span class="badge mono">indexed</span>
                {:else}
                  <span class="badge fresh mono">new</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if libraryStore.drives.length === 0 && extraRemembered.length === 0}
      <p class="empty-hint">
        Nothing detected. Use <strong>Choose a folder</strong> above.
      </p>
    {/if}

    <footer class="colophon mono">
      PHOTOVAULT &nbsp;·&nbsp; LOCAL · PRIVATE · YOURS
    </footer>
  </div>

  <!-- Drag-drop overlay covers the whole window. -->
  <div class="drop-overlay" aria-hidden={!dragOver}>
    <div class="drop-stamp">
      <span class="eyebrow"><span class="ornament"></span><span class="num">DROP</span><span class="ornament"></span></span>
      <strong class="drop-title">Open this folder.</strong>
      <span class="drop-sub mono">RELEASE TO BEGIN</span>
    </div>
  </div>

  {#if droppedPath}
    <div class="opening-overlay">
      <span class="eyebrow"><span class="ornament"></span><span>OPENING</span></span>
      <strong class="drop-title">{droppedPath}</strong>
    </div>
  {/if}
</main>

<style>
  .welcome {
    height: 100vh;
    overflow-y: auto;
    background: var(--bg);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: var(--s-9) var(--s-5);
    position: relative;
  }
  .canvas {
    width: 100%;
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: var(--s-6);
  }

  .masthead { display: flex; flex-direction: column; gap: var(--s-3); }
  .masthead h1 {
    font-size: var(--t-display);
    font-weight: 700;
    font-variation-settings: "opsz" 96, "wdth" 90;
    line-height: 0.95;
    letter-spacing: -0.04em;
  }
  .masthead .rule {
    display: block;
    height: 1px;
    background: var(--ink);
    width: 100%;
    transform: scaleX(0);
    transform-origin: left;
    animation: draw-in var(--t-slow) var(--ease-out) 100ms forwards;
    margin: 0;
    border: none;
  }

  .primary-row {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    flex-wrap: wrap;
  }
  .primary.big {
    font-family: var(--font-display);
    font-size: var(--t-base);
    font-weight: 600;
    padding: 12px 26px;
    border-radius: var(--r-md);
    font-variation-settings: "opsz" 16, "wdth" 100;
  }
  .hint {
    font-size: 10px;
    letter-spacing: 0.18em;
    color: var(--ink-faint);
  }

  section { display: flex; flex-direction: column; gap: var(--s-3); }
  section .eyebrow { display: inline-flex; }

  .empty-hint {
    padding: var(--s-5);
    text-align: center;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--ink-muted);
  }

  .drives {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .drive {
    width: 100%;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--s-4);
    padding: var(--s-4) var(--s-5);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    text-align: left;
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease);
  }
  .drive:hover {
    background: var(--bg-card);
    border-color: var(--ink-faint);
  }
  .drive-num { font-size: var(--t-sm); color: var(--ink-faint); }
  .drive-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .drive-body strong {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 600;
    font-variation-settings: "opsz" 18;
  }
  .drive-path {
    font-size: var(--t-xs);
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--accent-ghost);
    color: var(--ink);
  }
  .badge.fresh {
    background: transparent;
    color: var(--ink-faint);
    border: 1px solid var(--line);
  }

  .colophon {
    margin-top: var(--s-7);
    padding-top: var(--s-5);
    border-top: 1px solid var(--line-soft);
    text-align: center;
    color: var(--ink-faint);
    font-size: 9.5px;
    letter-spacing: 0.18em;
  }

  code.mono {
    background: var(--bg-card);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 0.92em;
    color: var(--ink);
  }

  /* Drag-drop overlays span the entire welcome surface. */
  .drop-overlay, .opening-overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    opacity: 0;
    transition: opacity var(--t-base-d) var(--ease);
    z-index: 50;
    background: rgba(0,0,0,0);
    backdrop-filter: blur(0px);
  }
  .welcome.dropping .drop-overlay {
    opacity: 1;
    background: var(--accent-ghost);
    backdrop-filter: blur(2px);
  }
  .opening-overlay {
    opacity: 1;
    background: var(--bg);
    flex-direction: column;
    gap: var(--s-3);
  }
  .drop-stamp {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: center;
    padding: var(--s-7) var(--s-8);
    border: 2px solid var(--ink);
    border-radius: var(--r-sm);
    background: var(--bg-paper);
    transform: scale(0.96);
    transition: transform var(--t-slow) var(--ease-out);
  }
  .welcome.dropping .drop-stamp { transform: scale(1); }
  .drop-title {
    font-family: var(--font-display);
    font-size: var(--t-3xl);
    font-weight: 700;
    font-variation-settings: "opsz" 60, "wdth" 92;
    letter-spacing: -0.02em;
    line-height: 1.05;
    color: var(--ink);
  }
  .drop-sub {
    font-size: var(--t-xs);
    color: var(--ink);
    letter-spacing: 0.22em;
  }
</style>
