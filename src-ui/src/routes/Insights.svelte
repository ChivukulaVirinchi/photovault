<script lang="ts">
  import { insights } from "../lib/api/all";
  import { photos } from "../lib/api/photos";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { InsightsData } from "../lib/api/all";

  let data = $state<InsightsData | null>(null);
  let year = $state<number | null>(null);
  let error = $state<string | null>(null);

  /// Show the first N entries of long lists; "Show all" reveals the rest.
  const PEEK = 10;
  let showAllPeople    = $state(false);
  let showAllCountries = $state(false);
  let showAllCities    = $state(false);

  /// Tooltip state for the monthly bars.
  let tip = $state<{ x: number; y: number; label: string } | null>(null);

  async function load() {
    try { data = await insights.compute(year); }
    catch (e) { error = JSON.stringify(e); }
  }

  $effect(() => { void year; load(); });

  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

  function onBarHover(e: MouseEvent, monthIdx: number, count: number) {
    const t = e.currentTarget as HTMLElement;
    const rect = t.getBoundingClientRect();
    tip = {
      x: rect.left + rect.width / 2,
      y: rect.top - 4,
      label: `${months[monthIdx]} — ${count.toLocaleString()} photos`,
    };
  }
  function clearTip() { tip = null; }

  function placeHref(city?: string, country?: string): string {
    // Search by the most specific term only — city when present, else
    // country. Concatenating "Nagpur India" matches the FTS-like
    // free-text path against both, but most users just want photos
    // from a single named place.
    const q = (city ?? country ?? "").trim();
    return `#/search?q=${encodeURIComponent(q)}`;
  }

  // ---------- heatmap ----------
  /// Build a 53-week × 7-day grid for the heatmap. Each cell is a day
  /// of the year (or the closest 53-week window covering this year).
  /// Color intensity is the photo count for that day.
  function heatmapGrid(d: InsightsData) {
    const yr = d.heatmap_year;
    const start = new Date(yr, 0, 1);
    // Align grid to Sunday: walk back to the previous Sunday.
    start.setDate(start.getDate() - start.getDay());
    const cells: Array<{ date: string; count: number; inYear: boolean }> = [];
    const cur = new Date(start);
    for (let i = 0; i < 53 * 7; i++) {
      const m = pad(cur.getMonth() + 1);
      const day = pad(cur.getDate());
      const date = `${cur.getFullYear()}-${m}-${day}`;
      cells.push({
        date,
        count: d.heatmap[date] ?? 0,
        inYear: cur.getFullYear() === yr,
      });
      cur.setDate(cur.getDate() + 1);
    }
    return cells;
  }
  function pad(n: number) { return n < 10 ? `0${n}` : `${n}`; }

  function intensityClass(count: number, max: number): string {
    if (count === 0) return "h0";
    const pct = max > 0 ? count / max : 0;
    if (pct < 0.15) return "h1";
    if (pct < 0.35) return "h2";
    if (pct < 0.6)  return "h3";
    return "h4";
  }

  function fmtDateLong(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString("en", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  }

  let hoverDay = $state<{ x: number; y: number; date: string; count: number } | null>(null);
  function onCellHover(e: MouseEvent, date: string, count: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hoverDay = { x: r.left + r.width / 2, y: r.top - 6, date, count };
  }
  function clearDayHover() { hoverDay = null; }

  /// Open the photo viewer at the first photo of `date`, with prev/next
  /// scoped to that day. Backend `photos_list_by_date` takes a half-open
  /// [start, end) range — we pass the day and the next day to get every
  /// photo whose `date_taken` falls inside the calendar day.
  let openingDay = $state(false);
  async function openDay(date: string, count: number) {
    if (count === 0 || openingDay) return;
    openingDay = true;
    try {
      const start = `${date}T00:00:00Z`;
      const next = new Date(`${date}T00:00:00Z`);
      next.setUTCDate(next.getUTCDate() + 1);
      const y = next.getUTCFullYear();
      const m = pad(next.getUTCMonth() + 1);
      const d = pad(next.getUTCDate());
      const end = `${y}-${m}-${d}T00:00:00Z`;
      const page = await photos.listByDate(start, end, null, 500);
      if (page.items.length === 0) {
        toasts.success("No photos found for this day.");
        return;
      }
      const ids = page.items.map((p) => p.id);
      browseContext.set(`day:${date}`, ids);
      window.location.hash = `/photo?id=${ids[0]}`;
    } catch (e) {
      toasts.error(`Couldn't load day: ${typeof e === "string" ? e : JSON.stringify(e)}`);
    } finally {
      openingDay = false;
    }
  }
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
    {@const d = data}
    <section class="stats">
      {#each [
        { n: d.total_photos, label: "photos" },
        { n: d.people_count, label: "people" },
        { n: d.album_count,  label: "albums" },
        { n: d.country_count, label: "countries" },
        { n: d.city_count,   label: "cities" },
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
        {#each d.monthly_counts as count, i}
          {@const max = Math.max(1, ...d.monthly_counts)}
          <div
            class="bar-col"
            onmouseenter={(e) => onBarHover(e, i, count)}
            onmousemove={(e) => onBarHover(e, i, count)}
            onmouseleave={clearTip}
            role="presentation"
          >
            <div class="bar" style="height: {(count / max) * 100}%"></div>
            <span class="month mono">{months[i].charAt(0)}</span>
          </div>
        {/each}
      </div>

      {#if Object.keys(d.heatmap).length > 0}
        {@const cells = heatmapGrid(d)}
        {@const hMax = Math.max(1, ...Object.values(d.heatmap))}
        <h3 class="section-title sub">Day by day · {d.heatmap_year}</h3>
        <div class="heatmap" role="presentation">
          <div class="heat-grid">
            {#each cells as c (c.date)}
              <button
                type="button"
                class="heat-cell {intensityClass(c.count, hMax)}"
                class:out={!c.inYear}
                class:clickable={c.count > 0}
                disabled={c.count === 0}
                onmouseenter={(e) => onCellHover(e, c.date, c.count)}
                onmouseleave={clearDayHover}
                onclick={() => openDay(c.date, c.count)}
                aria-label={c.count > 0
                  ? `${c.count} photo${c.count === 1 ? "" : "s"} on ${c.date} — click to open`
                  : `${c.date} — no photos`}
              ></button>
            {/each}
          </div>
          <div class="heat-legend mono">
            <span>less</span>
            <span class="heat-cell h0"></span>
            <span class="heat-cell h1"></span>
            <span class="heat-cell h2"></span>
            <span class="heat-cell h3"></span>
            <span class="heat-cell h4"></span>
            <span>more</span>
          </div>
        </div>
      {/if}
    </section>

    {#if d.top_people.length > 0}
      <section>
        <h3 class="section-title">Faces you see often</h3>
        <ul class="row">
          {#each (showAllPeople ? d.top_people : d.top_people.slice(0, PEEK)) as p (p.cluster_id)}
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
        {#if d.top_people.length > PEEK}
          <button class="more" onclick={() => (showAllPeople = !showAllPeople)}>
            {showAllPeople ? "Show fewer" : `Show all ${d.top_people.length}`}
          </button>
        {/if}
      </section>
    {/if}

    {#if d.top_countries.length > 0}
      <section>
        <h3 class="section-title">Countries you've been to</h3>
        <ul class="places">
          {#each (showAllCountries ? d.top_countries : d.top_countries.slice(0, PEEK)) as c (c.country)}
            <li>
              <a href={placeHref(undefined, c.country)} class="place-row">
                <span class="city">{c.country}</span>
                <span class="muted small mono">{c.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
        {#if d.top_countries.length > PEEK}
          <button class="more" onclick={() => (showAllCountries = !showAllCountries)}>
            {showAllCountries ? "Show fewer" : `Show all ${d.top_countries.length}`}
          </button>
        {/if}
      </section>
    {/if}

    {#if d.top_locations.length > 0}
      <section>
        <h3 class="section-title">Cities you've been to</h3>
        <ul class="places">
          {#each (showAllCities ? d.top_locations : d.top_locations.slice(0, PEEK)) as l (l.city + l.country)}
            <li>
              <a href={placeHref(l.city, l.country)} class="place-row">
                <span class="city">{l.city}</span>
                <span class="country">, {l.country}</span>
                <span class="muted small mono">{l.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
        {#if d.top_locations.length > PEEK}
          <button class="more" onclick={() => (showAllCities = !showAllCities)}>
            {showAllCities ? "Show fewer" : `Show all ${d.top_locations.length}`}
          </button>
        {/if}
      </section>
    {/if}

    {#if d.top_cameras.length > 0}
      <section>
        <h3 class="section-title">Cameras</h3>
        <ul class="places">
          {#each d.top_cameras as cam (cam.camera)}
            <li>
              <a href="#/search?q={encodeURIComponent(cam.camera)}" class="place-row">
                <span class="city">{cam.camera}</span>
                <span class="muted small mono">{cam.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

{#if tip}
  <div class="floating-tip mono" style="left: {tip.x}px; top: {tip.y}px">
    {tip.label}
  </div>
{/if}
{#if hoverDay}
  <div class="floating-tip" style="left: {hoverDay.x}px; top: {hoverDay.y}px">
    <strong>{hoverDay.count}</strong>
    <span> {hoverDay.count === 1 ? "photo" : "photos"}</span>
    <span class="dim"> · {fmtDateLong(hoverDay.date)}</span>
  </div>
{/if}

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
  .section-title.sub { margin-top: var(--s-5); }

  /* ---- monthly bars ---- */
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
    cursor: pointer;
  }
  .bar {
    width: 70%;
    background: var(--accent);
    border-radius: 2px 2px 0 0;
    min-height: 2px;
    opacity: 0.7;
    transition: opacity var(--t-fast) var(--ease);
  }
  .bar-col:hover .bar { opacity: 1; }
  .month {
    font-size: 10px;
    color: var(--ink-faint);
    letter-spacing: 0.05em;
  }

  /* ---- heatmap ---- */
  .heatmap {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: var(--s-3);
  }
  .heat-grid {
    display: grid;
    grid-template-rows: repeat(7, 11px);
    grid-auto-flow: column;
    grid-auto-columns: 11px;
    gap: 3px;
    overflow-x: auto;
    padding-bottom: 4px;
  }
  .heat-cell {
    width: 11px;
    height: 11px;
    border-radius: 2px;
    cursor: default;
    background: var(--bg-card);
    border: 0;
    padding: 0;
    transition: filter var(--t-fast) var(--ease),
                outline var(--t-fast) var(--ease),
                transform var(--t-fast) var(--ease);
  }
  .heat-cell.clickable { cursor: pointer; }
  .heat-cell.clickable:hover {
    filter: brightness(1.3);
    outline: 2px solid var(--accent);
    transform: scale(1.4);
    z-index: 2;
    position: relative;
  }
  .heat-cell:disabled { cursor: default; }
  .heat-cell:hover:not(.clickable):not(:disabled) {
    filter: brightness(1.3);
    outline: 1px solid var(--ink-faint);
  }
  .heat-cell.out { opacity: 0.25; }
  .heat-cell.h0 { background: color-mix(in oklab, var(--bg) 90%, var(--accent)); }
  .heat-cell.h1 { background: color-mix(in oklab, var(--bg) 70%, var(--accent)); }
  .heat-cell.h2 { background: color-mix(in oklab, var(--bg) 50%, var(--accent)); }
  .heat-cell.h3 { background: color-mix(in oklab, var(--bg) 25%, var(--accent)); }
  .heat-cell.h4 { background: var(--accent); }
  .heat-legend {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: var(--s-3);
    font-size: 10px;
    color: var(--ink-muted);
    justify-content: flex-end;
  }
  .heat-legend .heat-cell { cursor: default; }

  /* ---- people row ---- */
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

  /* ---- places ---- */
  .places {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .places li {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
  }
  .place-row {
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
    padding: var(--s-3) var(--s-4);
    color: inherit;
    text-decoration: none;
    transition: background var(--t-fast) var(--ease);
  }
  .place-row:hover { background: var(--bg-card); }
  .city {
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
  }
  .country { color: var(--ink-muted); }
  .place-row .muted { margin-left: auto; }

  .more {
    margin-top: var(--s-2);
    background: transparent;
    border: 1px solid transparent;
    color: var(--accent);
    font-size: var(--t-sm);
    padding: 4px 0;
    cursor: pointer;
  }
  .more:hover { text-decoration: underline; }

  .floating-tip {
    position: fixed;
    transform: translate(-50%, -100%);
    padding: 5px 11px;
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    font-size: var(--t-xs);
    color: var(--ink);
    white-space: nowrap;
    pointer-events: none;
    z-index: 50;
    box-shadow: 0 6px 16px rgba(0,0,0,0.35);
  }
  .floating-tip strong { color: var(--ink); font-weight: 600; }
  .floating-tip .dim { color: var(--ink-muted); }
  .small { font-size: var(--t-xs); }
</style>
