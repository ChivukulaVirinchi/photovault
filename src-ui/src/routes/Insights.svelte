<script lang="ts">
  import { onMount } from "svelte";
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

<PageHeader
  num="10"
  label="INSIGHTS"
  title="A look at the whole."
  subtitle="The shape of your library — when you shoot, who's in it, where you've been."
>
  {#if data}
    <select bind:value={year}>
      <option value={null}>All time</option>
      {#each data.available_years as y}
        <option value={y}>{y}</option>
      {/each}
    </select>
  {/if}
</PageHeader>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if data}
    <section class="stats stagger">
      {#each [
        { n: data.total_photos, label: "photos" },
        { n: data.people_count, label: "people" },
        { n: data.album_count, label: "albums" },
        { n: data.country_count, label: "countries" },
        { n: data.city_count, label: "cities" },
      ] as stat, i}
        <div class="stat" style="--i: {i}">
          <strong class="num">{stat.n.toLocaleString()}</strong>
          <span class="label">{stat.label}</span>
        </div>
      {/each}
    </section>

    <section>
      <span class="eyebrow"><span class="ornament"></span>RHYTHM · BY MONTH</span>
      <div class="bars">
        {#each data.monthly_counts as count, i}
          {@const max = Math.max(1, ...data.monthly_counts)}
          <div class="bar-col">
            <div class="bar" style="height: {(count / max) * 100}%" title="{count}"></div>
            <span class="month mono">{["J","F","M","A","M","J","J","A","S","O","N","D"][i]}</span>
          </div>
        {/each}
      </div>
    </section>

    {#if data.top_people.length > 0}
      <section>
        <span class="eyebrow"><span class="ornament"></span>FACES YOU SEE OFTEN</span>
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
        <span class="eyebrow"><span class="ornament"></span>PLACES YOU'VE BEEN</span>
        <ul class="locations">
          {#each data.top_locations as l}
            <li>
              <span class="city">{l.city}</span>
              <em class="country">, {l.country}</em>
              <span class="muted small mono">{l.photo_count}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .page { padding: var(--s-6) var(--s-7); flex: 1; overflow-y: auto; }

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--s-3);
    margin-bottom: var(--s-7);
  }
  .stat {
    background: var(--bg-card);
    border: 1px solid var(--line);
    padding: var(--s-5);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .stat .num {
    font-family: var(--font-display);
    font-size: var(--t-3xl);
    font-weight: 500;
    line-height: 1;
    font-variation-settings: "opsz" 60, "SOFT" 50;
  }
  .stat .label {
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-muted);
  }

  section { margin-bottom: var(--s-7); }
  section .eyebrow { display: inline-flex; margin-bottom: var(--s-4); }

  .bars {
    display: flex;
    align-items: stretch;
    gap: 4px;
    height: 200px;
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
    width: 80%;
    background: linear-gradient(to top, var(--accent-deep), var(--accent));
    border-radius: 3px 3px 0 0;
    min-height: 2px;
    transition: opacity var(--t-fast) var(--ease);
  }
  .bar:hover { opacity: 0.85; }
  .month {
    font-size: 9px;
    color: var(--ink-faint);
    letter-spacing: 0.05em;
  }

  .row { list-style: none; padding: 0; margin: 0; display: flex; gap: var(--s-2); flex-wrap: wrap; }
  .row li { background: var(--bg-card); border: 1px solid var(--line); border-radius: var(--r-md); }
  .row a { display: flex; align-items: center; gap: var(--s-3); padding: var(--s-2) var(--s-3); color: inherit; }
  .row a:hover { text-decoration: none; background: var(--bg-elev); }
  .row img, .placeholder { width: 36px; height: 36px; border-radius: 50%; object-fit: cover; flex-shrink: 0; }
  .placeholder { background: var(--bg-elev); }
  .info { display: flex; flex-direction: column; }

  .locations { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 6px; }
  .locations li {
    padding: var(--s-3) var(--s-4);
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
  }
  .city { font-family: var(--font-display); font-size: var(--t-lg); font-weight: 500; }
  .country { color: var(--ink-muted); font-style: italic; }
  .locations .muted { margin-left: auto; }
  .small { font-size: var(--t-xs); }
</style>
