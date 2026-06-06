<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    ChevronLeft, ChevronRight, Info, ZoomIn, ZoomOut, Fullscreen,
    RotateCcw, RotateCw, FolderOpen, FolderPlus, Layers, Play, Star, Trash2, X,
  } from "lucide-svelte";
  import { photos, type ExifExtras } from "../lib/api/photos";
  import { library } from "../lib/api/library";
  import { system } from "../lib/api/system";
  import { stacks, trash, type PhotoStack } from "../lib/api/all";
  import { call } from "../lib/api/index";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
  import { slideshow } from "../lib/stores/slideshow.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { probeVideoPoster } from "../lib/videoProbe";
  import { extractDominantColor, type RGB } from "../lib/dominantColor";
  import ZoomImage from "../lib/components/ZoomImage.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import type { ZoomApi } from "../lib/zoomApi";
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
  let extras = $state<ExifExtras | null>(null);
  let stack = $state<PhotoStack | null>(null);
  let error = $state<string | null>(null);
  let detailEl = $state<HTMLElement | undefined>(undefined);
  let immersive = $state(false);

  // Closed by default — info button reveals.
  let metaOpen = $state(false);
  let tint = $state<RGB | null>(null);
  let manualRotate = $state(0);
  let zoomApi = $state<ZoomApi | undefined>(undefined);
  let showAddDialog = $state(false);
  let stackTrayOpen = $state(false);
  /// Tracks the current zoom mode for keyboard fit/actual commands.
  /// We can't read ZoomImage's internal scale directly, so this mirrors
  /// the user's fit ↔ 1:1 intent across photo changes.
  let atActual = $state(false);
  let loadSeq = 0;
  const resolvedImageCache = new Map<number, string>();

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

  /// Navigate between photos via replaceState so the photo journey
  /// collapses into a single history entry — pressing X / Esc takes
  /// the user back to whatever route they came from (Timeline, Album,
  /// Person, etc.), not stepping back through every photo viewed.
  function gotoId(target: number | null) {
    if (target == null) return;
    const url = `#/photo?id=${target}`;
    history.replaceState({}, "", url);
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  }

  const navAnchorId = $derived.by(() => {
    if (!photo) return null;
    if (browseContext.ids.includes(photo.id)) return photo.id;
    if (stack && browseContext.ids.includes(stack.cover_photo_id)) return stack.cover_photo_id;
    return photo.id;
  });
  const prevId = $derived(navAnchorId != null ? browseContext.prev(navAnchorId) : null);
  const nextId = $derived(navAnchorId != null ? browseContext.next(navAnchorId) : null);
  const position = $derived(navAnchorId != null ? browseContext.position(navAnchorId) : null);

  /// EXIF orientation → degrees of CSS rotation.
  /// Mirror variants (2/4/5/7) aren't auto-flipped for v1; their rotation
  /// portion is still applied so e.g. "5" reads as a quarter-turn.
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
    // Chromium/WebView already applies EXIF orientation when decoding
    // the original image. Applying it again here double-rotates portrait
    // files, so the viewer transform is reserved for user rotation.
    return (manualRotate + 360) % 360;
  });
  const isVideo = $derived(photo?.media_type === "video");
  const posterUrl = $derived(
    photo?.thumbnail_path ? thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) : undefined
  );

  function fileFormat(name: string): string {
    const m = name.toLowerCase().match(/\.([a-z0-9]+)$/);
    if (!m) return "—";
    const ext = m[1];
    const mapping: Record<string, string> = {
      // Stills
      jpg: "JPEG", jpeg: "JPEG", png: "PNG", gif: "GIF", webp: "WebP",
      heic: "HEIC", heif: "HEIF", avif: "AVIF",
      tif: "TIFF", tiff: "TIFF", bmp: "BMP",
      // RAW group (decoded via embedded JPEG preview — see
      // services::raw_preview in the engine). The "(camera RAW)"
      // suffix makes the info panel obvious at a glance.
      raw: "RAW",
      nef: "NEF (Nikon RAW)",
      cr2: "CR2 (Canon RAW)",
      cr3: "CR3 (Canon RAW)",
      arw: "ARW (Sony RAW)",
      dng: "DNG",
      orf: "ORF (Olympus RAW)",
      rw2: "RW2 (Panasonic RAW)",
      pef: "PEF (Pentax RAW)",
      rwl: "RWL (Leica RAW)",
      srw: "SRW (Samsung RAW)",
      raf: "RAF (Fujifilm RAW)",
      // Videos
      mp4: "MP4 video",
      m4v: "M4V video",
      mov: "QuickTime video",
      webm: "WebM video",
      mkv: "Matroska video",
      avi: "AVI video",
      "3gp": "3GPP video",
      "3g2": "3GPP2 video",
      mts: "AVCHD video",
      m2ts: "AVCHD video",
    };
    return mapping[ext] ?? ext.toUpperCase();
  }

  function fmtDuration(ms: number | null): string {
    if (ms == null || ms <= 0) return "—";
    const total = Math.round(ms / 1000);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    return h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
      : `${m}:${String(s).padStart(2, "0")}`;
  }

  async function decodeImage(url: string): Promise<void> {
    const img = new Image();
    img.decoding = "async";
    img.src = url;
    if (img.decode) {
      try {
        await img.decode();
        return;
      } catch {
        // Fall through to the event path; some WebView decoders reject
        // decode() even though the image still paints.
      }
    }
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("image decode failed"));
    });
  }

  async function imageUrlFor(photoId: number): Promise<string> {
    const cached = resolvedImageCache.get(photoId);
    if (cached) return cached;
    const { absolute_path } = await library.resolvePath(photoId);
    const url = convertFileSrc(absolute_path);
    resolvedImageCache.set(photoId, url);
    return url;
  }

  async function preloadPhoto(photoId: number | null) {
    if (photoId == null || resolvedImageCache.has(photoId)) return;
    try {
      const url = await imageUrlFor(photoId);
      const p = await photos.get(photoId);
      if (p.media_type !== "video") await decodeImage(url);
    } catch {}
  }

  async function load() {
    const seq = ++loadSeq;
    error = null;
    people = [];
    albums = [];
    extras = null;
    stack = null;
    stackTrayOpen = false;
    tint = null;
    manualRotate = 0;
    atActual = false;
    destroyMini();
    try {
      // Parallelise the two IPC calls — they're independent. As soon
      // as BOTH the photo row and the resolved URL are in hand, drop
      // them into state. We deliberately do NOT await decode here:
      // ZoomImage owns the decode wait (its dimsKnown gate keeps the
      // img hidden until pixels are ready) and the thumbnail-as-
      // background fallback covers the visible transition. Awaiting
      // decode used to gate the entire UI update on a 100-500ms
      // image read which made arrow-key nav feel sticky on big RAWs.
      const [p, url] = await Promise.all([photos.get(id), imageUrlFor(id)]);
      if (seq !== loadSeq) return;
      photo = p;
      imageUrl = url;
      try {
        const nextPeople = await call<PersonDto[]>("photos_people_in_photo", { photo_id: id });
        if (seq === loadSeq) people = nextPeople;
      } catch {}
      try {
        const nextAlbums = await call<AlbumDto[]>("photos_albums_for_photo", { photo_id: id });
        if (seq === loadSeq) albums = nextAlbums;
      } catch {}
      try {
        const nextStack = await stacks.getForPhoto(id);
        if (seq === loadSeq) stack = nextStack;
      } catch {}
      if (p.media_type === "video" && !p.thumbnail_path) {
        probeVideoPoster(id)
          .then(async () => {
            const refreshed = await photos.get(id);
            if (photo?.id === id) photo = refreshed;
          })
          .catch(() => {});
      }
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
    catch (e) {
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      error = `Couldn't reveal this photo in the file manager: ${msg}`;
    }
  }

  async function toggleFullscreen() {
    const next = !immersive;
    immersive = next;
    if (next) {
      metaOpen = false;
      stackTrayOpen = false;
      bumpActivity();
      try { await detailEl?.requestFullscreen?.(); } catch {}
    } else {
      try {
        if (document.fullscreenElement) await document.exitFullscreen();
      } catch {}
      bumpActivity();
    }
  }

  function rotateCcw() { manualRotate = (manualRotate - 90 + 360) % 360; }
  function rotateCw()  { manualRotate = (manualRotate + 90) % 360; }

  function startSlideshow() {
    if (!photo) return;
    const ids = browseContext.ids.includes(photo.id) ? browseContext.ids : [photo.id];
    slideshow.start({
      kind: "photo",
      label: "Viewer",
      ids,
      startId: photo.id,
    });
  }

  async function trashAndAdvance() {
    if (!photo) return;
    const id = photo.id;
    const advanceTo = nextId ?? prevId ?? null;
    try {
      await trash.trashPhotos([id]);
      photoVisibility.markTrashed([id]);
      browseContext.remove([id]);
      toasts.undoable(
        "Photo moved to trash",
        async () => {
          await trash.restore([id]);
          photoVisibility.markRestored([id]);
          // Re-load the trashed-then-restored photo back into view.
          gotoId(id);
        },
      );
      if (advanceTo != null) gotoId(advanceTo);
      else back();
    } catch (e) {
      toasts.error(`Couldn't move to trash: ${e}`);
    }
  }

  async function toggleFavorite() {
    if (!photo) return;
    try {
      photo = await photos.setFavorite(photo.id, !photo.is_favorite);
    } catch (e) {
      toasts.error(`Couldn't update favourite: ${e}`);
    }
  }

  async function setStackCover(photoId: number) {
    if (!stack) return;
    try {
      stack = await stacks.setCover(stack.id, photoId);
      if (photo?.id !== photoId) gotoId(photoId);
    } catch (e) {
      toasts.error(`Couldn't set best photo: ${e}`);
    }
  }

  async function removeFromStack(photoId: number) {
    if (!stack) return;
    try {
      stack = await stacks.removeMember(stack.id, photoId);
      if (photo?.id === photoId) {
        const next = stack?.cover_photo_id ?? nextId ?? prevId;
        if (next) gotoId(next);
      }
    } catch (e) {
      toasts.error(`Couldn't remove from stack: ${e}`);
    }
  }

  async function unstack() {
    if (!stack) return;
    try {
      await stacks.unstack(stack.id);
      stack = null;
      toasts.success("Stack removed from timeline");
    } catch (e) {
      toasts.error(`Couldn't unstack: ${e}`);
    }
  }

  async function trashStackOthers() {
    if (!stack) return;
    if (!confirm("Move all other photos in this stack to trash?")) return;
    const keepId = stack.cover_photo_id;
    const viewingKeep = photo?.id === keepId;
    const trashedIds = stack.members.filter((member) => member.photo_id !== keepId).map((member) => member.photo_id);
    try {
      const result = await stacks.trashOthers(stack.id);
      photoVisibility.markTrashed(trashedIds);
      browseContext.remove(trashedIds);
      toasts.success(`${result.count} ${result.count === 1 ? "photo" : "photos"} moved to trash`);
      stack = null;
      if (!viewingKeep) gotoId(keepId);
    } catch (e) {
      toasts.error(`Couldn't trash stack members: ${e}`);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (e.target instanceof HTMLVideoElement) return;
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        if (immersive) void toggleFullscreen();
        else back();
        break;
      case "ArrowLeft":  e.preventDefault(); e.stopPropagation(); gotoId(prevId); break;
      case "ArrowRight": e.preventDefault(); e.stopPropagation(); gotoId(nextId); break;
      case "i": case "I":
        if (!e.metaKey && !e.ctrlKey) { metaOpen = !metaOpen; e.preventDefault(); e.stopPropagation(); }
        break;
      case "+": case "=":
        if (!isVideo) { e.preventDefault(); e.stopPropagation(); zoomApi?.zoomIn(); }
        break;
      case "-": case "_":
        if (!isVideo) { e.preventDefault(); e.stopPropagation(); zoomApi?.zoomOut(); }
        break;
      case "0":
        if (!isVideo) { e.preventDefault(); e.stopPropagation(); zoomApi?.fit(); atActual = false; }
        break;
      case "1":
        if (!isVideo) { e.preventDefault(); e.stopPropagation(); zoomApi?.actual(); atActual = true; }
        break;
      case "[":
        if (!isVideo) { e.preventDefault(); e.stopPropagation(); rotateCcw(); }
        break;
      case "]":
        if (!isVideo) { e.preventDefault(); e.stopPropagation(); rotateCw(); }
        break;
      case "f": case "F":
        if (!e.metaKey && !e.ctrlKey) { e.preventDefault(); e.stopPropagation(); toggleFullscreen(); }
        break;
      case "s": case "S":
        if (!e.metaKey && !e.ctrlKey) { e.preventDefault(); e.stopPropagation(); toggleFavorite(); }
        break;
      case "Delete": case "Backspace":
        e.preventDefault(); e.stopPropagation(); trashAndAdvance(); break;
      case "a": case "A":
        if (!e.metaKey && !e.ctrlKey) { e.preventDefault(); e.stopPropagation(); showAddDialog = true; }
        break;
    }
  }

  onMount(() => {
    const onFullscreenChange = () => {
      if (!document.fullscreenElement && immersive) immersive = false;
    };
    window.addEventListener("keydown", onKey, { capture: true });
    document.addEventListener("fullscreenchange", onFullscreenChange);
    return () => {
      window.removeEventListener("keydown", onKey, { capture: true });
      document.removeEventListener("fullscreenchange", onFullscreenChange);
    };
  });

  onDestroy(() => {
    destroyMini();
    if (idleTimer) clearTimeout(idleTimer);
  });

  $effect(() => { void id; load(); });

  $effect(() => {
    if (!photo) return;
    // Preload ±2 photos so two rapid arrow presses still hit cached
    // pixels. Browser decoder is fast enough that we don't need to
    // gate this on each step; resolvedImageCache + the browser HTTP
    // cache dedupe duplicate requests.
    const me = photo.id;
    void preloadPhoto(prevId);
    void preloadPhoto(nextId);
    const after = nextId != null ? browseContext.next(nextId) : null;
    const before = prevId != null ? browseContext.prev(prevId) : null;
    if (after != null && after !== me) void preloadPhoto(after);
    if (before != null && before !== me) void preloadPhoto(before);
  });

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

<main bind:this={detailEl} class="detail" class:meta-open={metaOpen && !immersive} class:immersive>
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
        <!-- ZoomImage double-buffers internally: the visible <img>
             keeps showing the previous photo until the next one is
             paint-ready, then swaps src + naturalW/H + scale in one
             tick. No blank gap during nav, so no backdrop is needed —
             we removed the blurred thumb-bg that previously sat
             behind. The viewer's background-color + radial vignette
             still cover the FIRST photo load (the very first time the
             user lands here from Timeline) for the ~50 ms before the
             initial decode completes. -->
        {#if photo.media_type === "video"}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            class="video-player"
            src={imageUrl}
            poster={posterUrl}
            controls
            preload="metadata"
            playsinline
          ></video>
        {:else}
          <ZoomImage
            src={imageUrl}
            alt={photo.file_name}
            rotate={totalRotation}
            preferredMode={atActual ? "actual" : "fit"}
            bind:api={zoomApi}
          />
        {/if}
      {:else}
        <span class="loading muted mono">loading…</span>
      {/if}

      <!-- Floating toolbar -->
      <div class="toolbar">
        <button class="tool" onclick={back} title="Back (Esc)" aria-label="Back">
          <X size={16} strokeWidth={1.75} />
        </button>
        <span class="sep"></span>
        {#if !isVideo}
          <button class="tool" onclick={() => { zoomApi?.zoomOut(); atActual = false; }} title="Zoom out (−)" aria-label="Zoom out">
            <ZoomOut size={16} strokeWidth={1.75} />
          </button>
          <button class="tool" onclick={() => { zoomApi?.zoomIn(); atActual = false; }} title="Zoom in (+)" aria-label="Zoom in">
            <ZoomIn size={16} strokeWidth={1.75} />
          </button>
          <button class="tool" onclick={toggleFullscreen} title="Toggle fullscreen (F)" aria-label="Toggle fullscreen">
            <Fullscreen size={16} strokeWidth={1.75} />
          </button>
          <span class="sep"></span>
          <button class="tool" onclick={rotateCcw} title="Rotate CCW ([)" aria-label="Rotate counter-clockwise">
            <RotateCcw size={16} strokeWidth={1.75} />
          </button>
          <button class="tool" onclick={rotateCw} title="Rotate CW (])" aria-label="Rotate clockwise">
            <RotateCw size={16} strokeWidth={1.75} />
          </button>
          <span class="sep"></span>
        {/if}
        <button class="tool" onclick={() => (showAddDialog = true)} title="Add to album (A)" aria-label="Add to album">
          <FolderPlus size={16} strokeWidth={1.75} />
        </button>
        <button
          class="tool"
          class:on={photo?.is_favorite ?? false}
          onclick={toggleFavorite}
          title={photo?.is_favorite ? "Remove from favourites (S)" : "Add to favourites (S)"}
          aria-label={photo?.is_favorite ? "Remove from favourites" : "Add to favourites"}
        >
          <Star size={16} strokeWidth={1.75} fill={photo?.is_favorite ? "currentColor" : "none"} />
        </button>
        <button class="tool" onclick={startSlideshow} title="Start slideshow" aria-label="Start slideshow">
          <Play size={16} strokeWidth={1.75} />
        </button>
        <button class="tool" onclick={revealInFolder} title="Show in folder" aria-label="Show in folder">
          <FolderOpen size={16} strokeWidth={1.75} />
        </button>
        {#if stack}
          <button
            class="tool stack-tool"
            class:on={stackTrayOpen}
            onclick={() => (stackTrayOpen = !stackTrayOpen)}
            title={stackTrayOpen ? "Hide stack filmstrip" : `Show ${stack.member_count} stacked photos`}
            aria-label={stackTrayOpen ? "Hide stack filmstrip" : "Show stack filmstrip"}
          >
            <Layers size={16} strokeWidth={1.75} />
            <span class="tool-count mono">{stack.member_count}</span>
          </button>
        {/if}
        <button class="tool danger" onclick={trashAndAdvance} title="Move to trash (Del)" aria-label="Move to trash">
          <Trash2 size={16} strokeWidth={1.75} />
        </button>
        <span class="sep"></span>
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

      {#if stack && stackTrayOpen}
        <div class="stack-tray" data-no-marquee="true">
          <div class="stack-head">
            <span class="stack-title mono">{stack.member_count} stacked</span>
            <div class="stack-actions">
              <button onclick={unstack}>Unstack</button>
              <button class="danger-text" onclick={trashStackOthers}>Trash others</button>
            </div>
          </div>
          <div class="stack-strip">
            {#each stack.members as member (member.photo_id)}
              <a
                class="stack-thumb"
                class:current={photo?.id === member.photo_id}
                class:cover={member.is_cover}
                href="#/photo?id={member.photo_id}"
                title={member.score_reasons ?? "Stack member"}
              >
                {#if member.thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, member.thumbnail_path) ?? ""} alt="" />
                {/if}
                {#if member.is_cover}<span class="best-label">Best</span>{/if}
              </a>
            {/each}
          </div>
          {#if photo}
            {@const currentMember = stack.members.find((m) => m.photo_id === photo?.id)}
            {#if currentMember}
              <div class="stack-current">
                <span class="stack-reason">{currentMember.score_reasons ?? "Suggested from image quality"}</span>
                {#if !currentMember.is_cover}
                  <button onclick={() => setStackCover(currentMember.photo_id)}>Set as best</button>
                  <button onclick={() => removeFromStack(currentMember.photo_id)}>Remove</button>
                {/if}
              </div>
            {/if}
          {/if}
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
          {#if p.media_type === "video"}
            <dt>Duration</dt>
            <dd class="mono">{fmtDuration(p.duration_ms)}</dd>
            {#if p.video?.video_codec}<dt>Video</dt><dd class="mono">{p.video.video_codec}</dd>{/if}
            {#if p.video?.audio_codec}<dt>Audio</dt><dd class="mono">{p.video.audio_codec}</dd>{/if}
            {#if p.video?.frame_rate}<dt>Frame rate</dt><dd class="mono">{p.video.frame_rate.toFixed(2)} fps</dd>{/if}
            {#if p.video?.bitrate}<dt>Bitrate</dt><dd class="mono">{(p.video.bitrate / 1000000).toFixed(1)} Mbps</dd>{/if}
          {/if}
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
            <div class="place-row">
              {#if p.location?.city || p.location?.country}
                <span class="place">
                  {[p.location?.city, p.location?.country].filter(Boolean).join(", ")}
                </span>
              {:else}
                <span class="coords mono small">
                  Approx. {p.gps.lat.toFixed(3)}°, {p.gps.lng.toFixed(3)}°
                </span>
              {/if}
              {#if p.gps.altitude != null}
                <span class="alt mono small">{p.gps.altitude.toFixed(0)} m</span>
              {/if}
            </div>
          </div>
        {/if}
      </aside>
    {/if}
  </section>
</main>

{#if showAddDialog && photo}
  <AddToAlbumDialog
    photoIds={[photo.id]}
    onclose={() => (showAddDialog = false)}
    onsuccess={(album, count) => toasts.success(`Added ${count} to ${album.name}`)}
  />
{/if}

<style>
  .detail {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .detail.immersive {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: #000;
  }
  .detail.immersive .viewer-row {
    grid-template-columns: 1fr;
  }
  .detail.immersive .viewer {
    background-color: #000;
    background-image: none;
  }
  .detail.immersive .meta {
    display: none;
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

  .video-player {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    background: #000;
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
  .stack-tool {
    gap: 2px;
    width: auto;
    min-width: 34px;
    padding: 0 7px;
  }
  .tool-count {
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
  }
  .tool.danger:hover {
    color: var(--danger, #d96363);
    background: color-mix(in oklab, var(--bg-card) 80%, var(--danger, #d96363));
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

  .stack-tray {
    position: absolute;
    left: 50%;
    bottom: var(--s-3);
    transform: translateX(-50%);
    width: min(720px, calc(100% - 180px));
    background: color-mix(in oklab, var(--bg-paper) 82%, transparent);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: var(--s-2);
    z-index: 5;
    opacity: 0;
    transition: opacity 200ms var(--ease);
  }
  .viewer.active .stack-tray,
  .stack-tray:hover,
  .stack-tray:focus-within { opacity: 1; }
  .stack-head,
  .stack-current {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-2);
  }
  .stack-title {
    font-size: var(--t-xs);
    color: var(--ink-soft);
    white-space: nowrap;
  }
  .stack-actions {
    display: flex;
    gap: var(--s-2);
  }
  .stack-actions button,
  .stack-current button {
    font-size: var(--t-xs);
    padding: 4px 8px;
    border-radius: var(--r-sm);
  }
  .danger-text {
    color: var(--danger, #d96363);
  }
  .stack-strip {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    padding: var(--s-2) 0;
  }
  .stack-thumb {
    position: relative;
    flex: 0 0 auto;
    width: 58px;
    height: 58px;
    border-radius: var(--r-sm);
    overflow: hidden;
    background: var(--bg-elev);
    border: 2px solid transparent;
  }
  .stack-thumb.current { border-color: var(--accent); }
  .stack-thumb.cover { box-shadow: 0 0 0 1px var(--keep) inset; }
  .stack-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .best-label {
    position: absolute;
    left: 3px;
    bottom: 3px;
    background: var(--keep);
    color: #fff;
    border-radius: 999px;
    padding: 1px 5px;
    font-size: 10px;
    font-weight: 700;
  }
  .stack-current {
    border-top: 1px solid var(--line-soft);
    padding-top: var(--s-2);
  }
  .stack-reason {
    color: var(--ink-muted);
    font-size: var(--t-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

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
  .place-row {
    display: flex;
    align-items: baseline;
    gap: var(--s-3);
    flex-wrap: wrap;
  }
  .place {
    font-size: var(--t-sm);
    color: var(--ink);
    font-weight: 500;
  }
  .coords {
    color: var(--ink-muted);
    font-size: 11px;
  }
  .alt { color: var(--ink-faint); }
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
