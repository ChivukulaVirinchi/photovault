<script lang="ts">
  import { onMount } from "svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { Settings } from "../lib/api/all";

  let saving = $state(false);
  let error = $state<string | null>(null);

  onMount(() => { settingsStore.load(); });

  const s = $derived(settingsStore.data);

  async function patch(p: Partial<Settings>) {
    if (!s) return;
    saving = true;
    try { await settingsStore.update(p); }
    catch (e) { error = JSON.stringify(e); }
    finally { saving = false; }
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
</style>
