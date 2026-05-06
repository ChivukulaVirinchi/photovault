<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    ChevronLeft, ChevronRight, Info, ZoomIn, ZoomOut, Maximize2,
    RotateCcw, RotateCw, FolderOpen, X,
  } from "lucide-svelte";
  import { photos, type ExifExtras } from "../lib/api/photos";
  import { library } from "../lib/api/library";
  import { system } from "../lib/api/system";
  import { call } from "../lib/api/index";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { extractDominantColor, type RGB } from "../lib/dominantColor";
  import ZoomImage from "../lib/components/ZoomImage.svelte";
  import type { ZoomApi } from "../lib/zoomApi";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import maplibregl, { type Map as MapInstance } from "maplibre-gl";
  import "maplibre-gl/dist/maplibre-gl.css";
  import type { PhotoDto, PersonDto, AlbumDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let photo = $state<PhotoDto | null>(null);
  let imageUrl = $state<string | null>(null);
  let people = $state<PersonDto[]>([]);
  let albums = $state<AlbumDto[]>([]);
  let extras = $state<ExifExtras | null>(null);
  let error = $state<string | null>(null);

  // Closed by default — info button reveals.
  let metaOpen = $state(false);
  let tint = $state<RGB | null>(null);
  let manualRotate = $state(0);
  let zoomApi = $state<ZoomApi | undefined>(undefined);

  // Mouse-activity fade for viewer chrome (toolbar, chevrons, position,
  // filename, cursor). Resets on every mousemove inside the viewer; if
  // 2.5s pass with no movement, chrome and cursor fade away.
  let viewerActive = $state(true);
  let idleTimer: ReturnType<typeof setTimeout> | null = null;
  function bumpActivity() {
    viewerActive = true;
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = setTimeout(() => { viewerActive = false; }, 2500);
  }
  function onViewerEnter() { bumpActivity(); }
  function onViewerLeave() {
    if (idleTimer) clearTimeout(idleTimer);
    viewerActive = false;
  }

  let miniEl: HTMLDivElement | undefined = $state();
  let miniMap: MapInstance | null = null;

  function back() { history.back(); }

  function gotoId(target: number | null) {
    if (target == null) return;
    window.location.hash = `/photo?id=${target}`;
  }

  const prevId = $derived(photo ? browseContext.prev(photo.id) : null);
  const nextId = $derived(photo ? browseContext.next(photo.id) : null);
  const position = $derived(photo ? browseContext.position(photo.id) : null);

  /// EXIF orientation → degrees of CSS rotation.
  /// Mirror variants (2/4/5/7) aren't auto-flipped for v1; their rotation
  /// portion is still applied so e.g. "5" reads as a quarter-turn.
  function orientationDegrees(o: number): number {
    switch (o) {
      case 3: return 180;
      case 6: return 90;
      case 8: return 270;
      case 5: return 270;
      case 7: return 90;
      default: return 0;
    }
  }
  function orientationLabel(o: number): string {
    switch (o) {
      case 1: return "Normal";
      case 2: return "Mirrored horizontally";
      case 3: return "Rotated 180°";
      case 4: return "Mirrored vertically";
      case 5: return "Mirrored & rotated 90° CCW";
      case 6: return "Rotated 90° CW";
      case 7: return "Mirrored & rotated 90° CW";
      case 8: return "Rotated 90° CCW";
      default: return "Unknown";
    }
  }

  const totalRotation = $derived.by(() => {
    if (!photo) return manualRotate;
    return (orientationDegrees(photo.orientation) + manualRotate + 360) % 360;
  });

  function fileFormat(name: string): string {
    const m = name.toLowerCase().match(/\.([a-z0-9]+)$/);
    if (!m) return "—";
    const ext = m[1];
    const mapping: Record<string, string> = {
      jpg: "JPEG", jpeg: "JPEG", png: "PNG", gif: "GIF", webp: "WebP",
      heic: "HEIC", heif: "HEIF", avif: "AVIF",
      tif: "TIFF", tiff: "TIFF", bmp: "BMP",
      raw: "RAW", nef: "NEF (Nikon RAW)", cr2: "CR2 (Canon RAW)",
      cr3: "CR3 (Canon RAW)", arw: "ARW (Sony RAW)",
      dng: "DNG", orf: "ORF (Olympus RAW)", rw2: "RW2 (Panasonic RAW)",
    };
    return mapping[ext] ?? ext.toUpperCase();
  }

  async function load() {
    error = null;
    imageUrl = null;
    people = [];
    albums = [];
    extras = null;
    tint = null;
    manualRotate = 0;
    destroyMini();
    try {
      photo = await photos.get(id);
      try {
        const { absolute_path } = await library.resolvePath(id);
        imageUrl = convertFileSrc(absolute_path);
      } catch {}
      try {
        people = await call<PersonDto[]>("photos_people_in_photo", { photo_id: id });
      } catch {}
      try {
        albums = await call<AlbumDto[]>("photos_albums_for_photo", { photo_id: id });
      } catch {}
      // Tier-2 EXIF — re-parsed off the file at request time. Non-blocking.
      photos.exifExtras(id).then((e) => { if (photo?.id === id) extras = e; }).catch(() => {});
      if (photo?.gps && metaOpen) {
        setTimeout(initMini, 0);
      }
    } catch (e) { error = JSON.stringify(e); }
  }

  function initMini() {
    if (!miniEl || !photo?.gps) return;
    if (miniMap) destroyMini();
    miniMap = new maplibregl.Map({
      container: miniEl,
      style: {
        version: 8,
        sources: {
          osm: {
            type: "raster",
            tiles: [
              "https://a.tile.openstreetmap.org/{z}/{x}/{y}.png",
              "https://b.tile.openstreetmap.org/{z}/{x}/{y}.png",
              "https://c.tile.openstreetmap.org/{z}/{x}/{y}.png",
            ],
            tileSize: 256,
            attribution: "© OSM",
            maxzoom: 19,
          },
        },
        layers: [{ id: "osm", type: "raster", source: "osm" }],
      },
      center: [photo.gps.lng, photo.gps.lat],
      zoom: 13,
      attributionControl: { compact: true },
    });
    miniMap.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");
    const pin = document.createElement("div");
    pin.className = "mini-pin";
    new maplibregl.Marker({ element: pin })
      .setLngLat([photo.gps.lng, photo.gps.lat])
      .addTo(miniMap);
  }

  function destroyMini() {
    if (miniMap) {
      miniMap.remove();
      miniMap = null;
    }
  }

  async function revealInFolder() {
    if (!photo) return;
    try { await system.openInExplorer(photo.id); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function toggleFullscreen() {
    try {
      const w = getCurrentWindow();
      const f = await w.isFullscreen();
      await w.setFullscreen(!f);
    } catch {}
  }

  function rotateCcw() { manualRotate = (manualRotate - 90 + 360) % 360; }
  function rotateCw()  { manualRotate = (manualRotate + 90) % 360; }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    switch (e.key) {
      case "Escape":     back(); break;
      case "ArrowLeft":  e.preventDefault(); gotoId(prevId); break;
      case "ArrowRight": e.preventDefault(); gotoId(nextId); break;
      case "i": case "I":
        if (!e.metaKey && !e.ctrlKey) { metaOpen = !metaOpen; e.preventDefault(); }
        break;
      case "+": case "=":
        e.preventDefault(); zoomApi?.zoomIn(); break;
      case "-": case "_":
        e.preventDefault(); zoomApi?.zoomOut(); break;
      case "0":
        e.preventDefault(); zoomApi?.fit(); break;
      case "1":
        e.preventDefault(); zoomApi?.actual(); break;
      case "[":
        e.preventDefault(); rotateCcw(); break;
      case "]":
        e.preventDefault(); rotateCw(); break;
      case "f": case "F":
        if (!e.metaKey && !e.ctrlKey) { e.preventDefault(); toggleFullscreen(); }
        break;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    load();
    return () => window.removeEventListener("keydown", onKey);
  });

  onDestroy(() => {
    destroyMini();
    if (idleTimer) clearTimeout(idleTimer);
  });

  $effect(() => { void id; load(); });

  $effect(() => {
    const pid = photo?.id;
    const tpath = photo?.thumbnail_path;
    if (!pid || !tpath) return;
    const url = thumbUrl(libraryStore.driveRoot, tpath);
    if (!url) return;
    let cancelled = false;
    extractDominantColor(url).then((rgb) => {
      if (!cancelled && photo?.id === pid) tint = rgb;
    });
    return () => { cancelled = true; };
  });

  $effect(() => {
    if (metaOpen && photo?.gps && !miniMap) setTimeout(initMini, 50);
    if (!metaOpen) destroyMini();
  });

  function fmtDate(iso: string | null): string {
    if (!iso) return "—";
    // The backend serializes naive (no Z) so this Date treats as local —
    // matching the wall-clock the photo recorded.
    const d = new Date(iso);
    return d.toLocaleString("en", {
      day: "numeric", month: "long", year: "numeric",
      hour: "numeric", minute: "2-digit",
    });
  }
  function fmtSize(b: number): string {
    return (b / 1024 / 1024).toFixed(1) + " MB";
  }

  const tintStyle = $derived(
    tint ? `--photo-tint: rgb(${tint[0]}, ${tint[1]}, ${tint[2]})` : ""
  );
</script>

<main class="detail" class:meta-open={metaOpen}>
  <section class="viewer-row">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="viewer"
      class:active={viewerActive}
      style={tintStyle}
      onmousemove={bumpActivity}
      onmouseenter={onViewerEnter}
      onmouseleave={onViewerLeave}
    >
      {#if error}
        <p class="error">{error}</p>
      {:else if photo && imageUrl}
        <ZoomImage
          src={imageUrl}
          alt={photo.file_name}
          rotate={totalRotation}
          bind:api={zoomApi}
        />
      {:else}
        <span class="loading muted mono">loading…</span>
      {/if}

      <!-- Floating toolbar -->
      <div class="toolbar">
        <button class="tool" onclick={back} title="Back (Esc)" aria-label="Back">
          <X size={16} strokeWidth={1.75} />
        </button>
        <span class="sep"></span>
        <button class="tool" onclick={() => zoomApi?.zoomOut()} title="Zoom out (−)" aria-label="Zoom out">
          <ZoomOut size={16} strokeWidth={1.75} />
        </button>
        <button class="tool" onclick={() => zoomApi?.fit()} title="Fit (0)" aria-label="Fit">
          <Maximize2 size={16} strokeWidth={1.75} />
        </button>
        <button class="tool" onclick={() => zoomApi?.zoomIn()} title="Zoom in (+)" aria-label="Zoom in">
          <ZoomIn size={16} strokeWidth={1.75} />
        </button>
        <span class="sep"></span>
        <button class="tool" onclick={rotateCcw} title="Rotate CCW ([)" aria-label="Rotate counter-clockwise">
          <RotateCcw size={16} strokeWidth={1.75} />
        </button>
        <button class="tool" onclick={rotateCw} title="Rotate CW (])" aria-label="Rotate clockwise">
          <RotateCw size={16} strokeWidth={1.75} />
        </button>
        <span class="sep"></span>
        <button class="tool" onclick={revealInFolder} title="Show in folder" aria-label="Show in folder">
          <FolderOpen size={16} strokeWidth={1.75} />
        </button>
        <button class="tool" class:on={metaOpen} onclick={() => (metaOpen = !metaOpen)} title="Toggle info (I)" aria-label="Toggle info">
          <Info size={16} strokeWidth={1.75} />
        </button>
      </div>

      <!-- Edge chevrons — visible on hover -->
      {#if prevId !== null}
        <button class="chevron prev" onclick={() => gotoId(prevId)} title="Previous (←)" aria-label="Previous">
          <ChevronLeft size={22} strokeWidth={2} />
        </button>
      {/if}
      {#if nextId !== null}
        <button class="chevron next" onclick={() => gotoId(nextId)} title="Next (→)" aria-label="Next">
          <ChevronRight size={22} strokeWidth={2} />
        </button>
      {/if}

      <!-- Position indicator -->
      {#if position}
        <div class="position mono">
          {position.index} / {position.total}
        </div>
      {/if}

      <!-- Filename -->
      {#if photo}
        <div class="filename mono" title={photo.file_path}>{photo.file_name}</div>
      {/if}
    </div>

    {#if metaOpen && photo}
      {@const p = photo}
      <aside class="meta" tabindex="-1">
        <h2 class="when display">{fmtDate(p.date_taken)}</h2>

        {#if p.location}
          <p class="where">
            {p.location.city ?? "Unknown"}{#if p.location.country}, {p.location.country}{/if}
          </p>
        {/if}

        {#if people.length > 0}
          <hr class="hairline" />
          <h3 class="section">People</h3>
          <ul class="chips">
            {#each people as person (person.id)}
              <li>
                <a class="chip person-chip" href="#/person?id={person.id}">
                  <span class="dot" aria-hidden="true"></span>
                  <span>{person.name ?? "Unnamed"}</span>
                </a>
              </li>
            {/each}
          </ul>
        {/if}

        {#if albums.length > 0}
          <hr class="hairline" />
          <h3 class="section">In albums</h3>
          <ul class="chips">
            {#each albums as a (a.id)}
              <li>
                <a class="chip" href="#/album?id={a.id}">{a.name}</a>
              </li>
            {/each}
          </ul>
        {/if}

        <!-- Camera section -->
        {#if p.camera || extras?.software || extras?.exposure_bias}
          <hr class="hairline" />
          <h3 class="section">Camera</h3>
          <dl class="specs">
            {#if p.camera?.make || p.camera?.model}
              <dt>Camera</dt>
              <dd>{[p.camera?.make, p.camera?.model].filter(Boolean).join(" ")}</dd>
            {/if}
            {#if p.camera?.lens}<dt>Lens</dt><dd>{p.camera.lens}</dd>{/if}
            {#if p.camera?.iso}<dt>ISO</dt><dd class="mono">{p.camera.iso}</dd>{/if}
            {#if p.camera?.aperture}<dt>Aperture</dt><dd class="mono">{p.camera.aperture}</dd>{/if}
            {#if p.camera?.shutter_speed}<dt>Shutter</dt><dd class="mono">{p.camera.shutter_speed}</dd>{/if}
            {#if p.camera?.focal_length}<dt>Focal</dt><dd class="mono">{p.camera.focal_length}</dd>{/if}
            {#if extras?.exposure_bias}<dt>Bias</dt><dd class="mono">{extras.exposure_bias}</dd>{/if}
            {#if p.camera?.flash}<dt>Flash</dt><dd>{p.camera.flash}</dd>{/if}
            {#if extras?.software}<dt>Software</dt><dd>{extras.software}</dd>{/if}
          </dl>
        {/if}

        <!-- File section -->
        <hr class="hairline" />
        <h3 class="section">File</h3>
        <dl class="specs">
          <dt>Format</dt>
          <dd>{fileFormat(p.file_name)}</dd>
          <dt>Dimensions</dt>
          <dd class="mono">{p.width ?? "?"} × {p.height ?? "?"}</dd>
          <dt>Size</dt>
          <dd class="mono">{fmtSize(p.file_size)}</dd>
          {#if p.orientation && p.orientation !== 1}
            <dt>Orientation</dt>
            <dd>{orientationLabel(p.orientation)}</dd>
          {/if}
          {#if extras?.modified_at}
            <dt>Modified</dt>
            <dd class="mono">{fmtDate(extras.modified_at)}</dd>
          {/if}
          {#if extras?.created_at}
            <dt>Created</dt>
            <dd class="mono">{fmtDate(extras.created_at)}</dd>
          {/if}
          <dt>Path</dt>
          <dd class="path mono" title={p.file_path}>{p.file_path}</dd>
        </dl>
        <button class="reveal" onclick={revealInFolder}>
          <FolderOpen size={13} strokeWidth={1.75} />
          <span>Show in folder</span>
        </button>

        {#if p.ocr}
          <hr class="hairline" />
          <h3 class="section">Transcribed text</h3>
          <p class="ocr">{p.ocr.text}</p>
        {/if}

        {#if p.gps}
          <hr class="hairline" />
          <h3 class="section">On the map</h3>
          <div class="mini-wrap">
            <div class="mini" bind:this={miniEl}></div>
            <div class="coords mono small">
              <span>{p.gps.lat.toFixed(4)}°, {p.gps.lng.toFixed(4)}°</span>
              {#if p.gps.altitude != null}
                <span class="alt">{p.gps.altitude.toFixed(0)} m</span>
              {/if}
            </div>
          </div>
        {/if}
      </aside>
    {/if}
  </section>
</main>

<style>
  .detail {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }

  .viewer-row {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 0px;
    overflow: hidden;
    transition: grid-template-columns var(--t-base-d) var(--ease);
  }
  .meta-open .viewer-row { grid-template-columns: 1fr 380px; }

  .viewer {
    position: relative;
    overflow: hidden;
    background-color: color-mix(in oklab, var(--bg) 93%, var(--photo-tint, transparent));
    background-image: radial-gradient(
      ellipse at center,
      transparent 0%,
      color-mix(in oklab, transparent 75%, black) 100%
    );
    transition: background-color 280ms var(--ease);
    cursor: none;
  }
  .viewer.active { cursor: default; }

  .loading {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
  }

  /* ===== floating toolbar ===== */
  .toolbar {
    position: absolute;
    top: var(--s-3);
    right: var(--s-3);
    display: flex;
    gap: 2px;
    align-items: center;
    padding: 4px;
    background: color-mix(in oklab, var(--bg-paper) 78%, transparent);
    backdrop-filter: blur(10px);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    z-index: 5;
    opacity: 0;
    transition: opacity 180ms var(--ease);
  }
  .viewer.active .toolbar,
  .toolbar:focus-within,
  .toolbar:hover { opacity: 1; }
  .tool {
    width: 30px;
    height: 30px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    color: var(--ink-soft);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
  }
  .tool:hover {
    background: var(--bg-card);
    color: var(--ink);
  }
  .tool.on {
    background: var(--accent-ghost);
    color: var(--accent);
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--line);
    margin: 0 2px;
  }

  /* ===== edge chevrons ===== */
  .chevron {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 44px;
    height: 44px;
    padding: 0;
    background: color-mix(in oklab, var(--bg-paper) 70%, transparent);
    backdrop-filter: blur(8px);
    border: 1px solid var(--line);
    border-radius: 50%;
    color: var(--ink);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    z-index: 4;
    opacity: 0;
    transition: opacity 200ms var(--ease),
                background var(--t-fast) var(--ease);
  }
  .chevron.prev { left: var(--s-4); }
  .chevron.next { right: var(--s-4); }
  .viewer.active .chevron { opacity: 1; }
  .chevron:hover { background: var(--bg-card); opacity: 1 !important; }

  /* ===== bottom-left position indicator ===== */
  .position {
    position: absolute;
    bottom: var(--s-3);
    left: var(--s-4);
    font-size: var(--t-xs);
    color: var(--ink-soft);
    background: color-mix(in oklab, var(--bg-paper) 70%, transparent);
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    z-index: 4;
    opacity: 0;
    transition: opacity 200ms var(--ease);
  }
  .viewer.active .position { opacity: 1; }

  /* ===== bottom-right filename ===== */
  .filename {
    position: absolute;
    bottom: var(--s-3);
    right: var(--s-4);
    font-size: var(--t-xs);
    color: var(--ink-muted);
    background: color-mix(in oklab, var(--bg-paper) 70%, transparent);
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    z-index: 4;
    opacity: 0;
    transition: opacity 200ms var(--ease);
  }
  .viewer.active .filename { opacity: 1; }

  /* ===== meta panel ===== */
  .meta {
    background: var(--bg-paper);
    border-left: 1px solid var(--line-soft);
    padding: var(--s-6) var(--s-5);
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }
  .when {
    font-size: var(--t-xl);
    line-height: 1.2;
    margin-top: var(--s-1);
    font-weight: 500;
    font-variation-settings: "opsz" 28, "wdth" 100;
    color: var(--ink);
  }
  .where {
    font-size: var(--t-base);
    color: var(--ink-soft);
    margin-top: -2px;
  }
  .section {
    font-size: var(--t-xs);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
    color: var(--ink-muted);
    margin: 0;
  }
  .hairline { margin: var(--s-2) 0; }

  .chips {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: 999px;
    font-size: var(--t-xs);
    color: var(--ink);
    text-decoration: none;
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease);
  }
  .chip:hover {
    background: var(--bg-elev);
    border-color: var(--ink-faint);
  }
  .person-chip .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }

  .specs {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 6px var(--s-4);
    margin: 0;
  }
  .specs dt {
    font-size: var(--t-xs);
    color: var(--ink-muted);
    padding-top: 2px;
  }
  .specs dd {
    margin: 0;
    font-size: var(--t-sm);
    color: var(--ink-soft);
    word-break: break-word;
  }
  .specs dd.path { font-size: 11px; color: var(--ink-muted); }

  .reveal {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    margin-top: 6px;
    font-size: var(--t-xs);
    color: var(--ink-soft);
  }
  .reveal:hover { color: var(--ink); }

  .ocr {
    font-style: italic;
    font-size: var(--t-sm);
    background: var(--bg-card);
    padding: var(--s-3) var(--s-4);
    border-radius: var(--r-sm);
    border-left: 2px solid var(--accent);
    color: var(--ink-soft);
    line-height: 1.6;
  }

  .mini-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mini {
    width: 100%;
    height: 200px;
    border-radius: var(--r-sm);
    overflow: hidden;
    background: var(--bg-card);
    border: 1px solid var(--line);
  }
  :global(.mini-pin) {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid var(--bg-paper);
    box-shadow: 0 0 0 1px var(--accent),
                0 4px 10px rgba(0, 0, 0, 0.5);
  }
  .coords {
    display: flex;
    gap: var(--s-3);
    color: var(--ink-muted);
    font-size: 10.5px;
  }
  .coords .alt { color: var(--ink-faint); }
  .small { font-size: var(--t-xs); }

  @media (max-width: 920px) {
    .meta-open .viewer-row {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr auto;
    }
    .meta {
      border-left: none;
      border-top: 1px solid var(--line-soft);
      max-height: 40vh;
    }
  }
</style>
