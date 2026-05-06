<script lang="ts">
  import { onMount } from "svelte";
  import { insights } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import type { InsightsData } from "../lib/api/all";

  let data = $state<InsightsData | null>(null);
  let year = $state<number | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try { data = await insights.compute(year); }
    catch (e) { error = JSON.stringify(e); }
  }

  $effect(() => { void year; load(); });
</script>

<main class="insights">
  <header>
    <h2>Insights</h2>
    {#if data}
      <select bind:value={year}>
        <option value={null}>All time</option>
        {#each data.available_years as y}
          <option value={y}>{y}</option>
        {/each}
      </select>
    {/if}
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if data}
    <section class="stats">
      <div class="stat"><strong>{data.total_photos.toLocaleString()}</strong><span class="muted">photos</span></div>
      <div class="stat"><strong>{data.people_count}</strong><span class="muted">people</span></div>
      <div class="stat"><strong>{data.album_count}</strong><span class="muted">albums</span></div>
      <div class="stat"><strong>{data.country_count}</strong><span class="muted">countries</span></div>
      <div class="stat"><strong>{data.city_count}</strong><span class="muted">cities</span></div>
    </section>

    <section>
      <h3>Monthly</h3>
      <div class="bars">
        {#each data.monthly_counts as count, i}
          {@const max = Math.max(1, ...data.monthly_counts)}
          <div class="bar" style="height: {(count / max) * 100}%" title="{count}">
            <span class="muted small">{["J","F","M","A","M","J","J","A","S","O","N","D"][i]}</span>
          </div>
        {/each}
      </div>
    </section>

    {#if data.top_people.length > 0}
      <section>
        <h3>Top people</h3>
        <ul class="row">
          {#each data.top_people as p}
            <li>
              <a href="#/person?id={p.cluster_id}">
                {#if p.face_crop_path}
                  <img src={thumbUrl(libraryStore.driveRoot, p.face_crop_path) ?? ""} alt="" />
                {/if}
                <strong>{p.name}</strong>
                <span class="muted small">{p.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if data.top_locations.length > 0}
      <section>
        <h3>Top locations</h3>
        <ul class="locations">
          {#each data.top_locations as l}
            <li>{l.city}, {l.country} <span class="muted small">{l.photo_count}</span></li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</main>

<style>
  .insights { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 20px; }
  h2 { margin: 0; }
  section { margin-bottom: 28px; }
  h3 { margin: 0 0 10px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.06em; color: #a8a8af; }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 12px; }
  .stat { background: #131316; padding: 16px 18px; border-radius: 8px; display: flex; flex-direction: column; gap: 4px; }
  .stat strong { font-size: 22px; }
  .bars { display: flex; align-items: flex-end; gap: 4px; height: 160px; padding: 8px; background: #0f0f12; border-radius: 6px; }
  .bar { flex: 1; background: #6aa9ff; border-radius: 4px 4px 0 0; min-height: 4px; position: relative; display: flex; align-items: flex-end; justify-content: center; padding-bottom: 2px; }
  .bar .small { font-size: 10px; color: #fff; }
  .row { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: 8px; }
  .row li { background: #131316; border-radius: 8px; }
  .row a { display: flex; align-items: center; gap: 10px; padding: 8px 12px; color: inherit; }
  .row a:hover { text-decoration: none; background: #1a1a1f; }
  .row img { width: 32px; height: 32px; border-radius: 50%; object-fit: cover; }
  .locations { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  .locations li { padding: 8px 12px; background: #131316; border-radius: 6px; }
  .small { font-size: 11px; }
</style>
