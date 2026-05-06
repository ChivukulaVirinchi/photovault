<script lang="ts">
  import { insights } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
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

<PageHeader title="Insights">
  {#if data}
    <select bind:value={year}>
      <option value={null}>All time</option>
      {#each data.available_years as y}
        <option value={y}>{y}</option>
      {/each}
    </select>
  {/if}
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if data}
    <section class="stats">
      {#each [
        { n: data.total_photos, label: "photos" },
        { n: data.people_count, label: "people" },
        { n: data.album_count, label: "albums" },
        { n: data.country_count, label: "countries" },
        { n: data.city_count, label: "cities" },
      ] as stat}
        <div class="stat">
          <strong class="num">{stat.n.toLocaleString()}</strong>
          <span class="lbl">{stat.label}</span>
        </div>
      {/each}
    </section>

    <section>
      <h3 class="section-title">Rhythm by month</h3>
      <div class="bars">
        {#each data.monthly_counts as count, i}
          {@const max = Math.max(1, ...data.monthly_counts)}
          <div class="bar-col">
            <div class="bar" style="height: {(count / max) * 100}%" title="{count} photos"></div>
            <span class="month mono">{["J","F","M","A","M","J","J","A","S","O","N","D"][i]}</span>
          </div>
        {/each}
      </div>
    </section>

    {#if data.top_people.length > 0}
      <section>
        <h3 class="section-title">Faces you see often</h3>
        <ul class="row">
          {#each data.top_people as p}
            <li>
              <a href="#/person?id={p.cluster_id}">
                {#if p.face_crop_path}
                  <img src={thumbUrl(libraryStore.driveRoot, p.face_crop_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder"></span>
                {/if}
                <span class="info">
                  <strong>{p.name}</strong>
                  <span class="muted small mono">{p.photo_count}</span>
                </span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if data.top_locations.length > 0}
      <section>
        <h3 class="section-title">Places you've been</h3>
        <ul class="locations">
          {#each data.top_locations as l}
            <li>
              <span class="city">{l.city}</span>
              <span class="country">, {l.country}</span>
              <span class="muted small mono">{l.photo_count}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: var(--s-3);
    margin-bottom: var(--s-6);
  }
  .stat {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    padding: var(--s-4) var(--s-4);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .stat .num {
    font-family: var(--font-display);
    font-size: var(--t-2xl);
    font-weight: 500;
    line-height: 1;
    color: var(--ink);
    font-variation-settings: "opsz" 36;
  }
  .stat .lbl {
    font-size: var(--t-xs);
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  section { margin-bottom: var(--s-6); }
  .section-title {
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0 0 var(--s-3);
  }

  .bars {
    display: flex;
    align-items: stretch;
    gap: 6px;
    height: 180px;
    padding: var(--s-3);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
  }
  .bar-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
  }
  .bar {
    width: 70%;
    background: var(--accent);
    border-radius: 2px 2px 0 0;
    min-height: 2px;
    opacity: 0.85;
    transition: opacity var(--t-fast) var(--ease);
  }
  .bar:hover { opacity: 1; }
  .month {
    font-size: 10px;
    color: var(--ink-faint);
    letter-spacing: 0.05em;
  }

  .row {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .row li {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    transition: border-color var(--t-fast) var(--ease);
  }
  .row li:hover { border-color: var(--accent); }
  .row a {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 6px var(--s-3) 6px 6px;
    color: inherit;
    text-decoration: none;
  }
  .row img, .placeholder {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .placeholder { background: var(--bg-elev); }
  .info { display: flex; flex-direction: column; }

  .locations {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .locations li {
    padding: var(--s-3) var(--s-4);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
  }
  .city {
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
  }
  .country { color: var(--ink-muted); }
  .locations .muted { margin-left: auto; }
  .small { font-size: var(--t-xs); }
</style>
