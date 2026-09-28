<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import {
    ChevronLeft,
    ChevronRight,
    Gauge,
    LocateFixed,
    Maximize2,
    Pause,
    Play,
    Repeat,
    Repeat1,
    X,
  } from "lucide-svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { commandErrorMessage } from "../api";
  import { photos } from "../api/photos";
  import { library } from "../api/library";
  import { slideshow } from "../stores/slideshow.svelte";
  import { thumbUrl } from "../thumbnail";
  import { libraryStore } from "../stores/library.svelte";
  import { decodeOffscreen } from "../decodeOffscreen";
  import { pooledImage } from "../assetImage";
  import { finishSlotTransition } from "../slideTransition";
  import { memoryContext } from "../surpriseHistory";
  import type { PhotoDto } from "../api/types";

  // Stable double-buffer. We keep two <img> elements mounted at all
  // times — one visible (front), one hidden (back). On advance, the
  // back gets the new src, we await decode(), then crossfade by
  // flipping `frontIdx`. The previous front then becomes the back,
  // ready for the next slide. Elements never unmount mid-slideshow,
  // which eliminates the "loading..." flash users were seeing.
  type Slot = { photo: PhotoDto | null; url: string | null; ready: boolean };
  let slots = $state<[Slot, Slot]>([
    { photo: null, url: null, ready: false },
    { photo: null, url: null, ready: false },
  ]);
  let frontIdx = $state(0);
  let loadError = $state<string | null>(null);
  let chromeActive = $state(true);
  let stageEl = $state<HTMLElement | undefined>(undefined);
  let dialogEl = $state<HTMLElement | undefined>(undefined);
  /// True once at least one image has been shown — gates auto-advance
  /// and lets the cold-start "loading..." disappear permanently.
  let booted = $state(false);
  let loading = $state(false);
  let introduction = $state(false);
  let introduced = false;
  const surprise = $derived(slideshow.kind === "surprise");
  let idleTimer: ReturnType<typeof setTimeout> | null = null;
  let advanceTimer: ReturnType<typeof setTimeout> | null = null;
  let loadSeq = 0;
  const URL_CACHE_CAP = 200;
  const urlCache = new Map<number, string>();
  const preloadedMedia = new Set<number>();
  const preloadInFlight = new Map<number, number>();

  const currentId = $derived(slideshow.currentId());
  const position = $derived(slideshow.position());
  const frontPhoto = $derived(slots[frontIdx].photo);
  const context = $derived(frontPhoto ? memoryContext(frontPhoto) : "");
  const frontUrl = $derived(slots[frontIdx].url);
  const backPhoto = $derived(slots[1 - frontIdx].photo);
  const backUrl = $derived(slots[1 - frontIdx].url);

  function bumpChrome() {
    chromeActive = true;
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = setTimeout(() => (chromeActive = false), 2200);
  }

  function close() {
    slideshow.close();
  }

  function showInTimeline() {
    const id = currentId;
    if (id == null) return;
    slideshow.close();
    window.location.hash = `/timeline?photo=${id}`;
  }

  /// Leaves the slideshow for the photo's own page with its info panel open.
  /// Deferred while playing: the slide is about to change underneath it.
  function showInfo() {
    const id = currentId;
    if (id == null) return;
    slideshow.close();
    window.location.hash = `/photo?id=${id}&info=1`;
  }

  async function toggleFullscreen() {
    try {
      const w = getCurrentWindow();
      await w.setFullscreen(!(await w.isFullscreen()));
    } catch {}
  }

  async function goNext() {
    if (loading) return;
    clearAdvanceTimer();
    // Keep the outgoing frame stable while the incoming slide prepares.
    // Hidden videos are reset by syncVideos after the slot is no longer visible.
    pauseVideos();
    await slideshow.next();
  }

  function goPrev() {
    if (loading) return;
    clearAdvanceTimer();
    pauseVideos();
    slideshow.prev();
  }

  function clearAdvanceTimer() {
    if (advanceTimer) clearTimeout(advanceTimer);
    advanceTimer = null;
  }

  function onKey(e: KeyboardEvent) {
    if (!slideshow.active) return;
    bumpChrome();
    if (e.key === "Tab" && dialogEl) {
      const controls = Array.from(dialogEl.querySelectorAll<HTMLElement>("button:not(:disabled), select, video[controls]"));
      const first = controls[0], last = controls.at(-1);
      if (e.shiftKey && (document.activeElement === first || document.activeElement === dialogEl)) {
        e.preventDefault(); last?.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault(); first?.focus();
      }
      return;
    }
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        close();
        break;
      case " ":
        e.preventDefault();
        e.stopPropagation();
        slideshow.togglePlaying();
        break;
      case "ArrowRight":
        e.preventDefault();
        e.stopPropagation();
        void goNext();
        break;
      case "ArrowLeft":
        e.preventDefault();
        e.stopPropagation();
        goPrev();
        break;
      case "f":
      case "F":
        if (!e.metaKey && !e.ctrlKey) {
          e.preventDefault();
          e.stopPropagation();
          void toggleFullscreen();
        }
        break;
      case "i":
      case "I":
        if (!e.metaKey && !e.ctrlKey && !slideshow.playing) {
          e.preventDefault();
          e.stopPropagation();
          showInfo();
        }
        break;
    }
  }

  async function resolveUrl(id: number): Promise<string> {
    const session = libraryStore.session;
    const driveRoot = libraryStore.driveRoot;
    const cached = urlCache.get(id);
    if (cached) {
      urlCache.delete(id);
      urlCache.set(id, cached);
      return cached;
    }
    const { absolute_path } = await library.resolvePath(id, true);
    if (session !== libraryStore.session || driveRoot !== libraryStore.driveRoot) {
      throw new Error("Open library changed");
    }
    const url = convertFileSrc(absolute_path);
    urlCache.set(id, url);
    while (urlCache.size > URL_CACHE_CAP) {
      const oldest = urlCache.keys().next().value;
      if (oldest == null) break;
      urlCache.delete(oldest);
      preloadedMedia.delete(oldest);
    }
    return url;
  }

  async function waitForSlotReady(idx: number): Promise<void> {
    await tick();
    const node = stageEl?.querySelector<HTMLElement>(`[data-slide-slot="${idx}"]`);
    if (!node) throw new Error("slide slot was not mounted");
    const media = node as HTMLImageElement & HTMLVideoElement;
    if (node instanceof HTMLImageElement && node.complete && node.naturalWidth > 0) return;
    // DOM shims (including jsdom) do not implement image decoding or native
    // load events. There is no readiness signal to await in that environment;
    // decodeOffscreen already provided the strongest available preparation.
    if (node instanceof HTMLImageElement && typeof node.decode !== "function") return;
    if (node instanceof HTMLVideoElement && node.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) return;
    await new Promise<void>((resolve, reject) => {
      let settled = false;
      const finish = (ok: boolean) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        media.removeEventListener("load", onReady);
        media.removeEventListener("loadeddata", onReady);
        media.removeEventListener("error", onError);
        ok ? resolve() : reject(new Error("visible slide failed to load"));
      };
      const onReady = () => finish(true);
      const onError = () => finish(false);
      const timer = setTimeout(() => finish(false), 8_000);
      media.addEventListener("load", onReady, { once: true });
      media.addEventListener("loadeddata", onReady, { once: true });
      media.addEventListener("error", onError, { once: true });
    });
  }

  async function loadSlide(id: number) {
    const seq = ++loadSeq;
    loading = true;
    loadError = null;
    try {
      const [p, url] = await Promise.all([photos.get(id), resolveUrl(id)]);
      if (seq !== loadSeq) return;
      if (p.media_type !== "video" && !preloadedMedia.has(id)) {
        // Non-strict: this is the slide about to be shown, so a decode
        // timeout must not become an error card. The image has loaded; only
        // decode() failed to settle (a known WebView2 ICC-JPEG hang), and
        // showing the bitmap beats refusing to show the photo at all.
        await decodeOffscreen(url, Image, { strict: false });
        if (seq !== loadSeq) return;
      }
      preloadedMedia.add(id);
      // Promote: assign the NEW slide to the back slot, then flip the
      // front pointer. Svelte's reactivity drives the crossfade via
      // class:ready bound to whether this slot is currently in front.
      const backSlot = 1 - frontIdx;
      await Promise.all([
        finishSlotTransition(stageEl?.querySelector(`[data-slide-slot="${backSlot}"]`) ?? null),
        finishSlotTransition(stageEl?.querySelector(`[data-backdrop-slot="${backSlot}"]`) ?? null),
      ]);
      if (seq !== loadSeq) return;
      slots[backSlot] = { photo: p, url, ready: false };
      await waitForSlotReady(backSlot);
      if (seq !== loadSeq) return;
      // The offscreen decode is the admission check; exposing the slot only
      // after a paint opportunity prevents a one-frame blank/resize flash.
      slots[backSlot].ready = true;
      frontIdx = backSlot;
      booted = true;
      loading = false;
      introduction = !introduced;
      introduced = true;
      slideshow.presented(id);
      void syncVideos();
      void preloadNeighbors();
      void slideshow.ensureMoreAhead();
    } catch (e) {
      if (seq !== loadSeq) return;
      loadError = commandErrorMessage(e);
      booted = true;
      loading = false;
    }
  }

  function pauseVideos() {
    stageEl?.querySelectorAll("video").forEach((video) => {
      video.pause();
    });
  }

  async function syncVideos() {
    await tick();
    if (!slideshow.active || !stageEl) return;
    stageEl.querySelectorAll("video").forEach((video) => {
      const visible = video.classList.contains("visible");
      if (!visible) {
        video.pause();
      } else if (slideshow.playing) {
        void video.play().catch(() => {});
      } else {
        video.pause();
      }
    });
  }

  async function preloadNeighbors() {
    const seq = loadSeq;
    const ids = slideshow.ids;
    const i = slideshow.index;
    // Surprise preloads ONLY the next slide: every preload is a full
    // original decode (plus a backend rendition for HEIC/RAW), and three
    // of those in flight starve the current slide on a USB drive — the
    // lag that made the slideshow stall between slides.
    const candidates = (
      slideshow.kind === "surprise"
        ? [ids[i + 1]]
        : [ids[i + 1], ids[i - 1], ids[i + 2]]
    ).filter((id): id is number => id != null);
    await Promise.all(
      candidates.map(async (id) => {
        if (preloadedMedia.has(id) || preloadInFlight.has(id)) return;
        const token = loadSeq;
        preloadInFlight.set(id, token);
        try {
          const [p, url] = await Promise.all([photos.get(id), resolveUrl(id)]);
          if (p.media_type !== "video") await decodeOffscreen(url);
          if (seq !== loadSeq || !slideshow.active) return;
          preloadedMedia.add(id);
        } catch {}
        finally {
          if (preloadInFlight.get(id) === token) preloadInFlight.delete(id);
        }
      }),
    );
  }

  $effect(() => {
    if (!slideshow.active || currentId == null) return;
    void loadSlide(currentId);
  });

  // Reset booted state when slideshow opens/closes so a re-open
  // shows the loading screen briefly until the first decode lands.
  $effect(() => {
    if (!slideshow.active) {
      loadSeq++;
      booted = false;
      loading = false;
      introduced = false;
      introduction = false;
      loadError = null;
      pauseVideos();
      slots = [
        { photo: null, url: null, ready: false },
        { photo: null, url: null, ready: false },
      ];
      urlCache.clear();
      preloadedMedia.clear();
      preloadInFlight.clear();
    }
  });

  // Auto-advance — re-runs whenever `currentId` changes (new slide is
  // up) so a fresh timer is set for each photo. Without depending on
  // currentId the timer only fired once and the slideshow stalled
  // after the first transition.
  $effect(() => {
    void currentId;
    clearAdvanceTimer();
    if (!slideshow.active || !slideshow.playing || !booted || loading) return;
    if (!loadError && frontPhoto?.media_type === "video") {
      void syncVideos();
      return clearAdvanceTimer;
    }
    advanceTimer = setTimeout(() => void goNext(), slideshow.intervalMs);
    return clearAdvanceTimer;
  });

  $effect(() => {
    void frontIdx;
    void slideshow.playing;
    void syncVideos();
  });

  $effect(() => {
    if (slideshow.active) {
      bumpChrome();
      const previousFocus = document.activeElement;
      void tick().then(() => { if (slideshow.active) dialogEl?.focus(); });
      window.addEventListener("keydown", onKey, { capture: true });
      return () => {
        window.removeEventListener("keydown", onKey, { capture: true });
        if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
      };
    }
  });

  onDestroy(() => {
    loadSeq++;
    if (idleTimer) clearTimeout(idleTimer);
    clearAdvanceTimer();
    pauseVideos();
  });
</script>

{#if slideshow.active}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="slideshow"
    class:slowshow={surprise}
    class:active={chromeActive}
    bind:this={dialogEl}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onmousemove={bumpChrome}
    onpointerdown={bumpChrome}
    aria-label={surprise ? "Surprise me slideshow" : "Slideshow"}
  >
    <div class="stage" bind:this={stageEl}>
      <!-- The blurred backdrop gets the SAME two-slot crossfade as the
           slides. A single backdrop that swaps src mid-fade hard-cuts
           while the foreground is still blending — the "double image". -->
      {#each slots as slot, idx (idx)}
        {#if slot.photo?.thumbnail_path}
          <img
            class="backdrop"
            data-backdrop-slot={idx}
            class:visible={idx === frontIdx && slot.ready && !loadError}
            use:pooledImage={{
              src: thumbUrl(libraryStore.driveRoot, slot.photo.thumbnail_path),
              // These coordinate visibility through `.visible` (opacity 0 ->
              // 0.75), so the pool must not drive their opacity: its inline
              // style beat the stylesheet, which both disabled the dimming and
              // left the later DOM slot permanently visible — so slot 0's
              // crossfade showed slot 1's photo.
              ownOpacity: true,
            }}
            alt=""
            aria-hidden="true"
          />
        {/if}
      {/each}

      {#if loadError}
        <div class="slide-error">
          <strong>Couldn't load this photo</strong>
          <span>{loadError}</span>
        </div>
      {/if}

      <!-- Two stable buffers. The one whose index matches `frontIdx`
           is visible (opacity 1); the other sits ready underneath. We
           never unmount, so there's no flash window. -->
      {#each slots as slot, idx (idx)}
        {#if slot.url && slot.photo}
          {#if slot.photo.media_type === "video"}
            <!-- svelte-ignore a11y_media_has_caption -->
            <video
              class="slide-image"
              data-slide-slot={idx}
              class:visible={idx === frontIdx && slot.ready && !loadError}
              src={slot.url}
              poster={slot.photo.thumbnail_path ? thumbUrl(libraryStore.driveRoot, slot.photo.thumbnail_path) ?? undefined : undefined}
              controls={idx === frontIdx}
              autoplay={idx === frontIdx && slideshow.playing}
              playsinline
              preload="metadata"
              onended={() => { if (idx === frontIdx && slideshow.playing) void goNext(); }}
            ></video>
          {:else}
            <img
              class="slide-image"
              data-slide-slot={idx}
              class:visible={idx === frontIdx && slot.ready && !loadError}
              src={slot.url}
              alt={slot.photo.file_name}
              decoding="async"
            />
          {/if}
        {/if}
      {/each}

      {#if !booted && !loadError}
        <div class="slide-loading mono">loading...</div>
      {/if}
    </div>

    {#if surprise && frontPhoto && !loadError && !loading}
      {#key frontPhoto.id}
        <div class="memory-context">
          {#if introduction}<span class="memory-intro">Remember this?</span>{/if}
          {#if context}<span>{context}</span>{/if}
        </div>
      {/key}
    {/if}

    <div class="topbar">
      <button class="tool" onclick={close} title="Close (Esc)" aria-label="Close slideshow">
        <X size={17} strokeWidth={1.8} />
      </button>
      <div class="title">
        <strong>{surprise ? "Surprise me" : slideshow.label}</strong>
        {#if surprise}
          <span>{slideshow.label}</span>
        {:else if frontPhoto}
          <span class="mono" title={frontPhoto.file_name}>{frontPhoto.file_name}</span>
        {/if}
      </div>
      <button class="tool" onclick={toggleFullscreen} title="Fullscreen (F)" aria-label="Toggle fullscreen">
        <Maximize2 size={17} strokeWidth={1.8} />
      </button>
      <button class="tool" onclick={showInTimeline} title="Show in timeline" aria-label="Show current photo in timeline">
        <LocateFixed size={17} strokeWidth={1.8} />
      </button>
    </div>

    <button class="edge prev" onclick={goPrev} title="Previous (←)" aria-label="Previous photo">
      <ChevronLeft size={26} strokeWidth={2} />
    </button>
    <button class="edge next" onclick={() => void goNext()} title="Next (→)" aria-label="Next photo">
      <ChevronRight size={26} strokeWidth={2} />
    </button>

    <div class="controls">
      <button class="control" onclick={() => slideshow.togglePlaying()} title="Play / pause (Space)" aria-label="Play or pause">
        {#if slideshow.playing}
          <Pause size={18} strokeWidth={1.9} />
        {:else}
          <Play size={18} strokeWidth={1.9} />
        {/if}
      </button>
      <button class="control" onclick={goPrev} title="Previous" aria-label="Previous photo">
        <ChevronLeft size={18} strokeWidth={1.9} />
      </button>
      <button class="control" onclick={() => void goNext()} title="Next" aria-label="Next photo">
        <ChevronRight size={18} strokeWidth={1.9} />
      </button>
      <span class="divider"></span>
      <label class="speed" title="Slide duration">
        <Gauge size={16} strokeWidth={1.8} />
        <select
          aria-label="Slide duration"
          value={String(slideshow.intervalMs)}
          onchange={(e) => slideshow.setInterval(Number((e.currentTarget as HTMLSelectElement).value))}
        >
          <option value="3000">3s</option>
          <option value="5000">5s</option>
          <option value="8000">8s</option>
          <option value="12000">12s</option>
        </select>
      </label>
      {#if !surprise}
      <button class="control" class:on={slideshow.loop} onclick={() => slideshow.toggleLoop()} title="Loop slideshow" aria-label="Toggle loop">
        {#if slideshow.loop}
          <Repeat1 size={17} strokeWidth={1.8} />
        {:else}
          <Repeat size={17} strokeWidth={1.8} />
        {/if}
      </button>
      {/if}
    </div>

    {#if position && !surprise}
      <div class="progress mono">
        {position.index} / {position.total}{#if slideshow.loadingMore}+{/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Crossfade duration is shared by slides AND backdrops so the blur
     track never cuts while the foreground is still blending. Surprise
     uses a slightly longer, calmer fade. */
  .slideshow { --crossfade: 420ms; }
  .slideshow.slowshow { --crossfade: 500ms; }
  .slowshow .slide-image {
    transform: translate(-50%, -50%);
    transition: opacity var(--crossfade) var(--ease);
  }
  .slowshow .slide-image.visible { transform: translate(-50%, -50%); }
  .memory-context {
    position: absolute; left: var(--s-5); right: var(--s-5); bottom: 88px;
    z-index: 2; display: flex; flex-direction: column; gap: 4px;
    align-items: center; text-align: center; pointer-events: none;
    color: rgba(255,255,255,0.8); font-size: var(--t-sm);
    text-shadow: 0 2px 12px rgba(0,0,0,0.9);
    opacity: 0; animation: memory-context 5s var(--ease) both;
  }
  .memory-intro { font-family: var(--font-display); font-size: var(--t-lg); }
  @keyframes memory-context {
    0%, 100% { opacity: 0; }
    12%, 70% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .slide-image, .slowshow .slide-image { transition: none; transform: translate(-50%, -50%); }
    .slide-image.visible { transform: translate(-50%, -50%); }
    .backdrop { transition: none; }
    .memory-context { animation-timing-function: steps(1, end); }
  }
  .slideshow {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: #050505;
    color: white;
    overflow: hidden;
    cursor: none;
  }
  .slideshow.active {
    cursor: default;
  }
  .stage {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .backdrop {
    position: absolute;
    inset: -8%;
    width: 116%;
    height: 116%;
    object-fit: cover;
    filter: blur(34px) saturate(1.15) brightness(0.38);
    transform: scale(1.03);
    opacity: 0;
    transition: opacity var(--crossfade) var(--ease);
  }
  .backdrop.visible {
    opacity: 0.75;
  }
  .stage::after {
    content: "";
    position: absolute;
    inset: 0;
    background:
      radial-gradient(ellipse at center, transparent 0%, rgba(0,0,0,0.42) 82%),
      linear-gradient(to bottom, rgba(0,0,0,0.35), transparent 18%, transparent 72%, rgba(0,0,0,0.45));
    pointer-events: none;
  }
  /* Robust centering: top/left 50% + translate(-50%, -50%). Doesn't
     depend on intrinsic dimensions being known at layout time, doesn't
     interact with grid/flex parent semantics. max-width/max-height keep
     the image inside the viewport while preserving aspect ratio. */
  .slide-image {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 1;
    /* Give both slots a stable viewport-sized box. The bitmap is fitted
       inside it, so late intrinsic dimensions cannot move a visible slide. */
    width: 100vw;
    height: 100vh;
    max-width: 100vw;
    max-height: 100vh;
    object-fit: contain;
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.985);
    transition: opacity 420ms var(--ease), transform 650ms var(--ease);
    box-shadow: 0 22px 80px rgba(0,0,0,0.42);
    pointer-events: none;
  }
  .slide-image.visible {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
  video.slide-image.visible {
    pointer-events: auto;
  }
  .slide-loading,
  .slide-error {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 3;
    color: rgba(255,255,255,0.76);
  }
  .slide-error {
    width: min(520px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 18px 20px;
    background: rgba(20,20,20,0.78);
    border: 1px solid rgba(255,255,255,0.14);
    border-radius: var(--r-md);
    backdrop-filter: blur(16px);
  }
  .slide-error strong {
    font-size: var(--t-base);
  }
  .slide-error span {
    font-size: var(--t-sm);
    color: rgba(255,255,255,0.68);
    word-break: break-word;
  }
  .topbar,
  .controls,
  .progress,
  .edge {
    opacity: 0;
    transition: opacity 180ms var(--ease), transform 180ms var(--ease), background 140ms var(--ease);
  }
  .active .topbar,
  .topbar:focus-within,
  .topbar:hover,
  .active .controls,
  .controls:focus-within,
  .controls:hover,
  .active .progress,
  .active .edge,
  .edge:focus-visible,
  .edge:hover {
    opacity: 1;
  }
  .topbar {
    position: absolute;
    top: var(--s-4);
    left: var(--s-4);
    right: var(--s-4);
    z-index: 4;
    display: grid;
    grid-template-columns: 38px minmax(0, 1fr) 38px 38px;
    align-items: center;
    gap: var(--s-3);
  }
  .title {
    justify-self: center;
    min-width: 0;
    max-width: min(620px, 72vw);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    text-align: center;
    color: rgba(255,255,255,0.92);
    text-shadow: 0 1px 12px rgba(0,0,0,0.5);
  }
  .title strong {
    font-size: var(--t-sm);
    font-weight: 600;
  }
  .title span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgba(255,255,255,0.62);
    font-size: var(--t-xs);
  }
  .tool,
  .control,
  .edge {
    color: rgba(255,255,255,0.86);
    background: rgba(18,18,18,0.58);
    border: 1px solid rgba(255,255,255,0.14);
    backdrop-filter: blur(18px);
    cursor: pointer;
  }
  .tool,
  .control {
    width: 38px;
    height: 38px;
    padding: 0;
    border-radius: var(--r-md);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .tool:hover,
  .control:hover,
  .control.on,
  .edge:hover {
    background: rgba(255,255,255,0.16);
    color: white;
  }
  .edge {
    position: absolute;
    top: 50%;
    z-index: 4;
    width: 52px;
    height: 52px;
    padding: 0;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transform: translateY(-50%);
  }
  .edge.prev { left: var(--s-5); }
  .edge.next { right: var(--s-5); }
  .controls {
    position: absolute;
    left: 50%;
    bottom: var(--s-5);
    z-index: 4;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 6px;
    border: 1px solid rgba(255,255,255,0.14);
    background: rgba(12,12,12,0.62);
    border-radius: var(--r-md);
    backdrop-filter: blur(18px);
    transform: translateX(-50%) translateY(8px);
  }
  .active .controls,
  .controls:hover,
  .controls:focus-within {
    transform: translateX(-50%) translateY(0);
  }
  .divider {
    width: 1px;
    height: 22px;
    background: rgba(255,255,255,0.16);
    margin: 0 3px;
  }
  .speed {
    height: 38px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    color: rgba(255,255,255,0.82);
    background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: var(--r-md);
  }
  .speed select {
    width: 56px;
    color: white;
    background: transparent;
    border: none;
    font-size: var(--t-sm);
    outline: none;
  }
  .speed option {
    color: black;
  }
  .progress {
    position: absolute;
    right: var(--s-5);
    bottom: var(--s-5);
    z-index: 4;
    padding: 7px 11px;
    color: rgba(255,255,255,0.7);
    background: rgba(12,12,12,0.55);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 999px;
    backdrop-filter: blur(18px);
    font-size: var(--t-xs);
  }

  @media (max-width: 720px) {
    .topbar {
      top: var(--s-3);
      left: var(--s-3);
      right: var(--s-3);
    }
    .edge {
      width: 44px;
      height: 44px;
    }
    .edge.prev { left: var(--s-3); }
    .edge.next { right: var(--s-3); }
    .controls {
      bottom: var(--s-3);
      max-width: calc(100vw - 24px);
    }
    .progress {
      display: none;
    }
    .title {
      max-width: calc(100vw - 120px);
    }
  }
</style>
