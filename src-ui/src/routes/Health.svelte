<script lang="ts">
  import { onMount } from "svelte";
  import { health } from "../lib/api/all";
  import type { LibraryHealthData } from "../lib/api/all";

  let data = $state<LibraryHealthData | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try { data = await health.compute(); }
    catch (e) { error = JSON.stringify(e); }
  }

  onMount(load);
</script>

<main class="health">
  <header><h2>Library health</h2></header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if data}
    <ul class="counters">
      <li><strong>{data.total_photos.toLocaleString()}</strong><span class="muted">total photos</span></li>
      <li><strong>{data.missing_thumbnails.toLocaleString()}</strong><span class="muted">missing thumbnails</span></li>
      <li><strong>{data.inaccurate_dates.toLocaleString()}</strong><span class="muted">inaccurate dates (mtime fallback)</span></li>
      <li><strong>{data.missing_dates.toLocaleString()}</strong><span class="muted">no date at all</span></li>
      <li>
        <strong>{data.heic_count.toLocaleString()}</strong>
        <span class="muted">HEIC photos {data.heic_decoder_available ? "(decoder available)" : "(decoder NOT available)"}</span>
      </li>
      <li><strong>{data.face_processed_no_faces.toLocaleString()}</strong><span class="muted">processed but found no faces</span></li>
    </ul>
  {/if}
</main>

<style>
  .health { flex: 1; overflow-y: auto; padding: 20px; }
  header { margin-bottom: 16px; }
  h2 { margin: 0; }
  .counters { list-style: none; padding: 0; margin: 0; display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .counters li { background: #131316; padding: 16px; border-radius: 8px; display: flex; flex-direction: column; gap: 4px; }
  .counters strong { font-size: 22px; }
</style>
