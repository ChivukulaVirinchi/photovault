<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import maplibregl, { type Map as MapInstance, type Marker } from "maplibre-gl";
  import "maplibre-gl/dist/maplibre-gl.css";
  import { map as mapApi } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { installTileCache } from "../lib/tile-cache";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { MapPin } from "../lib/api/all";

  installTileCache();

  let containerEl: HTMLDivElement | undefined = $state();
  let map: MapInstance | null = null;
  let markers: Marker[] = [];
  let pinCount = $state(0);
  let totalGeotagged = $state<number | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;
  let firstLoad = true;

  function buildMarkerElement(pin: MapPin): HTMLElement {
    if (pin.count > 1) {
      const el = document.createElement("button");
      el.type = "button";
      el.className = "pv-pin pv-pin-cluster";
      el.setAttribute("aria-label", `${pin.count} photos here`);
      const inner = document.createElement("span");
      inner.className = "pv-pin-count";
      inner.textContent =
        pin.count >= 1000
          ? (pin.count / 1000).toFixed(pin.count >= 10000 ? 0 : 1) + "k"
          : String(pin.count);
      el.appendChild(inner);
      el.onclick = (e) => {
        e.stopPropagation();
        if (!map) return;
        map.flyTo({
          center: [pin.lng, pin.lat],
          zoom: Math.min(map.getZoom() + 2, 18),
          speed: 1.5,
          curve: 1.4,
        });
      };
      return el;
    }
    const el = document.createElement("a");
    el.className = "pv-pin pv-pin-single";
    el.href = `#/photo?id=${pin.photo_id}`;
    el.setAttribute("aria-label", `Photo #${pin.photo_id}`);
    if (pin.thumbnail_path) {
      const url = thumbUrl(libraryStore.driveRoot, pin.thumbnail_path);
      if (url) {
        el.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
      }
    }
    return el;
  }

  async function refreshPins() {
    if (!map) return;
    const b = map.getBounds();
    loading = true;
    try {
      const pins = await mapApi.pins(
        {
          north: b.getNorth(),
          south: b.getSouth(),
          east: b.getEast(),
          west: b.getWest(),
        },
        Math.round(map.getZoom()),
        800,
      );
      for (const m of markers) m.remove();
      markers = pins.map((pin) => {
        const el = buildMarkerElement(pin);
        return new maplibregl.Marker({ element: el, anchor: "center" })
          .setLngLat([pin.lng, pin.lat])
          .addTo(map!);
      });
      pinCount = pins.reduce((acc, p) => acc + p.count, 0);
      if (firstLoad) {
        firstLoad = false;
        if (totalGeotagged === null) totalGeotagged = pinCount;
      }
      error = null;
    } catch (e) {
      error = JSON.stringify(e);
    } finally {
      loading = false;
    }
  }

  function debouncedRefresh() {
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(refreshPins, 220);
  }

  function fitToContent() {
    if (!map) return;
    map.flyTo({ center: [0, 20], zoom: 2, speed: 1.4 });
  }

  onMount(() => {
    if (!containerEl) return;
    map = new maplibregl.Map({
      container: containerEl,
      style: {
        version: 8,
        sources: {
          osm: {
            type: "raster",
            tiles: [
              "cached://https://a.tile.openstreetmap.org/{z}/{x}/{y}.png",
              "cached://https://b.tile.openstreetmap.org/{z}/{x}/{y}.png",
              "cached://https://c.tile.openstreetmap.org/{z}/{x}/{y}.png",
            ],
            tileSize: 256,
            attribution:
              '© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
            maxzoom: 19,
          },
        },
        layers: [
          { id: "osm", type: "raster", source: "osm" },
        ],
      },
      center: [0, 20],
      zoom: 2,
      attributionControl: { compact: true },
    });

    map.on("load", refreshPins);
    map.on("moveend", debouncedRefresh);
    map.on("zoomend", debouncedRefresh);
  });

  onDestroy(() => {
    if (refreshTimer) clearTimeout(refreshTimer);
    for (const m of markers) m.remove();
    map?.remove();
    map = null;
  });
</script>

<PageHeader title="Map">
  <span class="visible-count mono">
    {pinCount.toLocaleString()}<span class="muted"> visible</span>
  </span>
  {#if loading}<span class="loading mono">⋯</span>{/if}
  <button class="ghost" onclick={fitToContent}>Reset</button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="map-wrap">
  <div class="canvas" bind:this={containerEl}></div>
  <div class="vignette" aria-hidden="true"></div>
</div>

<style>
  .visible-count, .loading {
    font-size: var(--t-sm);
    color: var(--ink);
  }
  .loading {
    color: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }

  .map-wrap {
    flex: 1;
    position: relative;
    overflow: hidden;
    background: var(--bg-paper);
  }
  .canvas {
    position: absolute;
    inset: 0;
    /* A whisper of warmth on the tiles so the map echoes the gallery
       palette instead of the standard cold web blue. */
    filter: sepia(0.16) saturate(0.9) contrast(0.97);
  }
  .vignette {
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 80px color-mix(in oklab, var(--bg) 70%, transparent);
    z-index: 1;
  }

  /* Pins use :global() because MapLibre owns the marker DOM. */
  :global(.pv-pin) {
    display: block;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    cursor: pointer;
    transform-origin: center;
    transition: transform 140ms cubic-bezier(0.32, 0.72, 0.24, 1);
  }
  :global(.pv-pin:hover) { transform: scale(1.12); z-index: 2; }
  :global(.pv-pin:focus-visible) {
    outline: none;
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  :global(.pv-pin-single) {
    background-color: var(--bg-card);
    background-size: cover;
    background-position: center;
    border: 2px solid var(--bg-paper);
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.4),
      0 6px 14px rgba(0, 0, 0, 0.28),
      0 0 0 1px var(--accent) inset;
  }

  :global(.pv-pin-cluster) {
    background: var(--accent);
    color: #fff;
    border: 2px solid var(--bg-paper);
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 13px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.35),
      0 8px 18px var(--accent-soft);
  }
  :global(.pv-pin-cluster:hover) {
    filter: brightness(1.08);
  }
  :global(.pv-pin-count) {
    line-height: 1;
    letter-spacing: -0.02em;
  }

  /* MapLibre control overrides — match Quiet Gallery palette. */
  :global(.maplibregl-ctrl-group) {
    background: var(--bg-paper) !important;
    border: 1px solid var(--line) !important;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18) !important;
    border-radius: var(--r-md) !important;
    overflow: hidden;
  }
  :global(.maplibregl-ctrl-group button) {
    background: transparent !important;
    color: var(--ink-soft) !important;
    border-color: var(--line) !important;
  }
  :global(.maplibregl-ctrl-group button:hover) {
    background: var(--bg-card) !important;
  }
  :global(.maplibregl-ctrl-group button + button) {
    border-top: 1px solid var(--line) !important;
  }
  :global(.maplibregl-ctrl-attrib) {
    background: color-mix(in oklab, var(--bg-paper) 88%, transparent) !important;
    backdrop-filter: blur(6px);
    border-radius: var(--r-sm) 0 0 0 !important;
    font-family: var(--font-mono) !important;
    font-size: 10px !important;
  }
  :global(.maplibregl-ctrl-attrib a) { color: var(--accent) !important; }
  :global(.maplibregl-ctrl-attrib-button) {
    background-color: transparent !important;
  }
</style>
