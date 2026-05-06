<script lang="ts">
  // M3 placeholder: lists geotagged photos in a stub UI. Real MapLibre
  // integration (interactive map + clustered pins) lands in a follow-up
  // since it needs the maplibre-gl npm dep + tile cache wiring.
  import { onMount } from "svelte";
  import { map } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
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

<main class="map">
  <header>
    <h2>Map</h2>
    <span class="muted small">Interactive map view coming soon. {pins.length} clusters.</span>
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="grid">
    {#each pins as pin}
      <a class="cell" href="#/photo?id={pin.photo_id}">
        {#if pin.thumbnail_path}
          <img src={thumbUrl(libraryStore.driveRoot, pin.thumbnail_path) ?? ""} alt="" loading="lazy" />
        {/if}
        {#if pin.count > 1}<span class="cluster-count">{pin.count}</span>{/if}
      </a>
    {/each}
  </div>
</main>

<style>
  .map { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 16px; }
  h2 { margin: 0; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 6px; }
  .cell { aspect-ratio: 1; background: #131316; border-radius: 4px; overflow: hidden; position: relative; }
  .cell img { width: 100%; height: 100%; object-fit: cover; }
  .cluster-count {
    position: absolute; top: 8px; right: 8px;
    background: rgba(0,0,0,0.7); color: #fff;
    padding: 2px 8px; border-radius: 12px; font-size: 12px; font-weight: bold;
  }
  .small { font-size: 12px; }
</style>
