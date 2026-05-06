<script lang="ts">
  // M3 placeholder: shows geotagged photos as a grid. Real MapLibre
  // integration with clustered pins lands in a follow-up — wiring up
  // the maplibre-gl npm dep + tile cache + bounding-box query.
  import { onMount } from "svelte";
  import { map } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { MapPin } from "../lib/api/all";

  let pins = $state<MapPin[]>([]);
  let error = $state<string | null>(null);

  async function load() {
    try {
      pins = await map.pins(
        { north: 90, south: -90, east: 180, west: -180 },
        2,
        500,
      );
    } catch (e) { error = JSON.stringify(e); }
  }

  onMount(load);
</script>

<PageHeader
  num="06"
  label="MAP"
  title="Where the world's been."
  subtitle="Interactive map coming soon. For now — the places your camera has seen, clustered."
/>

{#if error}<p class="error">{error}</p>{/if}

<div class="page">
  {#if pins.length === 0}
    <div class="empty">
      <span class="eyebrow"><span class="ornament"></span>NO LOCATIONS</span>
      <p class="quiet">No geotagged photos found. We need GPS data in EXIF.</p>
    </div>
  {:else}
    <div class="grid stagger">
      {#each pins as pin, i}
        <a class="cell" href="#/photo?id={pin.photo_id}" style="--i: {Math.min(i, 30)}">
          {#if pin.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, pin.thumbnail_path) ?? ""} alt="" loading="lazy" />
          {/if}
          {#if pin.count > 1}
            <span class="count mono">{pin.count}</span>
          {/if}
          <span class="coords mono">
            {pin.lat.toFixed(2)}°, {pin.lng.toFixed(2)}°
          </span>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-6) var(--s-7); flex: 1; overflow-y: auto; }
  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
    display: flex; flex-direction: column; gap: var(--s-3); align-items: center;
  }
  .quiet { font-family: var(--font-display); font-style: italic; font-size: var(--t-lg); color: var(--ink-soft); }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    position: relative;
    transition: transform var(--t-fast) var(--ease);
  }
  .cell:hover { transform: scale(1.018); }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
  .count {
    position: absolute;
    top: var(--s-2); right: var(--s-2);
    background: rgba(0,0,0,0.7);
    backdrop-filter: blur(8px);
    color: var(--ink);
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 500;
    border: 1px solid rgba(255,255,255,0.08);
  }
  .coords {
    position: absolute;
    bottom: var(--s-2); left: var(--s-2);
    font-size: 9px;
    color: rgba(255,255,255,0.7);
    background: rgba(0,0,0,0.5);
    padding: 2px 6px;
    border-radius: 3px;
    backdrop-filter: blur(4px);
  }
</style>
