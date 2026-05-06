<script lang="ts">
  import { onMount } from "svelte";
  import { settings as settingsApi } from "../lib/api/all";
  import type { Settings } from "../lib/api/all";

  let s = $state<Settings | null>(null);
  let saving = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try { s = await settingsApi.get(); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function patch(p: Partial<Settings>) {
    if (!s) return;
    saving = true;
    try {
      s = await settingsApi.update(p);
    } catch (e) { error = JSON.stringify(e); }
    finally { saving = false; }
  }

  onMount(load);
</script>

<main class="settings">
  <header>
    <h2>Settings</h2>
    {#if saving}<span class="muted small">Saving…</span>{/if}
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if s}
    <section>
      <h3>Appearance</h3>
      <label>
        Theme
        <select value={s.theme} onchange={(e) => patch({ theme: (e.target as HTMLSelectElement).value })}>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
          <option value="system">System</option>
        </select>
      </label>
      <label>
        Date format
        <select value={s.date_format} onchange={(e) => patch({ date_format: (e.target as HTMLSelectElement).value })}>
          <option value="locale">Locale</option>
          <option value="iso">ISO</option>
          <option value="us">US</option>
          <option value="eu">EU</option>
        </select>
      </label>
    </section>

    <section>
      <h3>Library</h3>
      <label>
        Thumbnail size
        <input
          type="number" min="100" max="1000"
          value={s.thumbnail_size}
          onchange={(e) => patch({ thumbnail_size: Number((e.target as HTMLInputElement).value) })}
        />
      </label>
      <label class="checkbox">
        <input
          type="checkbox" checked={s.scan_hidden_folders}
          onchange={(e) => patch({ scan_hidden_folders: (e.target as HTMLInputElement).checked })}
        />
        Scan hidden folders
      </label>
      <label>
        Trash auto-delete (days)
        <input
          type="number" min="1" max="365"
          value={s.trash_auto_delete_days}
          onchange={(e) => patch({ trash_auto_delete_days: Number((e.target as HTMLInputElement).value) })}
        />
      </label>
    </section>

    <section>
      <h3>Face recognition</h3>
      <label>
        Detection confidence
        <input
          type="number" min="0.1" max="0.95" step="0.05"
          value={s.face_detection_confidence}
          onchange={(e) => patch({ face_detection_confidence: Number((e.target as HTMLInputElement).value) })}
        />
      </label>
      <label>
        Clustering threshold
        <input
          type="number" min="0.1" max="0.8" step="0.02"
          value={s.face_clustering_threshold}
          onchange={(e) => patch({ face_clustering_threshold: Number((e.target as HTMLInputElement).value) })}
        />
      </label>
    </section>

    <section>
      <h3>Memories</h3>
      <label class="checkbox">
        <input
          type="checkbox" checked={s.memories_enabled}
          onchange={(e) => patch({ memories_enabled: (e.target as HTMLInputElement).checked })}
        />
        Enable Memories
      </label>
      <label>
        Home city (optional)
        <input
          value={s.home_city_override ?? ""}
          onchange={(e) => patch({ home_city_override: ((e.target as HTMLInputElement).value || null) })}
        />
      </label>
    </section>

    <section>
      <h3>Updates</h3>
      <label class="checkbox">
        <input
          type="checkbox" checked={s.auto_update_check_enabled}
          onchange={(e) => patch({ auto_update_check_enabled: (e.target as HTMLInputElement).checked })}
        />
        Check for updates automatically
      </label>
    </section>
  {/if}
</main>

<style>
  .settings { flex: 1; overflow-y: auto; padding: 20px; max-width: 600px; }
  header { display: flex; gap: 14px; align-items: baseline; margin-bottom: 20px; }
  h2 { margin: 0; }
  section { margin-bottom: 28px; }
  h3 { margin: 0 0 10px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.06em; color: #a8a8af; }
  label { display: flex; flex-direction: column; gap: 4px; margin-bottom: 12px; font-size: 14px; color: #c8c8cc; }
  label.checkbox { flex-direction: row; align-items: center; gap: 8px; }
  input, select { max-width: 220px; }
</style>
