<script lang="ts">
  import { onMount } from "svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { albums, geocoding, health } from "../lib/api/all";
  import { toasts } from "../lib/stores/toast.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { LibraryHealthData, Settings } from "../lib/api/all";

  let saving = $state(false);
  let error = $state<string | null>(null);
  let backfilling = $state(false);
  let healthData = $state<LibraryHealthData | null>(null);

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
    backfilling = true;
    try {
      const r = await geocoding.backfill(force);
      if (!r.geonames_db_present) {
        toasts.error("GeoNames database not found. Run scripts/setup_assets.sh");
      } else if (r.considered === 0) {
        toasts.success("No photos need geocoding — all GPS-tagged photos already have place names.");
      } else if (r.cleared > 0) {
        toasts.success(`Resolved ${r.updated}, cleared ${r.cleared} stale match${r.cleared === 1 ? "" : "es"}.`);
      } else {
        toasts.success(`Resolved ${r.updated} of ${r.considered} GPS-tagged photos.`);
      }
      // Album suggestions (trip / event detection) need location data
      // to fire — so re-run detection now that the place names are
      // populated. Failure is non-fatal: the user can re-run from the
      // Albums tab.
      if (r.geonames_db_present && r.updated > 0) {
        try {
          const det = await albums.suggestions.runDetection();
          if (det.created > 0) {
            toasts.info(`Found ${det.created} new album suggestion${det.created === 1 ? "" : "s"}.`);
          }
        } catch {}
      }
    } catch (e) {
      toasts.error(`Geocoding backfill failed: ${e}`);
    } finally {
      backfilling = false;
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
      <h3 class="section-title">Face recognition</h3>
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
    </section>

    <section>
      <h3 class="section-title">Memories</h3>
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
        <button class="ghost" onclick={() => backfillGeocoding(true)} disabled={backfilling}
          title="Re-resolve every GPS-tagged photo, overwriting stale matches">
          Refresh all
        </button>
      </div>
    </section>

    {#if healthData}
      {@const h = healthData}
      <section>
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
    max-width: 680px;
  }
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
  .action-row { display: flex; gap: var(--s-2); }

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
