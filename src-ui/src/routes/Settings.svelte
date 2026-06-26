<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { albums, geocoding, health, people, semantic, stacks, systemEx } from "../lib/api/all";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { devMode } from "../lib/stores/devMode.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { AssetInventory, AssetItem, LibraryHealthData, SemanticStatus, Settings } from "../lib/api/all";
  import type { ExcludedFolderDto } from "../lib/api/types";

  let saving = $state(false);
  let error = $state<string | null>(null);
  let healthData = $state<LibraryHealthData | null>(null);
  let assets = $state<AssetInventory | null>(null);
  let semanticStatus = $state<SemanticStatus | null>(null);
  let exclusions = $state<ExcludedFolderDto[] | null>(null);
  let exclusionsBusy = $state(false);
  let exclusionsActing = $state(false);
  let assetsBusy = $state(false);
  let acting = $state(false);
  let testBusy = $state(false);
  let testResult = $state<{ ok: boolean; gpu_name: string; latency_ms: number; model?: string | null } | null>(null);
  let assistantApiKey = $state("");
  async function testBridge() {
    if (!s?.face_gpu_bridge_url) return;
    testBusy = true;
    testResult = null;
    try {
      const r = await systemEx.testGpuBridge(s.face_gpu_bridge_url);
      testResult = { ok: r.ok, gpu_name: r.gpu_name, latency_ms: r.latency_ms, model: r.model };
    } catch (e) {
      testResult = { ok: false, gpu_name: "Unreachable", latency_ms: 0 };
    } finally {
      testBusy = false;
    }
  }
  // Backfill state derived from the global jobs store so progress
  // survives navigation (a 50k-photo backfill is a multi-second job).
  const backfilling = $derived(jobs.isRunning("geocoding"));
  const geocodingJob = $derived(jobs.byKind("geocoding"));
  const installingAssets = $derived(jobs.isRunning("assets"));
  const assetsJob = $derived(jobs.byKind("assets"));
  const semanticRunning = $derived(jobs.isRunning("semantic"));
  const semanticJob = $derived(jobs.byKind("semantic"));
  const visualSearchReady = $derived(Boolean(semanticStatus?.assets_installed && semanticStatus?.onnx_runtime_installed));
  const visualSearchMissingRuntime = $derived(Boolean(semanticStatus?.assets_installed && !semanticStatus?.onnx_runtime_installed));
  const visualSearchStatus = $derived(
    visualSearchReady ? "ready" : visualSearchMissingRuntime ? "runtime missing" : "missing",
  );

  // React to backfill completion via the global store. Same reasoning
  // as Albums.svelte: a per-page Tauri `listen()` races with fast
  // events on small libraries. The store is filled by the app-boot
  // subscription and survives navigation.
  let toastedGeoIds = new Set<string>();
  $effect(() => {
    if (!geocodingJob) return;
    if (geocodingJob.status === "complete" && !toastedGeoIds.has(geocodingJob.id)) {
      toastedGeoIds.add(geocodingJob.id);
      const msg = geocodingJob.message || "Geocoding finished.";
      const isError =
        msg.toLowerCase().startsWith("geocoding failed") ||
        msg.toLowerCase().startsWith("geonames database not found");
      if (isError) toasts.error(msg);
      else toasts.success(msg);
      // If the backfill actually populated rows, kick album-suggestion
      // detection — those depend on location data being present.
      // Couldn't tell from the message alone; only fire when the
      // success branch indicates resolved counts.
      if (!isError && msg.startsWith("Resolved ")) {
        albums.suggestions.runDetection().catch(() => {});
      }
    }
  });

  onMount(() => {
    settingsStore.load();
    health.compute().then((d) => (healthData = d)).catch(() => {});
    loadAssets();
    loadSemanticStatus();
    loadExclusions();
  });

  let toastedAssetIds = new Set<string>();
  $effect(() => {
    if (!assetsJob) return;
    if (assetsJob.status === "complete" && !toastedAssetIds.has(assetsJob.id)) {
      toastedAssetIds.add(assetsJob.id);
      const msg = assetsJob.message || "Asset setup finished.";
      if (msg.toLowerCase().startsWith("asset install failed")) {
        toasts.error(msg);
      } else {
        toasts.success("Assets ready.");
        loadAssets();
      }
    }
  });

  let toastedSemanticIds = new Set<string>();
  $effect(() => {
    if (!semanticJob) return;
    if (semanticJob.status === "complete" && !toastedSemanticIds.has(semanticJob.id)) {
      toastedSemanticIds.add(semanticJob.id);
      const msg = semanticJob.message || "Visual search updated.";
      if (msg.toLowerCase().includes("failed")) toasts.error(msg);
      else toasts.success(msg);
      loadAssets();
      loadSemanticStatus();
    }
  });

  const s = $derived(settingsStore.data);

  async function patch(p: Partial<Settings>) {
    if (!s) return;
    saving = true;
    try { await settingsStore.update(p); }
    catch (e) { error = JSON.stringify(e); }
    finally { saving = false; }
  }

  async function backfillGeocoding(force = false) {
    if (backfilling) return;
    const placeholderId = `pending-geocoding-${Date.now()}`;
    jobs.register(placeholderId, "geocoding");
    toasts.success(force ? "Refreshing all place names…" : "Filling in place names…");
    try {
      const r = await geocoding.backfill(force);
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "geocoding");
    } catch (e) {
      jobs.dismiss(placeholderId);
      toasts.error(`Couldn't start: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    }
  }

  /// Wipe every photo's thumbnail_path and re-run the thumbnail pass.
  /// Used to upgrade legacy small thumbnails to the current default
  /// size after that default changed. Long-running on big libraries
  /// — progress shows in the global JobsIndicator.
  const regeneratingThumbs = $derived(jobs.isRunning("thumbnails"));
  const refreshingDates = $derived(jobs.isRunning("metadata"));
  let refreshingStacks = $state(false);
  async function refreshPhotoDates() {
    if (refreshingDates) return;
    if (
      !confirm(
        "Refresh capture dates for every photo and video?\n\nSmriti will re-read embedded metadata and strict filename dates, then use file modified time only as a fallback. You can keep using the app while it runs.",
      )
    )
      return;
    const placeholderId = `pending-metadata-${Date.now()}`;
    jobs.register(placeholderId, "metadata");
    toasts.success("Refreshing capture dates...");
    try {
      const r = await library.refreshPhotoDates();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "metadata");
    } catch (e) {
      jobs.dismiss(placeholderId);
      toasts.error(`Couldn't start: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    }
  }

  async function regenerateThumbs() {
    if (regeneratingThumbs) return;
    if (
      !confirm(
        "Re-generate every thumbnail at the current quality?\n\nThis overwrites the cached JPEGs on disk and takes a while on big libraries (each photo is decoded + resampled once). You can keep using the app while it runs.",
      )
    )
      return;
    const placeholderId = `pending-thumbs-${Date.now()}`;
    jobs.register(placeholderId, "thumbnails");
    toasts.success("Regenerating thumbnails…");
    try {
      const r = await library.regenerateThumbnails();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "thumbnails");
    } catch (e) {
      jobs.dismiss(placeholderId);
      toasts.error(`Couldn't start: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    }
  }

  async function loadAssets() {
    assetsBusy = true;
    try {
      assets = await systemEx.assetsInventory();
    } catch (e) {
      toasts.error(`Couldn't read assets: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      assetsBusy = false;
    }
  }

  async function loadSemanticStatus() {
    try {
      semanticStatus = await semantic.status();
    } catch {
      semanticStatus = null;
    }
  }

  async function installAssets() {
    if (installingAssets) return;
    const placeholderId = `pending-assets-${Date.now()}`;
    jobs.register(placeholderId, "assets");
    toasts.success("Downloading asset pack...");
    try {
      const r = await systemEx.installAssets();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "assets");
    } catch (e) {
      jobs.dismiss(placeholderId);
      toasts.error(`Couldn't start asset setup: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    }
  }

  async function installSemanticModel() {
    if (semanticRunning) return;
    const placeholderId = `pending-semantic-assets-${Date.now()}`;
    jobs.register(placeholderId, "semantic");
    toasts.success("Downloading visual search model...");
    try {
      const r = await semantic.installModel();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "semantic");
    } catch (e) {
      jobs.dismiss(placeholderId);
      toasts.error(`Couldn't start visual search model install: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    }
  }

  async function startSemanticIndexing() {
    if (semanticRunning) return;
    const placeholderId = `pending-semantic-index-${Date.now()}`;
    jobs.register(placeholderId, "semantic");
    toasts.success("Indexing visual search...");
    try {
      const r = await semantic.startIndexing();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "semantic");
    } catch (e) {
      jobs.dismiss(placeholderId);
      toasts.error(`Couldn't start visual search indexing: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    }
  }

  async function loadExclusions() {
    exclusionsBusy = true;
    try {
      exclusions = await library.exclusions.list();
    } catch {
      exclusions = null;
    } finally {
      exclusionsBusy = false;
    }
  }

  async function addExclusion() {
    if (exclusionsActing) return;
    let selected: string | string[] | null;
    try {
      selected = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: libraryStore.driveRoot ?? undefined,
      });
    } catch {
      toasts.error("Couldn't open the folder picker.");
      return;
    }
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return;

    exclusionsActing = true;
    try {
      const preview = await library.exclusions.preview(path);
      const count = preview.indexed_count;
      const itemText = count === 1 ? "1 indexed item" : `${count.toLocaleString()} indexed items`;
      if (
        !confirm(
          `Exclude ${preview.relative_path}?\n\nSmriti will skip this folder in future scans. ${itemText} inside it will be removed from Smriti. Files stay on disk.`,
        )
      )
        return;
      await library.exclusions.add(path);
      await loadExclusions();
      toasts.success(`Excluded ${preview.relative_path}`);
    } catch (e) {
      toasts.error(`Couldn't exclude folder: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      exclusionsActing = false;
    }
  }

  async function removeExclusion(relativePath: string) {
    if (exclusionsActing) return;
    exclusionsActing = true;
    try {
      await library.exclusions.remove(relativePath);
      await loadExclusions();
      toasts.success("Exclusion removed. Run scan to index this folder again.");
    } catch (e) {
      toasts.error(`Couldn't remove exclusion: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      exclusionsActing = false;
    }
  }

  function formatBytes(bytes: number | null | undefined): string {
    if (!bytes) return "-";
    const units = ["B", "KB", "MB", "GB"];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
  }

  function installedCount(items: AssetItem[]): number {
    return items.filter((item) => item.status === "active" || item.status === "extra").length;
  }

  function managedCount(items: AssetItem[]): number {
    return items.filter((item) => item.status !== "planned").length;
  }

  function hasExternalAssets(inventory: AssetInventory): boolean {
    const root = inventory.install_root.toLowerCase();
    return inventory.assets.some((item) =>
      item.path && item.status !== "planned" && !item.path.toLowerCase().startsWith(root),
    );
  }

  async function refreshStacks() {
    if (refreshingStacks) return;
    refreshingStacks = true;
    try {
      const result = await stacks.refresh();
      toasts.success(`${result.stacks_found} ${result.stacks_found === 1 ? "stack" : "stacks"} ready`);
    } catch (e) {
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      toasts.error(`Couldn't refresh stacks: ${msg}`);
    } finally {
      refreshingStacks = false;
    }
  }

  // ----- destructive actions -------------------------------------------
  // Each goes through `confirm()` and a toast on outcome. The reset
  // commands run synchronously on the backend (small, atomic SQL).

  async function runFacesFromScratch() {
    if (acting) return;
    if (
      !confirm(
        "Wipe every detected face, every cluster, and every face crop on disk, then re-run face detection?\n\nThis is irreversible. Names you've assigned to clusters will be lost.",
      )
    )
      return;
    acting = true;
    try {
      const r = await people.resetAll();
      toasts.success(
        `Reset ${r.faces_dropped.toLocaleString()} faces and ${r.clusters_dropped.toLocaleString()} clusters. Starting fresh detection…`,
      );
      // Kick the engine — registered as a regular faces job so the
      // global indicator shows progress.
      const placeholderId = `pending-faces-${Date.now()}`;
      jobs.register(placeholderId, "faces");
      try {
        const j = await people.startProcessing();
        jobs.dismiss(placeholderId);
        jobs.register(j.job_id, "faces");
      } catch (e) {
        jobs.dismiss(placeholderId);
        toasts.error(`Couldn't start: ${typeof e === "string" ? e : JSON.stringify(e)}`);
      }
    } catch (e) {
      toasts.error(`Reset failed: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      acting = false;
    }
  }

  async function resetFaceClusters() {
    if (acting) return;
    if (
      !confirm(
        "Drop every cluster and re-cluster from existing face embeddings?\n\nThis keeps the detected faces themselves but throws away grouping decisions and any names you've assigned. Useful when you've changed the clustering threshold.",
      )
    )
      return;
    acting = true;
    try {
      const r = await people.resetClusters();
      toasts.success(
        `Cleared ${r.clusters_dropped.toLocaleString()} cluster${r.clusters_dropped === 1 ? "" : "s"}. Re-clustering…`,
      );
      const placeholderId = `pending-faces-${Date.now()}`;
      jobs.register(placeholderId, "faces");
      try {
        const j = await people.startProcessing();
        jobs.dismiss(placeholderId);
        jobs.register(j.job_id, "faces");
      } catch (e) {
        jobs.dismiss(placeholderId);
        toasts.error(`Couldn't start: ${typeof e === "string" ? e : JSON.stringify(e)}`);
      }
    } catch (e) {
      toasts.error(`Reset failed: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      acting = false;
    }
  }

  async function resetSuggestions() {
    if (acting) return;
    if (
      !confirm(
        "Wipe every album suggestion, including ones you've already dismissed?\n\nUseful if you reflexively dismissed everything early on.",
      )
    )
      return;
    acting = true;
    try {
      const r = await albums.suggestions.resetAll();
      toasts.success(
        `Cleared ${r.dropped.toLocaleString()} suggestion${r.dropped === 1 ? "" : "s"}. Run Detect in Albums to repopulate.`,
      );
    } catch (e) {
      toasts.error(`Reset failed: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      acting = false;
    }
  }
</script>

<PageHeader title="Settings">
  {#if saving}
    <span class="status mono">Saving…</span>
  {/if}
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if s}
    <section>
      <h3 class="section-title">Appearance</h3>
      <label>
        <span class="label-text">Theme</span>
        <select value={s.theme} onchange={(e) => patch({ theme: (e.target as HTMLSelectElement).value })}>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
          <option value="system">System</option>
        </select>
      </label>
      <label>
        <span class="label-text">Date format</span>
        <select value={s.date_format} onchange={(e) => patch({ date_format: (e.target as HTMLSelectElement).value })}>
          <option value="locale">Locale</option>
          <option value="iso">ISO</option>
          <option value="us">US</option>
          <option value="eu">EU</option>
        </select>
      </label>
    </section>

    <section>
      <h3 class="section-title">AI features</h3>
      <label class="checkbox">
        <input type="checkbox" checked={s.ai_features_enabled}
          onchange={(e) => patch({ ai_features_enabled: (e.target as HTMLInputElement).checked })} />
        <span class="label-text">Enable AI features</span>
      </label>
      <p class="hint blurb">
        Enables provider-backed Assistant features. Local visual search remains available when this is off.
      </p>
    </section>

    {#if s.ai_features_enabled}
      <section>
        <h3 class="section-title">Assistant</h3>
        <label class="checkbox">
          <input type="checkbox" checked={s.assistant_enabled}
            onchange={(e) => patch({ assistant_enabled: (e.target as HTMLInputElement).checked })} />
          <span class="label-text">Enable Assistant</span>
        </label>
        <label>
          <span class="label-text">Provider</span>
          <select value={s.assistant_provider} onchange={(e) => patch({ assistant_provider: (e.target as HTMLSelectElement).value as Settings["assistant_provider"] })}>
            <option value="local">Local album tools</option>
            <option value="openai_compatible">OpenAI-compatible</option>
          </select>
        </label>
        {#if s.assistant_provider === "openai_compatible"}
          <label>
            <span class="label-text">Base URL</span>
            <input
              value={s.assistant_base_url}
              spellcheck="false"
              onchange={(e) => patch({ assistant_base_url: (e.target as HTMLInputElement).value.trim() })}
            />
          </label>
          <label>
            <span class="label-text">Model</span>
            <input
              value={s.assistant_model}
              spellcheck="false"
              onchange={(e) => patch({ assistant_model: (e.target as HTMLInputElement).value.trim() })}
            />
          </label>
          <label>
            <span class="label-text">
              API key
              {#if s.assistant_api_key_set}<span class="hint">(configured)</span>{/if}
            </span>
            <span class="inline-field">
              <input
                type="password"
                bind:value={assistantApiKey}
                autocomplete="off"
                spellcheck="false"
                placeholder={s.assistant_api_key_set ? "Stored key unchanged" : "Paste API key"}
              />
              <button
                class="ghost"
                onclick={async () => {
                  await patch({ assistant_api_key: assistantApiKey || null });
                  assistantApiKey = "";
                }}
                disabled={!assistantApiKey.trim()}
              >
                Save key
              </button>
              {#if s.assistant_api_key_set}
                <button class="ghost danger-soft" onclick={() => patch({ assistant_api_key: null })}>
                  Clear
                </button>
              {/if}
            </span>
          </label>
          <p class="hint blurb">
            The key is saved locally and is never sent back to the UI after saving.
          </p>
        {/if}
      </section>
    {/if}

    <section>
      <h3 class="section-title">Library</h3>
      <label>
        <span class="label-text">Thumbnail size</span>
        <input type="number" min="100" max="1000" value={s.thumbnail_size}
          onchange={(e) => patch({ thumbnail_size: Number((e.target as HTMLInputElement).value) })} />
      </label>
      <label class="checkbox">
        <input type="checkbox" checked={s.scan_hidden_folders}
          onchange={(e) => patch({ scan_hidden_folders: (e.target as HTMLInputElement).checked })} />
        <span class="label-text">Scan hidden folders</span>
      </label>
      <label class="checkbox">
        <input type="checkbox" checked={s.show_timeline_stacks}
          onchange={(e) => patch({ show_timeline_stacks: (e.target as HTMLInputElement).checked })} />
        <span class="label-text">Show stacks in timeline</span>
      </label>
      <label>
        <span class="label-text">Auto-delete from trash after</span>
        <span class="number-with-unit">
          <input type="number" min="1" max="365" value={s.trash_auto_delete_days}
            onchange={(e) => patch({ trash_auto_delete_days: Number((e.target as HTMLInputElement).value) })} />
          <span class="unit mono">days</span>
        </span>
      </label>
      <label>
        <span class="label-text">
          Thumbnail cache <span class="hint">(per drive, on disk)</span>
        </span>
        <span class="number-with-unit">
          <select
            value={String(s.thumbnail_cache_gb)}
            onchange={(e) => patch({ thumbnail_cache_gb: Number((e.target as HTMLSelectElement).value) })}
          >
            <option value="1">1 GB</option>
            <option value="2">2 GB</option>
            <option value="5">5 GB</option>
            <option value="10">10 GB</option>
            <option value="25">25 GB</option>
          </select>
          <span class="unit hint mono">on next library open</span>
        </span>
      </label>
      <div class="subsection">
        <div class="section-heading-row">
          <div>
            <h4 class="subsection-title">Capture dates</h4>
            <p class="hint blurb">
              Re-read dates for every photo and video using embedded metadata first, filename dates next, and file modified time only as a fallback.
            </p>
          </div>
          <button class="ghost" onclick={refreshPhotoDates} disabled={refreshingDates}>
            {refreshingDates ? "Refreshing dates..." : "Refresh dates"}
          </button>
        </div>
      </div>
      <div class="subsection">
        <div class="section-heading-row">
          <div>
            <h4 class="subsection-title">Excluded folders</h4>
            <p class="hint blurb">
              Folders Smriti skips during scans and reindexing. Existing files are removed from Smriti only; files on disk stay untouched.
            </p>
          </div>
          <button class="ghost" onclick={addExclusion} disabled={exclusionsBusy || exclusionsActing}>
            {exclusionsActing ? "Working..." : "Exclude folder..."}
          </button>
        </div>
        {#if exclusionsBusy}
          <p class="hint blurb">Checking exclusions...</p>
        {:else if exclusions === null}
          <p class="hint blurb">Open a library to manage excluded folders.</p>
        {:else if exclusions.length === 0}
          <p class="hint blurb">No excluded folders.</p>
        {:else}
          <div class="exclusion-list" role="list" aria-label="Excluded folders">
            {#each exclusions as item}
              <div class="exclusion-row" role="listitem">
                <div class="exclusion-main">
                  <strong class="mono" title={item.relative_path}>{item.relative_path}</strong>
                  <span class="hint">
                    {item.indexed_count === 0
                      ? "No indexed items"
                      : `${item.indexed_count.toLocaleString()} indexed ${item.indexed_count === 1 ? "item" : "items"} remaining`}
                  </span>
                </div>
                <button class="ghost danger-soft" onclick={() => removeExclusion(item.relative_path)} disabled={exclusionsActing}>
                  Remove
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>

    <section class="full-width assets-section">
      <div class="section-heading-row">
        <h3 class="section-title">Assets</h3>
        <span class="hint mono">
          {#if assets}
            {installedCount(assets.assets)} / {managedCount(assets.assets)} ready · {formatBytes(assets.total_size_bytes)}
          {:else}
            Checking...
          {/if}
        </span>
      </div>
      <p class="hint blurb">
        Local runtimes, models, and offline data live outside the app binary. Smriti uses these when available and keeps browsing usable when optional assets are missing.
      </p>
      <div class="asset-actions">
        <button class="primary" onclick={installAssets} disabled={installingAssets}>
          {installingAssets ? "Installing..." : "Download assets"}
        </button>
        <button class="ghost" onclick={loadAssets} disabled={assetsBusy}>
          {assetsBusy ? "Checking..." : "Recheck"}
        </button>
      </div>
      <div class="semantic-panel">
        <div class="section-heading-row">
          <div>
            <h4 class="subsection-title">Visual search</h4>
            <p class="hint blurb">
              Search by image meaning and find visually similar photos. The model is optional and stored outside the app binary.
            </p>
          </div>
          <span class="asset-status" data-status={visualSearchReady ? "active" : "missing"}>
            {visualSearchStatus}
          </span>
        </div>
        {#if semanticStatus}
          <div class="semantic-stats" aria-label="Visual search status">
            <span><strong>{semanticStatus.display_name}</strong></span>
            <span class="mono">{semanticStatus.indexed_photos.toLocaleString()} indexed</span>
            <span class="mono">{semanticStatus.pending_photos.toLocaleString()} pending</span>
            <span class="mono">{semanticStatus.failed_photos.toLocaleString()} failed</span>
            <span class="mono">{formatBytes(semanticStatus.vector_bytes)} vectors</span>
          </div>
          {#if devMode.enabled}
            <p class="asset-path mono" title={semanticStatus.model_dir}>{semanticStatus.model_dir}</p>
          {/if}
          {#if visualSearchMissingRuntime}
            <p class="hint blurb">ONNX Runtime is missing. Click Download assets, then recheck visual search before indexing.</p>
          {/if}
        {:else}
          <p class="hint blurb">Open a library to see visual search status.</p>
        {/if}
        <div class="asset-actions">
          <button class="primary" onclick={installSemanticModel} disabled={semanticRunning || semanticStatus?.assets_installed}>
            {semanticRunning ? "Working..." : "Download visual model"}
          </button>
          <button class="ghost" onclick={startSemanticIndexing} disabled={semanticRunning || !visualSearchReady}>
            {semanticRunning ? "Indexing..." : "Index visual search"}
          </button>
          <button class="ghost" onclick={loadSemanticStatus} disabled={semanticRunning}>
            Recheck visual search
          </button>
        </div>
      </div>
      {#if assets && hasExternalAssets(assets)}
        <p class="hint blurb">
          Some assets are being read from another search root. This is normal in development checkouts and portable installs; the table shows the exact active locations.
        </p>
      {/if}
      {#if assets && devMode.enabled}
        <details class="asset-roots">
          <summary>Search roots</summary>
          <ul>
            {#each assets.roots as root}
              <li class="mono">{root}</li>
            {/each}
          </ul>
        </details>
      {/if}
      {#if assets}
        <div class="asset-table" role="table" aria-label="Installed assets">
          <div class="asset-row asset-head" role="row">
            <span role="columnheader">Asset</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Size</span>
            <span role="columnheader">Location</span>
          </div>
          {#each assets.assets as item}
            <div class="asset-row" role="row">
              <span class="asset-name" role="cell">
                <strong>{item.label}</strong>
                {#if item.note}<small>{item.note}</small>{/if}
              </span>
              <span role="cell">
                <span class="asset-status" data-status={item.status}>{item.status}</span>
              </span>
              <span class="mono" role="cell">{formatBytes(item.size_bytes)}</span>
              <span class="asset-path mono" role="cell" title={item.path ?? ""}>
                {item.path ?? "-"}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section>
      <h3 class="section-title">Faces &amp; people</h3>
      <label>
        <span class="label-text">Detection confidence</span>
        <input type="number" min="0.1" max="0.95" step="0.05" value={s.face_detection_confidence}
          onchange={(e) => patch({ face_detection_confidence: Number((e.target as HTMLInputElement).value) })} />
      </label>
      <label>
        <span class="label-text">Clustering threshold</span>
        <input type="number" min="0.1" max="0.8" step="0.02" value={s.face_clustering_threshold}
          onchange={(e) => patch({ face_clustering_threshold: Number((e.target as HTMLInputElement).value) })} />
      </label>
      <p class="hint blurb">
        Tighter clustering threshold = more "different" decisions; looser = more "same".
        {#if devMode.enabled}Adjust then use Reset clusters below to re-group existing faces.{/if}
      </p>
      {#if devMode.enabled}
        <div class="action-row">
          <button class="ghost danger-soft" onclick={resetFaceClusters} disabled={acting}>
            Reset face clusters only
          </button>
          <button class="ghost danger" onclick={runFacesFromScratch} disabled={acting}>
            Run faces from scratch
          </button>
        </div>
      {/if}
    </section>

    <section>
      <h3 class="section-title">Bursts</h3>
      <label>
        <span class="label-text">
          Burst time window
          <span class="hint">(photos taken within this window are grouped)</span>
        </span>
        <span class="number-with-unit">
          <input
            type="number"
            min="1"
            max="30"
            value={s.burst_time_window_seconds}
            onchange={(e) =>
              patch({
                burst_time_window_seconds: Number((e.target as HTMLInputElement).value),
              })}
          />
          <span class="unit mono">seconds</span>
        </span>
      </label>
      <p class="hint blurb">
        Tighter window = only true rapid-fire bursts get grouped.
        Looser window pulls in slower handheld sequences. Default 3 s
        matches typical phone burst-mode timing. Re-run "Detect bursts"
        on the Bursts page after a change.
      </p>
    </section>

    <section>
      <h3 class="section-title">Memories &amp; suggestions</h3>
      <label class="checkbox">
        <input type="checkbox" checked={s.memories_enabled}
          onchange={(e) => patch({ memories_enabled: (e.target as HTMLInputElement).checked })} />
        <span class="label-text">Enable memories</span>
      </label>
      <label>
        <span class="label-text">
          Home city <span class="hint">(optional, for trip detection)</span>
        </span>
        <input value={s.home_city_override ?? ""}
          onchange={(e) => patch({ home_city_override: ((e.target as HTMLInputElement).value || null) })} />
      </label>
      {#if devMode.enabled}
        <div class="action-row">
          <button class="ghost danger-soft" onclick={resetSuggestions} disabled={acting}>
            Reset all suggestions
          </button>
        </div>
        <p class="hint blurb">
          "Reset all suggestions" wipes both pending and dismissed suggestion records.
          Use it after detector improvements so previously-dismissed runs can return.
        </p>
      {/if}
    </section>

    <section>
      <h3 class="section-title">Places</h3>
      <p class="hint blurb">
        Resolves GPS coordinates to city/country using the bundled GeoNames database.
        Run this once after first setup, or after a scan that ran with the database missing.
      </p>
      <div class="action-row">
        <button class="primary" onclick={() => backfillGeocoding(false)} disabled={backfilling}>
          {backfilling ? "Resolving…" : "Fill in place names"}
        </button>
        {#if devMode.enabled}
          <button class="ghost danger-soft" onclick={() => backfillGeocoding(true)} disabled={backfilling}
            title="Re-resolve every GPS-tagged photo, overwriting stale matches">
            Refresh all
          </button>
        {/if}
      </div>
    </section>

    {#if devMode.enabled && healthData}
      {@const h = healthData}
      <section class="full-width">
        <h3 class="section-title">Library health</h3>
        <!-- "Missing thumbnails" used to live here but the count was
             misleading: on-demand thumbnail generation makes "missing"
             a transient state, not an actionable defect. Removed to
             reduce noise. Dev mode now has a dedicated Regenerate
             button below for the use case it was hinting at. -->
        <ul class="counters">
          {#each [
            { n: h.total_photos, label: "Total photos", tone: "neutral" },
            { n: h.inaccurate_dates, label: "Inaccurate dates", tone: h.inaccurate_dates > 0 ? "warn" : "ok", note: "Pulled from mtime — possibly wrong year." },
            { n: h.missing_dates, label: "No date at all", tone: h.missing_dates > 0 ? "warn" : "ok" },
            { n: h.heic_count, label: "HEIC photos", tone: h.heic_decoder_available || h.heic_count === 0 ? "ok" : "warn", note: h.heic_decoder_available ? "Decoder available." : "Decoder NOT available — these won't render." },
            { n: h.face_processed_no_faces, label: "Processed but no faces", tone: "neutral", note: "Likely face-less landscapes; nothing to fix." },
          ] as stat}
            <li class="counter" data-tone={stat.tone}>
              <strong class="num">{stat.n.toLocaleString()}</strong>
              <span class="lbl">{stat.label}</span>
              {#if stat.note}<span class="note">{stat.note}</span>{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if devMode.enabled}
      <section>
        <h3 class="section-title">Maintenance</h3>
        <p class="hint blurb">
          Re-generate every cached thumbnail at the current quality.
          Useful after upgrading from a version that produced smaller
          thumbs, or if the cache files have rotted on disk.
        </p>
        <div class="action-row">
          <button class="ghost" onclick={regenerateThumbs} disabled={regeneratingThumbs}>
            {regeneratingThumbs ? "Regenerating…" : "Regenerate thumbnails"}
          </button>
          <button class="ghost" onclick={refreshStacks} disabled={refreshingStacks}>
            {refreshingStacks ? "Refreshing…" : "Refresh stacks"}
          </button>
        </div>
      </section>
    {/if}

    <section>
      <h3 class="section-title">Updates</h3>
      <label class="checkbox">
        <input type="checkbox" checked={s.auto_update_check_enabled}
          onchange={(e) => patch({ auto_update_check_enabled: (e.target as HTMLInputElement).checked })} />
        <span class="label-text">Check for updates automatically</span>
      </label>
    </section>

    {#if devMode.enabled}
    <section>
      <h3 class="section-title">Cloud face acceleration (advanced)</h3>
      <p class="hint blurb">
        Offload face embedding to a free GPU notebook you control.
        Sends only 112×112 face crops (not photos) to a URL you provide.
        Uses local CPU as a fallback only when the matching local embedding model is installed.
        <a href="https://github.com/anomalyco/photovault/blob/main/docs/face-gpu-bridge.md" target="_blank" rel="noopener" class="inline-link">How to set up a free Kaggle / Colab notebook →</a>
      </p>
      <label class="checkbox" style="margin-bottom: var(--s-3)">
        <input type="checkbox" checked={s.face_gpu_bridge_enabled}
          onchange={async (e) => {
            const enabled = (e.target as HTMLInputElement).checked;
            await patch({ face_gpu_bridge_enabled: enabled });
            if (!enabled) await patch({ face_gpu_bridge_url: null });
          }} />
        <span class="label-text">Use a remote GPU for face embedding</span>
      </label>
      {#if s.face_gpu_bridge_enabled}
        <label>
          <span class="label-text">Bridge URL</span>
          <div class="url-row">
            <input
              style="max-width: 320px"
              value={s.face_gpu_bridge_url ?? ""}
              placeholder="e.g. https://abc.ngrok.io"
              onchange={(e) => patch({ face_gpu_bridge_url: ((e.target as HTMLInputElement).value.trim() || null) })} />
            <button class="ghost" onclick={testBridge} disabled={testBusy || !s.face_gpu_bridge_url}>
              {testBusy ? "Testing…" : "Test connection"}
            </button>
          </div>
        </label>
        {#if testResult}
          <p class="test-result" class:ok={testResult.ok} class:fail={!testResult.ok}>
            {testResult.ok
              ? `✓ ${testResult.gpu_name} · ${testResult.model ?? "model ok"} @ ${testResult.latency_ms}ms`
              : `✕ ${testResult.gpu_name}`}
          </p>
        {/if}
      {/if}
    </section>
    {/if}

    <!-- Always-visible: the developer-mode toggle lives at the bottom
         so people who want it can find it, but it never gets in the
         way of the simple settings the rest of the time. -->
    <section>
      <h3 class="section-title">Developer</h3>
      <label class="checkbox">
        <input type="checkbox" checked={devMode.enabled}
          onchange={(e) => devMode.set((e.target as HTMLInputElement).checked)} />
        <span class="label-text">Developer mode</span>
      </label>
      <p class="hint blurb">
        Reveals advanced controls: regenerate thumbnails, reset face
        clusters, run faces from scratch, refresh all places, the
        cloud-GPU bridge, and library health counters. Off by default
        — most users never need these.
      </p>
    </section>
  {/if}
</div>

<style>
  .page {
    padding: var(--s-5) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
  }
  /* Cap individual sections at a readable width while letting the
     scroll surface fill the window. Lines of body text shouldn't run
     edge-to-edge on a 4K screen, but the scroll area should — that's
     why this is on `section`, not `.page`. */
  section { max-width: 720px; }
  /* Library health counters benefit from more horizontal room — they
     are tiles, not prose. */
  section.full-width { max-width: none; }
  .status { font-size: var(--t-sm); color: var(--accent); }
  section { margin-bottom: var(--s-6); }
  .section-title {
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0 0 var(--s-3);
  }
  .subsection {
    padding-top: var(--s-4);
  }
  .subsection-title {
    margin: 0 0 var(--s-1);
    color: var(--ink);
    font-size: var(--t-sm);
    font-weight: 600;
  }
  .section-heading-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--s-3);
  }

  label {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) 0;
    border-bottom: 1px solid var(--line-soft);
  }
  label.checkbox {
    grid-template-columns: auto 1fr;
    cursor: pointer;
  }
  .label-text {
    font-size: var(--t-base);
    color: var(--ink);
  }
  .hint {
    font-size: var(--t-xs);
    color: var(--ink-muted);
    font-style: italic;
  }
  .number-with-unit {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .inline-field {
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    flex-wrap: wrap;
  }
  .unit { color: var(--ink-muted); font-size: var(--t-xs); }
  input, select { max-width: 200px; }
  input[type="checkbox"] { max-width: none; width: auto; margin-right: 0; }
  .blurb {
    color: var(--ink-soft);
    font-size: var(--t-sm);
    line-height: 1.5;
    margin: 0 0 var(--s-3);
  }
  .action-row { display: flex; gap: var(--s-2); flex-wrap: wrap; }
  .exclusion-list {
    border-top: 1px solid var(--line-soft);
    border-bottom: 1px solid var(--line-soft);
  }
  .exclusion-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-2) 0;
    border-top: 1px solid var(--line-soft);
  }
  .exclusion-row:first-child { border-top: 0; }
  .exclusion-main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .exclusion-main strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink);
    font-size: var(--t-sm);
    font-weight: 500;
  }
  .assets-section { max-width: 980px; }
  .asset-actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    margin-bottom: var(--s-3);
    flex-wrap: wrap;
  }
  .asset-roots {
    margin: 0 0 var(--s-3);
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
  .asset-roots summary {
    cursor: pointer;
    color: var(--ink-soft);
    margin-bottom: var(--s-2);
  }
  .asset-roots ul {
    margin: 0;
    padding-left: var(--s-4);
  }
  .asset-roots li {
    margin-bottom: 4px;
    overflow-wrap: anywhere;
  }
  .semantic-panel {
    border-top: 1px solid var(--line-soft);
    border-bottom: 1px solid var(--line-soft);
    padding: var(--s-3) 0;
    margin: var(--s-3) 0;
  }
  .semantic-stats {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2) var(--s-4);
    align-items: center;
    color: var(--ink-soft);
    font-size: var(--t-sm);
    margin-bottom: var(--s-2);
  }
  .semantic-stats strong {
    color: var(--ink);
    font-weight: 600;
  }
  .asset-table {
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .asset-row {
    display: grid;
    grid-template-columns: minmax(190px, 1.2fr) 92px 80px minmax(220px, 1fr);
    gap: var(--s-3);
    align-items: center;
    padding: var(--s-2) 0;
    border-top: 1px solid var(--line-soft);
    min-height: 48px;
  }
  .asset-row:first-child { border-top: 0; }
  .asset-head {
    min-height: auto;
    color: var(--ink-muted);
    font-size: var(--t-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .asset-name {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .asset-name strong {
    color: var(--ink);
    font-size: var(--t-sm);
    font-weight: 600;
  }
  .asset-name small {
    color: var(--ink-muted);
    font-size: var(--t-xs);
    line-height: 1.35;
  }
  .asset-status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 70px;
    height: 24px;
    border-radius: var(--r-sm);
    border: 1px solid var(--line);
    font-size: var(--t-xs);
    text-transform: capitalize;
  }
  .asset-status[data-status="active"] {
    color: var(--keep);
    border-color: color-mix(in oklab, var(--keep) 50%, var(--line));
    background: color-mix(in oklab, var(--keep) 8%, transparent);
  }
  .asset-status[data-status="extra"] {
    color: var(--ink-soft);
    background: var(--bg-paper);
  }
  .asset-status[data-status="missing"] {
    color: var(--accent);
    border-color: color-mix(in oklab, var(--accent) 55%, var(--line));
    background: color-mix(in oklab, var(--accent) 8%, transparent);
  }
  .asset-status[data-status="planned"] {
    color: var(--ink-muted);
    border-style: dashed;
  }
  .asset-path {
    color: var(--ink-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  @media (max-width: 760px) {
    .section-heading-row {
      align-items: flex-start;
      flex-direction: column;
    }
    .exclusion-row {
      grid-template-columns: 1fr;
    }
    .asset-row {
      grid-template-columns: 1fr 82px;
      gap: var(--s-2);
    }
    .asset-head { display: none; }
    .asset-path {
      grid-column: 1 / -1;
      font-size: var(--t-xs);
    }
  }
  /* Destructive-action affordances. `.danger-soft` reads as "careful"
     but doesn't shout; `.danger` warns hot. */
  button.danger-soft {
    color: var(--ink-soft);
    border-color: var(--line);
  }
  button.danger-soft:hover:not(:disabled) {
    color: var(--hot, #d05a4a);
    border-color: var(--hot, #d05a4a);
  }
  button.danger {
    color: var(--hot, #d05a4a);
    border-color: color-mix(in oklab, var(--hot, #d05a4a) 60%, var(--line));
  }
  button.danger:hover:not(:disabled) {
    background: color-mix(in oklab, var(--hot, #d05a4a) 8%, transparent);
    border-color: var(--hot, #d05a4a);
  }

  /* Library health counters — moved here from the dedicated /health route. */
  .counters {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--s-3);
  }
  .counter {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    padding: var(--s-3) var(--s-4);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-left-width: 3px;
  }
  .counter[data-tone="ok"]   { border-left-color: var(--keep); }
  .counter[data-tone="warn"] { border-left-color: var(--accent); }
  .counter[data-tone="neutral"] { border-left-color: var(--ink-faint); }
  .counter .num {
    font-family: var(--font-display);
    font-size: var(--t-2xl);
    font-weight: 500;
    line-height: 1;
    color: var(--ink);
    font-variation-settings: "opsz" 36;
  }
  .counter .lbl { font-size: var(--t-sm); color: var(--ink); font-weight: 500; }
  .counter .note { font-size: var(--t-xs); color: var(--ink-muted); margin-top: 2px; }
  .url-row { display: flex; gap: var(--s-2); align-items: center; }
  .test-result { font-size: var(--t-sm); margin-top: var(--s-2); padding: var(--s-2); border-radius: var(--r-sm); }
  .test-result.ok { color: var(--keep); background: color-mix(in oklab, var(--keep) 10%, transparent); }
  .test-result.fail { color: var(--hot, #d05a4a); background: color-mix(in oklab, var(--hot, #d05a4a) 10%, transparent); }
  .inline-link { color: var(--accent); text-decoration: underline; font-style: normal; }
</style>
