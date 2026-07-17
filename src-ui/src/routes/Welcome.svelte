<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { library } from "../lib/api/library";
  import { commandErrorMessage } from "../lib/api";
  import { systemEx } from "../lib/api/all";
  import type { AssetHealthDto, PhotoSummaryDto } from "../lib/api/types";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { thumbUrl } from "../lib/thumbnail";

  let dragOver = $state(false);
  let droppedPath = $state<string | null>(null);
  let pickError = $state<string | null>(null);
  let compatPhotos = $state<PhotoSummaryDto[]>([]);
  let compatTotal = $state<number | null>(null);
  let compatLoading = $state(false);
  let compatError = $state<string | null>(null);
  let compatLoaded = $state(false);
  let compatSeq = 0;
  let mounted = true;
  let assetHealth = $state<AssetHealthDto | null>(null);
  let assetError = $state<string | null>(null);
  let assetHealthLoading = $state(false);
  const installingAssets = $derived(jobs.isRunning("assets"));
  const assetsJob = $derived(jobs.byKind("assets"));
  let handledAssetJobId: string | null = null;

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
      if (!mounted) return;
      droppedPath = shortRoot(path);
      try {
        await libraryStore.open(path);
      } catch {}
      finally { if (mounted) droppedPath = null; }
    } catch {
      if (!mounted) return;
      pickError =
        "Couldn't open the folder picker. Run this in the Tauri window, not a browser tab.";
    }
  }

  async function handleDrop(paths: string[]) {
    if (paths.length === 0) return;
    const first = paths[0];
    if (!mounted) return;
    droppedPath = shortRoot(first);
    try { await libraryStore.open(first); } catch {}
    finally { if (mounted) droppedPath = null; }
  }

  async function loadAssetHealth() {
    assetHealthLoading = true;
    try {
      const health = await systemEx.assetHealth();
      if (mounted) {
        assetHealth = health;
        assetError = null;
      }
    } catch (error) {
      if (mounted) assetError = commandErrorMessage(error);
    } finally {
      if (mounted) assetHealthLoading = false;
    }
  }

  async function installAssets() {
    if (installingAssets) return;
    assetError = null;
    const placeholderId = `pending-assets-${Date.now()}`;
    jobs.register(placeholderId, "assets");
    try {
      const result = await systemEx.installAssets();
      jobs.dismiss(placeholderId);
      jobs.register(result.job_id, "assets");
    } catch (error) {
      jobs.dismiss(placeholderId);
      if (mounted) assetError = commandErrorMessage(error);
    }
  }

  const extraRemembered = $derived(
    libraryStore.remembered.filter(
      (p) => !libraryStore.drives.some((d) => d.path === p),
    ),
  );
  const schemaTooNew = $derived(
    libraryStore.unsupportedSchema ??
      (libraryStore.lastError?.kind === "schema_too_new" ? libraryStore.lastError : null),
  );
  const canLoadMoreCompat = $derived(
    !compatError && (compatTotal == null || compatPhotos.length < compatTotal),
  );

  async function loadCompatPhotos(reset = false) {
    const seq = ++compatSeq;
    compatLoading = true;
    compatError = null;
    if (reset) {
      compatPhotos = [];
      compatTotal = null;
      compatLoaded = false;
    }
    try {
      const page = await library.compatPhotos(reset ? 0 : compatPhotos.length, 100);
      if (!mounted || seq !== compatSeq) return;
      compatPhotos = reset ? page.items : [...compatPhotos, ...page.items];
      compatTotal = page.total;
      compatLoaded = true;
    } catch (e) {
      if (!mounted || seq !== compatSeq) return;
      compatError = commandErrorMessage(e);
      compatLoaded = true;
    } finally {
      if (mounted && seq === compatSeq) compatLoading = false;
    }
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    mounted = true;
    void loadAssetHealth();
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!mounted) return;
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
      mounted = false;
      unlisten?.();
    };
  });

  $effect(() => {
    const job = assetsJob;
    if (!job || job.id === handledAssetJobId) return;
    if (job.status === "complete" || job.status === "error") {
      handledAssetJobId = job.id;
      if (job.status === "error") {
        assetError = job.message || "Asset installation failed.";
      } else {
        void loadAssetHealth();
      }
    }
  });

  $effect(() => {
    if (libraryStore.unsupportedSchema && !compatLoaded && !compatLoading) {
      void loadCompatPhotos(true);
    }
    if (!libraryStore.unsupportedSchema && (compatLoaded || compatPhotos.length > 0 || compatTotal != null || compatError)) {
      compatSeq += 1;
      compatPhotos = [];
      compatTotal = null;
      compatError = null;
      compatLoaded = false;
    }
  });
</script>

<main class="welcome" class:dropping={dragOver}>
  <div class="canvas">
    <header class="hero">
      <h1 class="hero-title display">Photographs.</h1>
      <p class="hero-sub">Everything you've kept.</p>
    </header>

    {#if schemaTooNew}
      <div class="schema-error">
        <p class="error">
          This library uses schema v{schemaTooNew.db_version}; this Smriti build supports v{schemaTooNew.max_supported}.
        </p>
        {#if libraryStore.driveRoot}
          <p class="schema-note mono">{libraryStore.driveRoot}</p>
        {/if}
        <a class="primary small-action" href="#/settings">Open Settings</a>
      </div>
      {#if libraryStore.unsupportedSchema}
        <section class="compat-preview">
          <div class="compat-head">
            <h3 class="section-title">Read-only preview</h3>
            {#if compatTotal != null}
              <span class="compat-count">{compatPhotos.length} / {compatTotal}</span>
            {/if}
          </div>
          {#if compatError}
            <p class="error">{compatError}</p>
          {/if}
          {#if compatPhotos.length > 0}
            <div class="compat-grid">
              {#each compatPhotos as photo (photo.id)}
                <div class="compat-thumb">
                  {#if photo.thumbnail_path}
                    <img src={thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) ?? ""} alt="" loading="lazy" />
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
          {#if canLoadMoreCompat}
            <button class="ghost load-more" onclick={() => loadCompatPhotos(false)} disabled={compatLoading}>
              {compatLoading ? "Loading" : "Load more"}
            </button>
          {/if}
        </section>
      {/if}
    {:else if libraryStore.error}<p class="error">{libraryStore.error}</p>{/if}
    {#if pickError}<p class="error">{pickError}</p>{/if}

    {#if assetHealth?.missing_face_models || assetHealth?.missing_onnx_runtime || assetHealth?.missing_geonames_db}
      <section class="asset-setup" aria-label="Optional asset setup">
        <div class="asset-copy">
          <strong>Enable faces and offline place names</strong>
          <span>One download installs the local runtime, face models, and GeoNames database.</span>
          {#if assetError}<span class="error">{assetError}</span>{/if}
        </div>
        <button class="primary" onclick={installAssets} disabled={installingAssets}>
          {installingAssets ? "Installing assets…" : "Set up assets"}
        </button>
      </section>
    {:else if assetError}
      <p class="error">Couldn't check optional assets: {assetError}</p>
    {:else if assetHealthLoading}
      <p class="hint">Checking local assets…</p>
    {/if}

    {#if extraRemembered.length > 0}
      <section>
        <h3 class="section-title">Open again</h3>
        <ul class="drives">
          {#each extraRemembered as p (p)}
            <li>
              <button
                class="drive"
                disabled={libraryStore.loading}
                onclick={() => libraryStore.open(p).catch(() => {})}
              >
                <span class="drive-body">
                  <strong>{shortRoot(p)}</strong>
                  <span class="drive-path mono">{p}</span>
                </span>
                <span class="badge">Indexed</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if libraryStore.drives.length > 0}
      <section>
        <h3 class="section-title">Detected drives</h3>
        <ul class="drives">
          {#each libraryStore.drives as d (d.path)}
            <li>
              <button
                class="drive"
                disabled={libraryStore.loading}
                onclick={() => libraryStore.open(d.path).catch(() => {})}
              >
                <span class="drive-body">
                  <strong>{shortRoot(d.path)}</strong>
                  <span class="drive-path mono">{d.path}</span>
                </span>
                {#if d.has_photovault_db}
                  <span class="badge">Indexed</span>
                {:else}
                  <span class="badge fresh">New</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <div class="cta-row">
      <button class="primary cta" onclick={browseForFolder} disabled={libraryStore.loading}>
        Choose a folder
      </button>
      <a class="ghost settings-link" href="#/settings">Settings</a>
      <span class="hint">or drag one onto this window</span>
    </div>
  </div>

  <!-- Drag-drop overlay covers the whole window. -->
  <div class="drop-overlay" aria-hidden={!dragOver}>
    <div class="drop-stamp">
      <strong class="drop-title display">Open this folder</strong>
      <span class="drop-sub">Release to begin</span>
    </div>
  </div>

  {#if droppedPath}
    <div class="opening-overlay">
      <strong class="drop-title display">{droppedPath}</strong>
      <span class="drop-sub">Opening</span>
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
    padding: var(--s-9) var(--s-5) var(--s-7);
    position: relative;
  }
  .canvas {
    width: 100%;
    max-width: 680px;
    display: flex;
    flex-direction: column;
    gap: var(--s-6);
  }

  .hero {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding-bottom: var(--s-3);
  }
  .hero-title {
    font-size: var(--t-display);
    font-weight: 700;
    font-variation-settings: "opsz" 96, "wdth" 92;
    line-height: 0.95;
    letter-spacing: -0.04em;
    color: var(--ink);
    margin: 0;
  }
  .hero-sub {
    font-size: var(--t-lg);
    color: var(--ink-soft);
    line-height: 1.4;
    margin: 0;
  }

  .cta-row {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    flex-wrap: wrap;
  }
  .primary.cta {
    font-size: var(--t-base);
    font-weight: 600;
    padding: 11px 22px;
  }
  .hint {
    font-size: var(--t-sm);
    color: var(--ink-muted);
  }
  .asset-setup {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-4);
    padding: var(--s-4) var(--s-5);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-paper);
  }
  .asset-copy {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--ink-soft);
    font-size: var(--t-sm);
    line-height: 1.4;
  }
  .asset-copy strong { color: var(--ink); }
  .asset-copy .error { margin: var(--s-1) 0 0; }
  .asset-setup button { flex-shrink: 0; }
  @media (max-width: 620px) {
    .asset-setup { align-items: stretch; flex-direction: column; }
  }
  .schema-error {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    flex-wrap: wrap;
  }
  .schema-error .error {
    margin: 0;
  }
  .schema-note {
    width: 100%;
    margin: 0;
    color: var(--ink-muted);
    font-size: var(--t-xs);
    overflow-wrap: anywhere;
  }
  .compat-preview {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }
  .compat-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--s-3);
  }
  .compat-count {
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
  .compat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(74px, 1fr));
    gap: 2px;
  }
  .compat-thumb {
    aspect-ratio: 1;
    background: var(--bg-card);
    overflow: hidden;
  }
  .compat-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .load-more {
    align-self: flex-start;
    font-size: var(--t-sm);
    padding: 8px 12px;
  }
  .small-action {
    text-decoration: none;
    font-size: var(--t-sm);
    font-weight: 600;
    padding: 8px 12px;
  }
  .settings-link {
    text-decoration: none;
    font-size: var(--t-base);
    padding: 11px 16px;
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: transparent;
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease);
  }
  .settings-link:hover {
    background: var(--bg-paper);
    border-color: var(--ink-faint);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }
  .section-title {
    font-size: var(--t-sm);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .drives {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .drive {
    width: 100%;
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--s-4);
    padding: var(--s-4) var(--s-5);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    text-align: left;
    color: var(--ink);
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease);
  }
  .drive:hover {
    background: var(--bg-card);
    border-color: var(--accent);
  }
  .drive-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .drive-body strong {
    font-size: var(--t-lg);
    font-weight: 600;
    color: var(--ink);
  }
  .drive-path {
    font-size: var(--t-xs);
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: var(--t-xs);
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--accent-ghost);
    color: var(--accent);
    font-weight: 500;
    border: 1px solid transparent;
    flex-shrink: 0;
  }
  .badge.fresh {
    background: transparent;
    color: var(--ink-muted);
    border-color: var(--line);
  }

  /* ===== Drag-drop overlay ===== */
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
    background: rgba(0, 0, 0, 0);
  }
  .welcome.dropping .drop-overlay {
    opacity: 1;
    background: color-mix(in oklab, var(--accent) 12%, transparent);
    backdrop-filter: blur(2px);
  }
  .opening-overlay {
    opacity: 1;
    background: var(--bg);
    flex-direction: column;
    gap: var(--s-2);
  }
  .drop-stamp {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    align-items: center;
    padding: var(--s-7) var(--s-8);
    border: 2px solid var(--accent);
    border-radius: var(--r-md);
    background: var(--bg-paper);
    transform: scale(0.96);
    transition: transform var(--t-slow) var(--ease-out);
  }
  .welcome.dropping .drop-stamp { transform: scale(1); }
  .drop-title {
    font-size: var(--t-3xl);
    font-weight: 700;
    font-variation-settings: "opsz" 60, "wdth" 92;
    letter-spacing: -0.02em;
    line-height: 1.05;
    color: var(--ink);
  }
  .drop-sub {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }

  @media (max-width: 560px) {
    .welcome { padding: var(--s-7) var(--s-4); }
    .hero-title { font-size: var(--t-3xl); }
  }
</style>
