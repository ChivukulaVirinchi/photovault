<script lang="ts">
  import { onMount } from "svelte";
  import { health } from "../lib/api/all";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { LibraryHealthData } from "../lib/api/all";

  let data = $state<LibraryHealthData | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try { data = await health.compute(); }
    catch (e) { error = JSON.stringify(e); }
  }

  onMount(load);
</script>

<PageHeader title="Library health" />

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if data}
    <ul class="counters">
      {#each [
        { n: data.total_photos, label: "Total photos", tone: "neutral" },
        { n: data.missing_thumbnails, label: "Missing thumbnails", tone: data.missing_thumbnails > 0 ? "warn" : "ok" },
        { n: data.inaccurate_dates, label: "Inaccurate dates", tone: data.inaccurate_dates > 0 ? "warn" : "ok", note: "Pulled from mtime — possibly wrong year." },
        { n: data.missing_dates, label: "No date at all", tone: data.missing_dates > 0 ? "warn" : "ok" },
        { n: data.heic_count, label: "HEIC photos", tone: data.heic_decoder_available || data.heic_count === 0 ? "ok" : "warn", note: data.heic_decoder_available ? "Decoder available." : "Decoder NOT available — these won't render." },
        { n: data.face_processed_no_faces, label: "Processed but no faces", tone: "neutral", note: "Likely face-less landscapes; nothing to fix." },
      ] as stat}
        <li class="counter" data-tone={stat.tone}>
          <strong class="num">{stat.n.toLocaleString()}</strong>
          <span class="lbl">{stat.label}</span>
          {#if stat.note}<span class="note">{stat.note}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .counters {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--s-3);
  }
  .counter {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    padding: var(--s-4) var(--s-5);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-left-width: 3px;
  }
  .counter[data-tone="ok"] { border-left-color: var(--keep); }
  .counter[data-tone="warn"] { border-left-color: var(--accent); }
  .counter[data-tone="neutral"] { border-left-color: var(--ink-faint); }
  .counter .num {
    font-family: var(--font-display);
    font-size: var(--t-3xl);
    font-weight: 500;
    line-height: 1;
    color: var(--ink);
    font-variation-settings: "opsz" 48;
  }
  .counter .lbl {
    font-size: var(--t-sm);
    color: var(--ink);
    font-weight: 500;
  }
  .counter .note {
    font-size: var(--t-xs);
    color: var(--ink-muted);
    margin-top: 2px;
  }
</style>
