<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import maplibregl, { type Map as MapInstance, type Marker } from "maplibre-gl";
  import "maplibre-gl/dist/maplibre-gl.css";
  import { map as mapApi } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { installTileCache } from "../lib/tile-cache";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import { X, ZoomIn } from "lucide-svelte";
  import type { MapPin } from "../lib/api/all";
  import type { PhotoSummaryDto } from "../lib/api/types";

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

  // Filmstrip drawer state
  let drawerOpen = $state(false);
  let drawerPin = $state<MapPin | null>(null);
  let drawerPhotos = $state<PhotoSummaryDto[]>([]);
  let drawerLoading = $state(false);

  /// Cluster appearance:
  ///   far zoom (≤6): big count-only bubble (counts can be huge, thumbs feel noisy)
  ///   mid zoom (7+): representative thumbnail + +N badge
  function clusterShowsThumb(zoom: number): boolean {
    return zoom >= 6.5;
  }

  function buildMarkerElement(pin: MapPin, zoom: number): HTMLElement {
    if (pin.count > 1) {
      const wrap = document.createElement("button");
      wrap.type = "button";
      wrap.className = "pv-pin pv-pin-cluster";
      wrap.setAttribute("aria-label", `${pin.count} photos here`);

      if (clusterShowsThumb(zoom) && pin.thumbnail_path) {
        wrap.classList.add("with-thumb");
        const url = thumbUrl(libraryStore.driveRoot, pin.thumbnail_path);
        if (url) wrap.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
        const badge = document.createElement("span");
        badge.className = "pv-pin-badge";
        badge.textContent =
          pin.count >= 1000
            ? "+" + (pin.count / 1000).toFixed(pin.count >= 10000 ? 0 : 1) + "k"
            : "+" + pin.count;
        wrap.appendChild(badge);
      } else {
        const inner = document.createElement("span");
        inner.className = "pv-pin-count";
        inner.textContent =
          pin.count >= 1000
            ? (pin.count / 1000).toFixed(pin.count >= 10000 ? 0 : 1) + "k"
            : String(pin.count);
        wrap.appendChild(inner);
      }

      wrap.onclick = (e) => {
        e.stopPropagation();
        openDrawer(pin);
      };
      return wrap;
    }

    const el = document.createElement("a");
    el.className = "pv-pin pv-pin-single";
    el.href = `#/photo?id=${pin.photo_id}`;
    el.setAttribute("aria-label", `Photo #${pin.photo_id}`);
    if (pin.thumbnail_path) {
      const url = thumbUrl(libraryStore.driveRoot, pin.thumbnail_path);
      if (url) el.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
    }
    return el;
  }

  async function refreshPins() {
    if (!map) return;
    const b = map.getBounds();
    const z = map.getZoom();
    loading = true;
    try {
      const pins = await mapApi.pins(
        {
          north: b.getNorth(),
          south: b.getSouth(),
          east: b.getEast(),
          west: b.getWest(),
        },
        Math.round(z),
        800,
      );
      for (const m of markers) m.remove();
      markers = pins.map((pin) => {
        const el = buildMarkerElement(pin, z);
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

  async function openDrawer(pin: MapPin) {
    drawerPin = pin;
    drawerOpen = true;
    drawerLoading = true;
    drawerPhotos = [];
    try {
      drawerPhotos = await mapApi.clusterFilmstrip(pin.photo_ids);
    } catch (e) {
      error = JSON.stringify(e);
    } finally {
      drawerLoading = false;
    }
  }

  function closeDrawer() {
    drawerOpen = false;
    drawerPin = null;
    drawerPhotos = [];
  }

  function zoomIntoCluster() {
    if (!map || !drawerPin) return;
    map.flyTo({
      center: [drawerPin.lng, drawerPin.lat],
      zoom: Math.min(map.getZoom() + 2, 18),
      speed: 1.5,
      curve: 1.4,
    });
    closeDrawer();
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (drawerOpen && e.key === "Escape") {
      e.preventDefault();
      closeDrawer();
    }
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
    window.addEventListener("keydown", onKey);
  });

  onDestroy(() => {
    if (refreshTimer) clearTimeout(refreshTimer);
    for (const m of markers) m.remove();
    map?.remove();
    map = null;
    window.removeEventListener("keydown", onKey);
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

  {#if drawerOpen && drawerPin}
    {@const dp = drawerPin}
    <aside class="drawer" aria-label="Photos at this location">
      <header>
        <div class="title-row">
          <h3>
            <span class="num mono">{dp.count}</span>
            <span class="label">{dp.count === 1 ? "photo" : "photos"} here</span>
          </h3>
          <button class="icon-btn" onclick={closeDrawer} aria-label="Close" title="Close (Esc)">
            <X size={14} strokeWidth={1.75} />
          </button>
        </div>
        <button class="ghost zoom-cluster" onclick={zoomIntoCluster}>
          <ZoomIn size={13} strokeWidth={1.75} />
          <span>Zoom in</span>
        </button>
      </header>
      <div class="grid">
        {#if drawerLoading}
          <div class="loading-state mono">loading…</div>
        {:else}
          {#each drawerPhotos as p (p.id)}
            <a class="cell" href="#/photo?id={p.id}">
              {#if p.thumbnail_path}
                <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
              {/if}
            </a>
          {/each}
        {/if}
      </div>
    </aside>
  {/if}
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
    filter: sepia(0.16) saturate(0.9) contrast(0.97);
  }
  .vignette {
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 80px color-mix(in oklab, var(--bg) 70%, transparent);
    z-index: 1;
  }

  /* ---- filmstrip drawer ---- */
  .drawer {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(420px, 50vw);
    background: var(--bg-paper);
    border-left: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    z-index: 5;
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.35);
    animation: slide-in 220ms var(--ease) both;
  }
  @keyframes slide-in {
    from { transform: translateX(100%); }
    to   { transform: translateX(0); }
  }
  .drawer header {
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: var(--s-3);
  }
  .drawer h3 {
    margin: 0;
    flex: 1;
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    font-size: var(--t-base);
    font-weight: 600;
  }
  .drawer .num {
    font-size: var(--t-2xl);
    font-weight: 600;
    color: var(--ink);
  }
  .drawer .label { font-size: var(--t-sm); color: var(--ink-muted); }
  .icon-btn {
    width: 26px;
    height: 26px;
    border: 1px solid transparent;
    background: transparent;
    border-radius: var(--r-sm);
    color: var(--ink-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .icon-btn:hover { background: var(--bg-card); color: var(--ink); }
  .zoom-cluster {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--t-sm);
    padding: 4px 10px;
  }
  .drawer .grid {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-3) var(--s-4);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    gap: 4px;
  }
  .drawer .cell {
    aspect-ratio: 1;
    background: var(--bg-card);
    border-radius: var(--r-sm);
    overflow: hidden;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .drawer .cell:hover {
    filter: brightness(1.08);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .drawer .cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .loading-state {
    grid-column: 1 / -1;
    text-align: center;
    color: var(--ink-muted);
    padding: var(--s-6);
  }

  /* Pins use :global() because MapLibre owns the marker DOM. */
  :global(.pv-pin) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    cursor: pointer;
    transform-origin: center;
    transition: transform 140ms cubic-bezier(0.32, 0.72, 0.24, 1);
    position: relative;
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
    width: 40px;
    height: 40px;
  }

  /* Cluster pins: count-only bubble (default) and thumbnail variant */
  :global(.pv-pin-cluster) {
    background: var(--accent);
    color: #fff;
    border: 2px solid var(--bg-paper);
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 13px;
    padding: 0;
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.35),
      0 8px 18px var(--accent-soft);
  }
  :global(.pv-pin-cluster.with-thumb) {
    background-color: var(--bg-card);
    background-size: cover;
    background-position: center;
    width: 50px;
    height: 50px;
  }
  :global(.pv-pin-cluster.with-thumb::after) {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: 50%;
    box-shadow: inset 0 0 0 2px var(--accent);
    pointer-events: none;
  }
  :global(.pv-pin-cluster:hover) {
    filter: brightness(1.08);
  }
  :global(.pv-pin-count) {
    line-height: 1;
    letter-spacing: -0.02em;
  }
  :global(.pv-pin-badge) {
    position: absolute;
    bottom: -6px;
    right: -8px;
    background: var(--accent);
    color: #fff;
    border: 2px solid var(--bg-paper);
    border-radius: 999px;
    padding: 1px 7px;
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    font-weight: 700;
    line-height: 1.4;
    z-index: 1;
    box-shadow: 0 2px 6px rgba(0,0,0,0.35);
  }

  /* MapLibre control overrides */
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
