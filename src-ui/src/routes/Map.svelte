<script module lang="ts">
  import type { PhotoSummaryDto as CachedPhotoSummaryDto } from "../lib/api/types";

  type CachedClusterRef = {
    lat: number;
    lng: number;
    count: number;
    photo_ids: number[];
  };

  let cachedMapRoute:
    | {
        driveRoot: string | null;
        drawerOpen: boolean;
        drawerRef: CachedClusterRef | null;
        drawerPhotos: CachedPhotoSummaryDto[];
      }
    | null = null;
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import maplibregl, { type Map as MapInstance, type Marker } from "maplibre-gl";
  import "maplibre-gl/dist/maplibre-gl.css";
  import Supercluster from "supercluster";
  import { commandErrorMessage } from "../lib/api";
  import { map as mapApi } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { enqueueThumbnail, thumbnailOnVisible } from "../lib/thumbnailRequest";
  import { installTileCache } from "../lib/tile-cache";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import { X, ZoomIn } from "lucide-svelte";
  import type { MapPin } from "../lib/api/all";
  import type { PhotoSummaryDto } from "../lib/api/types";

  installTileCache();

  const currentDriveRoot = libraryStore.driveRoot;
  const currentMapCache = cachedMapRoute?.driveRoot === currentDriveRoot ? cachedMapRoute : null;

  /// Each geotagged photo, stored once and clustered client-side. The
  /// previous design did the clustering on the backend and re-fetched
  /// on every zoom change — markers stayed at their old positions
  /// while the new query was in flight, then snapped 1–2 s later.
  /// Supercluster runs on the data we already have, so each zoom step
  /// recomputes positions before MapLibre's next frame.
  type PinFeature = {
    type: "Feature";
    geometry: { type: "Point"; coordinates: [number, number] };
    properties: { photo_id: number; thumbnail_path: string | null };
  };

  let containerEl: HTMLDivElement | undefined = $state();
  let map: MapInstance | null = null;
  let markers: Marker[] = [];
  let markerThumbnailCancels: Array<() => void> = [];
  let pinCount = $state(0);
  let totalGeotagged = $state<number | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let renderTimer: ReturnType<typeof setTimeout> | null = null;
  let cluster: Supercluster<PinFeature["properties"]> | null = null;
  let viewportPins: MapPin[] = [];
  let viewportSeq = 0;
  let usingViewportPins = false;
  let disposed = false;
  let drawerSeq = 0;
  const ALL_PINS_CAP = 100_000;

  // Filmstrip drawer state — for clusters, photo_ids are the leaves of
  // the supercluster tree; for single pins, just the one id.
  type ClusterRef = {
    lat: number;
    lng: number;
    count: number;
    photo_ids: number[];
  };
  let drawerOpen = $state(currentMapCache?.drawerOpen ?? false);
  let drawerRef = $state<ClusterRef | null>(currentMapCache?.drawerRef ?? null);
  let drawerPhotos = $state<PhotoSummaryDto[]>(currentMapCache?.drawerPhotos ?? []);
  let drawerLoading = $state(false);
  const DRAWER_LIMIT = 500;
  type MapReturnState = {
    center: [number, number];
    zoom: number;
    bearing: number;
    pitch: number;
  };

  function readReturnState(): MapReturnState | null {
    try {
      const parsed = history.state?.smritiMapReturnState as MapReturnState | undefined;
      if (!parsed) return null;
      const rest = { ...(history.state ?? {}) };
      delete rest.smritiMapReturnState;
      history.replaceState(rest, "", location.href);
      if (!Array.isArray(parsed.center) || parsed.center.length !== 2) return null;
      if (!Number.isFinite(parsed.zoom)) return null;
      return parsed;
    } catch {
      return null;
    }
  }

  function rememberReturnState() {
    if (!map) return;
    const center = map.getCenter();
    const state: MapReturnState = {
      center: [center.lng, center.lat],
      zoom: map.getZoom(),
      bearing: map.getBearing(),
      pitch: map.getPitch(),
    };
    history.replaceState({ ...(history.state ?? {}), smritiMapReturnState: state }, "", location.href);
  }

  function saveMapRouteCache() {
    cachedMapRoute = {
      driveRoot: currentDriveRoot,
      drawerOpen,
      drawerRef,
      drawerPhotos,
    };
  }

  /// Cluster appearance:
  ///   far zoom (≤6): big count-only bubble
  ///   mid zoom (7+): representative thumbnail + +N badge
  function clusterShowsThumb(zoom: number): boolean {
    return zoom >= 6.5;
  }

  function clearMarkers() {
    for (const cancel of markerThumbnailCancels) cancel();
    markerThumbnailCancels = [];
    for (const m of markers) m.remove();
    markers = [];
  }

  function buildClusterElement(
    count: number,
    photoId: number,
    repThumb: string | null,
    zoom: number,
    onClick: () => void,
  ): HTMLElement {
    const anchor = document.createElement("div");
    anchor.className = "pv-pin-anchor";
    const wrap = document.createElement("button");
    wrap.type = "button";
    wrap.className = "pv-pin pv-pin-cluster";
    wrap.setAttribute("aria-label", `${count} photos here`);

    if (clusterShowsThumb(zoom) && repThumb) {
      wrap.classList.add("with-thumb");
      const url = thumbUrl(libraryStore.driveRoot, repThumb);
      if (url) wrap.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
    } else if (clusterShowsThumb(zoom) && photoId > 0) {
      wrap.classList.add("with-thumb");
      markerThumbnailCancels.push(
        enqueueThumbnail(photoId, (path) => {
          if (!wrap.isConnected) return;
          const url = thumbUrl(libraryStore.driveRoot, path);
          if (url) wrap.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
        }, Date.now() + 1_000_000_000),
      );
    }

    if (clusterShowsThumb(zoom)) {
      const badge = document.createElement("span");
      badge.className = "pv-pin-badge";
      badge.textContent =
        count >= 1000
          ? "+" + (count / 1000).toFixed(count >= 10000 ? 0 : 1) + "k"
          : "+" + count;
      wrap.appendChild(badge);
    } else {
      const inner = document.createElement("span");
      inner.className = "pv-pin-count";
      inner.textContent =
        count >= 1000
          ? (count / 1000).toFixed(count >= 10000 ? 0 : 1) + "k"
          : String(count);
      wrap.appendChild(inner);
    }

    wrap.onclick = (e) => {
      e.stopPropagation();
      onClick();
    };
    anchor.appendChild(wrap);
    return anchor;
  }

  function buildSingleElement(photoId: number, thumb: string | null): HTMLElement {
    const anchor = document.createElement("div");
    anchor.className = "pv-pin-anchor";
    const el = document.createElement("a");
    el.className = "pv-pin pv-pin-single";
    el.href = `#/photo?id=${photoId}`;
    el.setAttribute("aria-label", `Photo #${photoId}`);
    el.addEventListener("click", () => {
      rememberReturnState();
      browseContext.set(`map:${photoId}`, [photoId]);
    });
    if (thumb) {
      const url = thumbUrl(libraryStore.driveRoot, thumb);
      if (url) el.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
    } else {
      markerThumbnailCancels.push(
        enqueueThumbnail(photoId, (path) => {
          if (!el.isConnected) return;
          const url = thumbUrl(libraryStore.driveRoot, path);
          if (url) el.style.backgroundImage = `url("${url.replace(/"/g, '\\"')}")`;
        }, Date.now() + 1_000_000_000),
      );
    }
    anchor.appendChild(el);
    return anchor;
  }

  /// Render the visible clusters/points. Synchronous — no IPC roundtrip.
  function renderMarkers() {
    if (!map) return;
    if (usingViewportPins) {
      renderViewportMarkers();
      return;
    }
    if (!cluster) return;
    const b = map.getBounds();
    const z = Math.round(map.getZoom());
    const bbox: [number, number, number, number] = [
      b.getWest(),
      b.getSouth(),
      b.getEast(),
      b.getNorth(),
    ];
    const features = cluster.getClusters(bbox, z);

    clearMarkers();

    let visible = 0;
    for (const f of features) {
      const [lng, lat] = f.geometry.coordinates as [number, number];
      const props = f.properties as Record<string, unknown>;
      const isCluster = props.cluster === true;
      let el: HTMLElement;
      if (isCluster) {
        const count = props.point_count as number;
        const clusterId = props.cluster_id as number;
        // Pick a representative thumbnail from the cluster's leaves.
        // getLeaves with limit=1 is enough for the marker face.
        const leaves = cluster!.getLeaves(clusterId, 1, 0) as PinFeature[];
        const rep = leaves[0];
        el = buildClusterElement(
          count,
          rep?.properties.photo_id ?? 0,
          rep?.properties.thumbnail_path ?? null,
          map.getZoom(),
          () => openClusterDrawer(clusterId, count, lat, lng),
        );
        visible += count;
      } else {
        el = buildSingleElement(
          props.photo_id as number,
          (props.thumbnail_path as string | null) ?? null,
        );
        visible += 1;
      }
      markers.push(
        new maplibregl.Marker({ element: el, anchor: "center" })
          .setLngLat([lng, lat])
          .addTo(map!),
      );
    }
    pinCount = visible;
  }

  function renderViewportMarkers() {
    if (!map) return;
    clearMarkers();

    let visible = 0;
    for (const pin of viewportPins) {
      let el: HTMLElement;
      if (pin.count > 1) {
        el = buildClusterElement(pin.count, pin.photo_id, pin.thumbnail_path, map.getZoom(), () => {
          openServerClusterDrawer(pin);
        });
        visible += pin.count;
      } else {
        el = buildSingleElement(pin.photo_id, pin.thumbnail_path);
        visible += 1;
      }
      markers.push(
        new maplibregl.Marker({ element: el, anchor: "center" })
          .setLngLat([pin.lng, pin.lat])
          .addTo(map),
      );
    }
    pinCount = visible;
  }

  function scheduleRender() {
    if (renderTimer) clearTimeout(renderTimer);
    // Tiny debounce so a rapid pan-then-zoom collapses to one paint.
    renderTimer = setTimeout(() => {
      if (usingViewportPins) {
        void loadViewportPins();
      } else {
        renderMarkers();
      }
    }, usingViewportPins ? 120 : 16);
  }

  async function loadAllPins() {
    loading = true;
    try {
      const pins = await mapApi.pinsAll();
      if (disposed) return;
      if (pins.length >= ALL_PINS_CAP) {
        usingViewportPins = true;
        cluster = null;
        totalGeotagged = null;
        await loadViewportPins();
        if (disposed) return;
        error = null;
        return;
      }
      const features: PinFeature[] = pins.map((p: MapPin) => ({
        type: "Feature",
        geometry: { type: "Point", coordinates: [p.lng, p.lat] },
        properties: { photo_id: p.photo_id, thumbnail_path: p.thumbnail_path },
      }));
      const sc = new Supercluster<PinFeature["properties"]>({
        radius: 60,
        maxZoom: 16,
        minPoints: 2,
      });
      sc.load(features);
      cluster = sc;
      totalGeotagged = features.length;
      renderMarkers();
      error = null;
    } catch (e) {
      if (!disposed) error = commandErrorMessage(e);
    } finally {
      if (!disposed) loading = false;
    }
  }

  async function loadViewportPins() {
    if (!map) return;
    const seq = ++viewportSeq;
    const b = map.getBounds();
    const bounds = {
      north: b.getNorth(),
      south: b.getSouth(),
      east: b.getEast(),
      west: b.getWest(),
    };
    loading = true;
    try {
      const pins = await mapApi.pins(bounds, Math.round(map.getZoom()), 1200);
      if (disposed || seq !== viewportSeq) return;
      viewportPins = pins;
      renderViewportMarkers();
      error = null;
    } catch (e) {
      if (!disposed && seq === viewportSeq) error = commandErrorMessage(e);
    } finally {
      if (!disposed && seq === viewportSeq) loading = false;
    }
  }

  function openDrawerPhoto(photoId: number) {
    saveMapRouteCache();
    rememberReturnState();
    if (drawerRef) {
      browseContext.set(`map:${drawerRef.lat.toFixed(5)},${drawerRef.lng.toFixed(5)}`, drawerRef.photo_ids);
    }
    window.location.hash = `/photo?id=${photoId}`;
  }

  function fitToContent() {
    if (!map) return;
    map.flyTo({ center: [0, 20], zoom: 2, speed: 1.4 });
  }

  async function openClusterDrawer(clusterId: number, count: number, lat: number, lng: number) {
    if (!cluster) return;
    const seq = ++drawerSeq;
    const contextLeaves = cluster.getLeaves(clusterId, Math.min(count, DRAWER_LIMIT), 0) as PinFeature[];
    const contextIds = contextLeaves.map((l) => l.properties.photo_id);
    const filmstripIds = contextIds.slice(0, DRAWER_LIMIT);
    drawerRef = { lat, lng, count, photo_ids: contextIds };
    drawerOpen = true;
    drawerLoading = true;
    drawerPhotos = [];
    saveMapRouteCache();
    try {
      const photos = await mapApi.clusterFilmstrip(filmstripIds);
      if (seq === drawerSeq && !disposed) {
        drawerPhotos = photos;
        saveMapRouteCache();
      }
    } catch (e) {
      if (seq === drawerSeq && !disposed) error = commandErrorMessage(e);
    } finally {
      if (seq === drawerSeq && !disposed) {
        drawerLoading = false;
        saveMapRouteCache();
      }
    }
  }

  async function openServerClusterDrawer(pin: MapPin) {
    const contextIds = pin.photo_ids.length > 0 ? pin.photo_ids : [pin.photo_id];
    await openDrawerForIds(contextIds.slice(0, DRAWER_LIMIT), pin.count, pin.lat, pin.lng, contextIds);
  }

  async function openDrawerForIds(
    photoIds: number[],
    count: number,
    lat: number,
    lng: number,
    contextIds = photoIds,
  ) {
    const seq = ++drawerSeq;
    drawerRef = { lat, lng, count, photo_ids: contextIds };
    drawerOpen = true;
    drawerLoading = true;
    drawerPhotos = [];
    saveMapRouteCache();
    try {
      const photos = await mapApi.clusterFilmstrip(photoIds);
      if (seq === drawerSeq && !disposed) {
        drawerPhotos = photos;
        saveMapRouteCache();
      }
    } catch (e) {
      if (seq === drawerSeq && !disposed) error = commandErrorMessage(e);
    } finally {
      if (seq === drawerSeq && !disposed) {
        drawerLoading = false;
        saveMapRouteCache();
      }
    }
  }

  function closeDrawer() {
    drawerSeq++;
    drawerOpen = false;
    drawerRef = null;
    drawerPhotos = [];
    saveMapRouteCache();
  }

  function patchDrawerThumbnail(photoId: number, thumbnailPath: string) {
    drawerPhotos = drawerPhotos.map((p) => (
      p.id === photoId ? { ...p, thumbnail_path: thumbnailPath } : p
    ));
    saveMapRouteCache();
  }

  function zoomIntoCluster() {
    if (!map || !drawerRef) return;
    map.flyTo({
      center: [drawerRef.lng, drawerRef.lat],
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
    disposed = false;
    const returnState = readReturnState();
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
      center: returnState?.center ?? [0, 20],
      zoom: returnState?.zoom ?? 2,
      bearing: returnState?.bearing ?? 0,
      pitch: returnState?.pitch ?? 0,
      attributionControl: { compact: true },
    });

    map.on("load", loadAllPins);
    map.on("move", scheduleRender);
    map.on("zoom", scheduleRender);
    map.on("click", closeDrawer);
    window.addEventListener("keydown", onKey);
  });

  onDestroy(() => {
    saveMapRouteCache();
    disposed = true;
    drawerSeq++;
    if (renderTimer) clearTimeout(renderTimer);
    window.removeEventListener("keydown", onKey);
    clearMarkers();
    map?.remove();
    map = null;
    cluster = null;
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

  {#if drawerOpen && drawerRef}
    {@const dp = drawerRef}
    <!-- The outer .drawer-shell owns position + scroll layout; the
         inner .drawer-anim runs the slide-in transform. Splitting the
         two means the animated transform never sits on the flex
         column that contains the scroll container — some Chromium
         versions stop honouring overflow:auto on a descendant once an
         ancestor carries a non-identity transform, which manifested
         as "photos squeezed, no scroll". -->
    <div class="drawer-shell">
      <aside class="drawer-anim" aria-label="Photos at this location">
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
              <button
                class="cell"
                type="button"
                onclick={() => openDrawerPhoto(p.id)}
                use:thumbnailOnVisible={{
                  id: p.id,
                  thumbnailPath: p.thumbnail_path,
                  onReady: (path) => patchDrawerThumbnail(p.id, path),
                }}
              >
                {#if p.thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
                {/if}
              </button>
            {/each}
          {/if}
        </div>
      </aside>
    </div>
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
  /* Outer shell: positioning + the flex column that the scroll
     container relies on. No transform here, ever. */
  .drawer-shell {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(420px, 50vw);
    z-index: 5;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  /* Inner element runs the slide-in animation. `forwards` keeps the
     final keyframe applied; the keyframe ends at translateX(0) which
     is the no-op state. The scroll-bearing descendants live INSIDE
     this element so they see a transformed ancestor only at this
     level — Chromium's overflow accounting is fine with that, the
     bug only triggered when the transform sat on the same element as
     the flex container plus position:absolute. */
  .drawer-anim {
    flex: 1;
    background: var(--bg-paper);
    border-left: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    min-height: 0;
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.35);
    animation: slide-in 220ms var(--ease) forwards;
  }
  @keyframes slide-in {
    from { transform: translateX(100%); }
    to   { transform: translateX(0); }
  }
  .drawer-anim header {
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    flex-shrink: 0;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: var(--s-3);
  }
  .drawer-anim h3 {
    margin: 0;
    flex: 1;
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    font-size: var(--t-base);
    font-weight: 600;
  }
  .drawer-anim .num {
    font-size: var(--t-2xl);
    font-weight: 600;
    color: var(--ink);
  }
  .drawer-anim .label { font-size: var(--t-sm); color: var(--ink-muted); }
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
  .drawer-anim .grid {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: var(--s-3) var(--s-4);
    /* Fixed-size cells. We tried `auto-fill, minmax(96px, 1fr)` + cell
       `aspect-ratio: 1`, and on Tauri's WebView2 build the grid track
       sizer didn't honour `aspect-ratio` consistently — cells collapsed
       to 0 height and visually stacked on top of each other. Hard-coded
       100×100 squares + `justify-content: center` give a predictable
       photo grid that always paints right and degrades gracefully when
       the drawer narrows. */
    display: grid;
    grid-template-columns: repeat(auto-fill, 100px);
    grid-auto-rows: 100px;
    justify-content: center;
    gap: 4px;
  }
  .drawer-anim .cell {
    width: 100px;
    height: 100px;
    background: var(--bg-card);
    border: 0;
    border-radius: var(--r-sm);
    padding: 0;
    overflow: hidden;
    cursor: pointer;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .drawer-anim .cell:hover {
    filter: brightness(1.08);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .drawer-anim .cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .loading-state {
    grid-column: 1 / -1;
    text-align: center;
    color: var(--ink-muted);
    padding: var(--s-6);
  }

  /* Pins use :global() because MapLibre owns the marker DOM. */
  :global(.pv-pin-anchor) {
    width: 52px;
    height: 52px;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: auto;
  }
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
  :global(.pv-pin-anchor:hover .pv-pin) { transform: scale(1.12); }
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
