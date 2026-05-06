<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { photos } from "../lib/api/photos";
  import { library } from "../lib/api/library";
  import { call } from "../lib/api/index";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import maplibregl, { type Map as MapInstance } from "maplibre-gl";
  import "maplibre-gl/dist/maplibre-gl.css";
  import type { PhotoDto, PersonDto, AlbumDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let photo = $state<PhotoDto | null>(null);
  let imageUrl = $state<string | null>(null);
  let people = $state<PersonDto[]>([]);
  let albums = $state<AlbumDto[]>([]);
  let error = $state<string | null>(null);
  let metaOpen = $state(true);

  let miniEl: HTMLDivElement | undefined = $state();
  let miniMap: MapInstance | null = null;

  function back() { history.back(); }

  async function load() {
    error = null;
    imageUrl = null;
    people = [];
    albums = [];
    destroyMini();
    try {
      photo = await photos.get(id);
      // Resolve absolute path for the full-res image. If asset protocol is on,
      // convertFileSrc gives us a fetchable URL.
      try {
        const { absolute_path } = await library.resolvePath(id);
        imageUrl = convertFileSrc(absolute_path);
      } catch {}
      // People + albums in parallel — both are cheap.
      try {
        people = await call<PersonDto[]>("photos_people_in_photo", { photo_id: id });
      } catch {}
      try {
        albums = await call<AlbumDto[]>("photos_albums_for_photo", { photo_id: id });
      } catch {}
      // Minimap if we have GPS.
      if (photo?.gps && metaOpen) {
        // Defer to next tick so miniEl is bound after render.
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
      zoom: 12,
      interactive: false,
      attributionControl: false,
    });
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

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") back();
      if ((e.key === "i" || e.key === "I") && !(e.target instanceof HTMLInputElement)) {
        metaOpen = !metaOpen;
        if (metaOpen && photo?.gps) setTimeout(initMini, 50);
      }
    };
    window.addEventListener("keydown", onKey);
    load();
    return () => window.removeEventListener("keydown", onKey);
  });

  onDestroy(() => destroyMini());

  $effect(() => {
    void id;
    load();
  });

  $effect(() => {
    // Re-init mini when toggling open with GPS data already loaded.
    if (metaOpen && photo?.gps && !miniMap) setTimeout(initMini, 50);
    if (!metaOpen) destroyMini();
  });

  function fmtDate(iso: string | null): string {
    if (!iso) return "—";
    const d = new Date(iso);
    return d.toLocaleString("en", {
      day: "numeric", month: "long", year: "numeric",
      hour: "numeric", minute: "2-digit",
    });
  }
  function fmtSize(b: number): string {
    return (b / 1024 / 1024).toFixed(1) + " MB";
  }
</script>

<main class="detail" class:meta-closed={!metaOpen}>
  <header class="bar">
    <button class="ghost" onclick={back}>← Back</button>
    <span class="filename mono">{photo?.file_name ?? ""}</span>
    <button
      class="ghost"
      onclick={() => (metaOpen = !metaOpen)}
      title="Toggle details (I)"
    >
      {metaOpen ? "Hide details" : "Show details"}
      <span class="kbd-hint mono">I</span>
    </button>
  </header>

  <section class="viewer-row">
    <div class="viewer">
      {#if error}
        <p class="error">{error}</p>
      {:else if photo && imageUrl}
        <img src={imageUrl} alt={photo.file_name} />
      {:else}
        <span class="loading muted mono">loading…</span>
      {/if}

      {#if !metaOpen}
        <button
          class="floating-toggle"
          onclick={() => (metaOpen = true)}
          aria-label="Show details"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M9 3L5 7L9 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
      {/if}
    </div>

    {#if metaOpen && photo}
      <aside class="meta">
        <span class="eyebrow">
          <span class="num">№&nbsp;{String(photo.id).padStart(4, "0")}</span>
          <span class="ornament"></span>
          <span>DETAILS</span>
        </span>

        <h2 class="when">{fmtDate(photo.date_taken)}</h2>

        {#if photo.location}
          <p class="where">
            {photo.location.city ?? "Unknown"}{#if photo.location.country}, <em>{photo.location.country}</em>{/if}
          </p>
        {/if}

        {#if people.length > 0}
          <hr class="rule" />
          <span class="eyebrow"><span class="ornament"></span><span>PEOPLE</span></span>
          <ul class="chips">
            {#each people as p (p.id)}
              <li>
                <a class="chip person-chip" href="#/person?id={p.id}">
                  <span class="dot" aria-hidden="true"></span>
                  <span>{p.name ?? "Unnamed"}</span>
                </a>
              </li>
            {/each}
          </ul>
        {/if}

        {#if albums.length > 0}
          <hr class="rule" />
          <span class="eyebrow"><span class="ornament"></span><span>IN ALBUMS</span></span>
          <ul class="chips">
            {#each albums as a (a.id)}
              <li>
                <a class="chip" href="#/album?id={a.id}">{a.name}</a>
              </li>
            {/each}
          </ul>
        {/if}

        {#if photo.gps}
          <hr class="rule" />
          <span class="eyebrow"><span class="ornament"></span><span>ON THE MAP</span></span>
          <div class="mini-wrap">
            <div class="mini" bind:this={miniEl}></div>
            <span class="coords mono small">
              {photo.gps.lat.toFixed(4)}°, {photo.gps.lng.toFixed(4)}°
            </span>
          </div>
        {/if}

        <hr class="rule" />
        <span class="eyebrow"><span class="ornament"></span><span>TECHNICAL</span></span>

        <dl class="specs">
          {#if photo.camera}
            {#if photo.camera.make || photo.camera.model}
              <dt>Camera</dt>
              <dd>{photo.camera.make ?? ""} {photo.camera.model ?? ""}</dd>
            {/if}
            {#if photo.camera.lens}<dt>Lens</dt><dd>{photo.camera.lens}</dd>{/if}
            {#if photo.camera.iso}<dt>ISO</dt><dd class="mono">{photo.camera.iso}</dd>{/if}
            {#if photo.camera.aperture}<dt>Aperture</dt><dd class="mono">{photo.camera.aperture}</dd>{/if}
            {#if photo.camera.shutter_speed}<dt>Shutter</dt><dd class="mono">{photo.camera.shutter_speed}</dd>{/if}
            {#if photo.camera.focal_length}<dt>Focal</dt><dd class="mono">{photo.camera.focal_length}</dd>{/if}
          {/if}
          <dt>Dimensions</dt>
          <dd class="mono">{photo.width ?? "?"} × {photo.height ?? "?"}</dd>
          <dt>Size</dt>
          <dd class="mono">{fmtSize(photo.file_size)}</dd>
          <dt>Path</dt>
          <dd class="path mono" title={photo.file_path}>{photo.file_path}</dd>
        </dl>

        {#if photo.ocr}
          <hr class="rule" />
          <span class="eyebrow"><span class="ornament"></span><span>TRANSCRIBED TEXT</span></span>
          <p class="ocr">{photo.ocr.text}</p>
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
  .bar {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: var(--s-4);
    align-items: center;
    padding: var(--s-3) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    background: var(--bg-paper);
  }
  .filename {
    text-align: center;
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .kbd-hint {
    margin-left: var(--s-2);
    font-size: 9px;
    background: var(--bg-card);
    padding: 1px 6px;
    border-radius: 3px;
    color: var(--ink-faint);
  }

  .viewer-row {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 380px;
    overflow: hidden;
    transition: grid-template-columns var(--t-base-d) var(--ease);
  }
  .meta-closed .viewer-row { grid-template-columns: 1fr 0px; }

  .viewer {
    background: var(--bg);
    background-image: radial-gradient(
      ellipse at center,
      transparent 0%,
      rgba(0, 0, 0, 0.4) 100%
    );
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--s-5);
    overflow: hidden;
    position: relative;
  }
  .viewer img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    box-shadow: 0 30px 60px rgba(0, 0, 0, 0.5);
    animation: rise var(--t-slow) var(--ease-out);
  }

  .floating-toggle {
    position: absolute;
    top: var(--s-4);
    right: var(--s-4);
    width: 36px;
    height: 36px;
    padding: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(8px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--ink);
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease);
  }
  .floating-toggle:hover { background: rgba(0, 0, 0, 0.75); }

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
    font-size: var(--t-2xl);
    margin-top: var(--s-2);
    line-height: 1.1;
  }
  .where {
    font-family: var(--font-display);
    font-size: var(--t-base);
    font-weight: 400;
    color: var(--ink-soft);
    font-variation-settings: "opsz" 14;
  }
  .where em {
    font-style: italic;
    color: var(--accent);
  }

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
    transition: background var(--t-fast) var(--ease);
  }
  .chip:hover {
    background: var(--bg-elev);
    text-decoration: none;
  }
  .person-chip .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }

  .mini-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mini {
    width: 100%;
    height: 160px;
    border-radius: var(--r-sm);
    overflow: hidden;
    background: var(--bg-card);
    border: 1px solid var(--line);
  }
  :global(.mini-pin) {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid var(--bg-paper);
    box-shadow: 0 0 0 2px var(--accent), 0 4px 10px rgba(0, 0, 0, 0.5);
  }
  .coords {
    color: var(--ink-muted);
    font-size: 10.5px;
  }

  .specs {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 8px var(--s-4);
    margin: 0;
  }
  .specs dt {
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-faint);
    padding-top: 2px;
  }
  .specs dd {
    margin: 0;
    font-size: var(--t-sm);
    color: var(--ink-soft);
    word-break: break-word;
  }
  .specs dd.path { font-size: 11px; color: var(--ink-muted); }

  .ocr {
    font-family: var(--font-display);
    font-style: italic;
    font-variation-settings: "opsz" 14;
    font-size: var(--t-sm);
    background: var(--bg-card);
    padding: var(--s-4);
    border-radius: var(--r-sm);
    border-left: 2px solid var(--accent);
    color: var(--ink-soft);
    line-height: 1.6;
  }

  .loading {
    font-size: var(--t-sm);
    letter-spacing: 0.08em;
  }
  .small { font-size: var(--t-xs); }

  @media (max-width: 920px) {
    .viewer-row { grid-template-columns: 1fr; grid-template-rows: 1fr auto; }
    .meta { border-left: none; border-top: 1px solid var(--line-soft); max-height: 40vh; }
  }
</style>
