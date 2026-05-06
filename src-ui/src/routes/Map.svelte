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

  // One-time install. Idempotent.
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
      // Render scientific-ish: 1.2k for 1234
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
    // Single pin: thumbnail medallion that links to photo detail.
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
      // Drop existing markers, build new ones.
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
        // Compute first total from a world-bounds fetch as a baseline.
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
          {
            id: "osm",
            type: "raster",
            source: "osm",
          },
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

<PageHeader
  num="06"
  label="MAP"
  title="Where the world's been."
  subtitle="Pan and zoom — the pins know how to find each other."
>
  <span class="visible-count mono">
    {pinCount.toLocaleString()} <span class="muted">visible</span>
  </span>
  {#if loading}<span class="loading mono">⋯</span>{/if}
  <button class="ghost" onclick={fitToContent}>Reset view</button>
</PageHeader>

{#if error}<p class="error map-error">{error}</p>{/if}

<div class="map-wrap">
  <div class="canvas" bind:this={containerEl}></div>
  <div class="vignette" aria-hidden="true"></div>
</div>

<style>
  .visible-count, .loading {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .loading { color: var(--accent); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }

  .map-error {
    margin: var(--s-3) var(--s-7);
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
    /* Signature warm filter — the map echoes the editorial palette
       instead of looking like every other slick blue web map. */
    filter: sepia(0.22) hue-rotate(-8deg) saturate(0.85) contrast(0.95)
      brightness(0.9);
  }
  /* A subtle vignette that frames the canvas like a magazine spread. */
  .vignette {
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 80px rgba(22, 18, 16, 0.55);
    z-index: 1;
  }

  /* Pins use :global() because MapLibre owns the marker DOM. */
  :global(.pv-pin) {
    display: block;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    cursor: pointer;
    transform-origin: center;
    transition: transform 140ms cubic-bezier(0.32, 0.72, 0.24, 1);
  }
  :global(.pv-pin:hover) { transform: scale(1.15); z-index: 2; }
  :global(.pv-pin:focus-visible) {
    outline: none;
    box-shadow: 0 0 0 3px rgba(217, 122, 63, 0.45);
  }

  :global(.pv-pin-single) {
    background-color: var(--bg-card);
    background-size: cover;
    background-position: center;
    border: 2px solid var(--bg-paper);
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.4),
      0 6px 16px rgba(0, 0, 0, 0.3),
      0 0 0 1px var(--accent) inset;
  }

  :global(.pv-pin-cluster) {
    background: var(--accent);
    color: var(--bg);
    border: 2px solid var(--bg-paper);
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 13px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.4),
      0 8px 20px rgba(217, 122, 63, 0.35);
  }
  :global(.pv-pin-cluster:hover) {
    background: #e89a64; /* var(--accent-warm) */
  }
  :global(.pv-pin-count) {
    line-height: 1;
    letter-spacing: -0.02em;
  }

  /* MapLibre control overrides — match editorial dark palette */
  :global(.maplibregl-ctrl-group) {
    background: var(--bg-paper) !important;
    border: 1px solid var(--line) !important;
    box-shadow: var(--shadow-soft) !important;
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
    background: rgba(28, 23, 20, 0.85) !important;
    backdrop-filter: blur(6px);
    border-radius: var(--r-md) 0 0 0 !important;
    font-family: var(--font-mono) !important;
    font-size: 9.5px !important;
  }
  :global(.maplibregl-ctrl-attrib a) { color: var(--accent) !important; }
  :global(.maplibregl-ctrl-attrib-button) {
    background-color: transparent !important;
  }
</style>
