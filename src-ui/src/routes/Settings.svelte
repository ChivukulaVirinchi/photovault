<script lang="ts">
  import { onMount } from "svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { albums, geocoding, health, people } from "../lib/api/all";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { LibraryHealthData, Settings } from "../lib/api/all";

  let saving = $state(false);
  let error = $state<string | null>(null);
  let healthData = $state<LibraryHealthData | null>(null);
  let acting = $state(false);
  // Backfill state derived from the global jobs store so progress
  // survives navigation (a 50k-photo backfill is a multi-second job).
  const backfilling = $derived(jobs.isRunning("geocoding"));
  const geocodingJob = $derived(jobs.byKind("geocoding"));

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
        Adjust then use Reset clusters below to re-group existing faces.
      </p>
      <div class="action-row">
        <button class="ghost danger-soft" onclick={resetFaceClusters} disabled={acting}>
          Reset face clusters only
        </button>
        <button class="ghost danger" onclick={runFacesFromScratch} disabled={acting}>
          Run faces from scratch
        </button>
      </div>
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
      <div class="action-row">
        <button class="ghost danger-soft" onclick={resetSuggestions} disabled={acting}>
          Reset all suggestions
        </button>
      </div>
      <p class="hint blurb">
        "Reset all suggestions" wipes both pending and dismissed suggestion records.
        Use it after detector improvements so previously-dismissed runs can return.
      </p>
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
        <button class="ghost danger-soft" onclick={() => backfillGeocoding(true)} disabled={backfilling}
          title="Re-resolve every GPS-tagged photo, overwriting stale matches">
          Refresh all
        </button>
      </div>
    </section>

    {#if healthData}
      {@const h = healthData}
      <section class="full-width">
        <h3 class="section-title">Library health</h3>
        <ul class="counters">
          {#each [
            { n: h.total_photos, label: "Total photos", tone: "neutral" },
            { n: h.missing_thumbnails, label: "Missing thumbnails", tone: h.missing_thumbnails > 0 ? "warn" : "ok" },
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

    <section>
      <h3 class="section-title">Updates</h3>
      <label class="checkbox">
        <input type="checkbox" checked={s.auto_update_check_enabled}
          onchange={(e) => patch({ auto_update_check_enabled: (e.target as HTMLInputElement).checked })} />
        <span class="label-text">Check for updates automatically</span>
      </label>
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
</style>
