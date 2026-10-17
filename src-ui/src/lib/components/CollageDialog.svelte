<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { untrack } from "svelte";
  import Check from "lucide-svelte/icons/check";
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Copy from "lucide-svelte/icons/copy";
  import Download from "lucide-svelte/icons/download";
  import Shuffle from "lucide-svelte/icons/shuffle";
  import { commandErrorMessage } from "../api";
  import { collageApi, type CollageLocation } from "../api/all";
  import { library } from "../api/library";
  import { photos } from "../api/photos";
  import {
    BORDERS,
    EXPORT_LONG_SIDE,
    MAX_PHOTOS,
    MIN_PHOTOS,
    SHAPES,
    autoArrange,
    autoFocusAll,
    canvasSize,
    defaultFocus,
    hitTest,
    layoutsFor,
    matchColor,
    panFocus,
    renderCollage,
    type ArrangePhoto,
    type BorderId,
    type CollagePhoto,
    type FaceBox,
    type Focus,
    type Layout,
    type Rect,
    type ShapeId,
  } from "../collage";
  import { collage } from "../stores/collage.svelte";
  import { libraryStore } from "../stores/library.svelte";
  import { toasts } from "../stores/toast.svelte";

  /** Preview bitmaps are capped so dragging stays smooth with nine photos. */
  const PREVIEW_LONG_SIDE = 1400;
  const COPY_LONG_SIDE = 2000;

  interface Item {
    id: number;
    full: HTMLImageElement;
    preview: ImageBitmap;
    faces: FaceBox[];
    average: [number, number, number];
  }

  let dialog = $state<HTMLDialogElement | undefined>();
  let canvas = $state<HTMLCanvasElement | undefined>();
  let stageWidth = $state(0);
  let stageHeight = $state(0);

  let items = $state.raw<Item[]>([]);
  /** Slot → index into `items`. */
  let order = $state<number[]>([]);
  /** Crop per photo id, so a photo keeps its crop when it changes cell. */
  let focus = $state<Record<number, Focus>>({});
  /** Arranged automatically: photos are framed for whichever cell they're
   *  in until the user reframes them by hand. */
  let autoMode = $state(false);
  /** Photos the user has reframed by hand (auto mode only). */
  let touched = $state<Record<number, boolean>>({});
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let skipped = $state(0);

  let layoutId = $state<string | null>(null);
  let shape = $state<ShapeId>("square");
  let gap = $state(0.015);
  let radius = $state(0);
  let border = $state<BorderId>("white");
  let busy = $state(false);
  let flash = $state<string | null>(null);
  let saveLocation = $state<CollageLocation | null>(null);
  /** The last saved file, for "Show in folder". */
  let savedPath = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  let rects: Rect[] = [];
  let drag: {
    pointerId: number;
    slot: number;
    lastX: number;
    lastY: number;
    startFocus: Focus;
    wasTouched: boolean;
    swapping: boolean;
  } | null = null;
  let dragView = $state<{ origin: number; target: number } | null>(null);
  let hoverSlot = $state(-1);
  let dragging = $state(false);
  let cellBoxes = $state<Rect[]>([]);

  const layouts = $derived(layoutsFor(items.length));
  const layout = $derived<Layout>(layouts.find((l) => l.id === layoutId) ?? layouts[0]);
  const background = $derived(
    border === "match"
      ? matchColor(items.map((item) => item.average))
      : (BORDERS.find((b) => b.id === border)?.color ?? "#ffffff"),
  );
  const ratio = $derived(SHAPES.find((s) => s.id === shape)?.ratio ?? 1);
  /** Preview size in CSS pixels: the largest box of this shape that fits. */
  const view = $derived.by(() => {
    const w = Math.max(0, stageWidth);
    const h = Math.max(0, stageHeight);
    if (!w || !h) return { width: 0, height: 0 };
    return w / h > ratio ? { width: h * ratio, height: h } : { width: w, height: w / ratio };
  });

  // ---- open / close ------------------------------------------------------

  $effect(() => {
    const element = dialog;
    if (!element) return;
    if (collage.open && !element.open) element.showModal();
    if (!collage.open && element.open) element.close();
  });

  $effect(() => {
    if (!collage.open || saveLocation) return;
    collageApi.location().then((l) => (saveLocation = l), () => {});
  });

  $effect(() => {
    if (!collage.open) return;
    const ids = collage.ids;
    const arrange = untrack(() => ({ auto: collage.auto, preset: collage.preset }));
    let cancelled = false;
    void load(ids, arrange, () => cancelled);
    return () => {
      cancelled = true;
      release();
    };
  });

  function close() {
    collage.open = false;
  }

  function release() {
    for (const item of items) item.preview.close();
    items = [];
    order = [];
    focus = {};
    touched = {};
    autoMode = false;
    rects = [];
    cellBoxes = [];
    drag = null;
    dragView = null;
  }

  async function loadImage(id: number): Promise<HTMLImageElement> {
    const { absolute_path } = await library.resolvePath(id, true);
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.src = convertFileSrc(absolute_path);
    await img.decode();
    return img;
  }

  async function makePreview(img: HTMLImageElement): Promise<ImageBitmap> {
    const scale = Math.min(1, PREVIEW_LONG_SIDE / Math.max(img.naturalWidth, img.naturalHeight));
    return createImageBitmap(img, {
      resizeWidth: Math.max(1, Math.round(img.naturalWidth * scale)),
      resizeHeight: Math.max(1, Math.round(img.naturalHeight * scale)),
      resizeQuality: "high",
    });
  }

  function averageColor(bitmap: ImageBitmap): [number, number, number] {
    const probe = document.createElement("canvas");
    probe.width = probe.height = 8;
    const ctx = probe.getContext("2d", { willReadFrequently: true });
    if (!ctx) return [200, 200, 200];
    ctx.drawImage(bitmap, 0, 0, 8, 8);
    const { data } = ctx.getImageData(0, 0, 8, 8);
    let r = 0, g = 0, b = 0;
    for (let i = 0; i < data.length; i += 4) { r += data[i]; g += data[i + 1]; b += data[i + 2]; }
    const n = data.length / 4;
    return [r / n, g / n, b / n];
  }

  async function load(
    ids: number[],
    arrange: { auto: boolean; preset: { layoutId: string; shape: ShapeId } | null },
    cancelled: () => boolean,
  ) {
    const session = libraryStore.session;
    loading = true;
    loadError = null;
    skipped = 0;
    layoutId = null;
    savedPath = null;
    try {
      const details = await photos.getMany(ids);
      const byId = new Map(details.map((p) => [p.id, p]));
      const stills = ids.filter((id) => byId.get(id)?.media_type === "photo").slice(0, MAX_PHOTOS);
      skipped = ids.length - stills.length;
      if (stills.length < MIN_PHOTOS) {
        loadError = "A collage needs at least two photos. Videos can't be used.";
        return;
      }
      const faces = await collageApi.faceBoxes(stills).catch(() => ({}) as Record<string, FaceBox[]>);
      const loaded = await Promise.all(
        stills.map(async (id): Promise<Item> => {
          const full = await loadImage(id);
          const preview = await makePreview(full);
          return { id, full, preview, faces: faces[String(id)] ?? [], average: averageColor(preview) };
        }),
      );
      if (cancelled() || session !== libraryStore.session) {
        for (const item of loaded) item.preview.close();
        return;
      }
      items = loaded;
      order = loaded.map((_, i) => i);
      focus = Object.fromEntries(
        loaded.map((item) => [item.id, defaultFocus(item.faces, item.preview.width / item.preview.height)]),
      );
      touched = {};
      autoMode = arrange.auto;
      if (arrange.auto) {
        // A suggestion arrives already arranged, in slot order; otherwise
        // choose here. Either way the first photo is the best one.
        const known = arrange.preset && layoutsFor(loaded.length).some((l) => l.id === arrange.preset!.layoutId);
        const chosen = known ? { ...arrange.preset!, order: order } : autoArrange(loaded.map(arrangePhoto));
        shape = chosen.shape;
        layoutId = chosen.layoutId;
        order = chosen.order;
      }
    } catch (error) {
      if (!cancelled()) loadError = `Couldn't open these photos: ${commandErrorMessage(error)}`;
    } finally {
      if (!cancelled()) loading = false;
    }
  }

  // ---- drawing -----------------------------------------------------------

  function arrangePhoto(item: Item): ArrangePhoto {
    return { aspect: item.preview.width / item.preview.height, faces: item.faces };
  }

  function slotPhotos(useFull: boolean): CollagePhoto[] {
    return order.map((i) => {
      const item = items[i];
      return useFull
        ? { source: item.full, width: item.full.naturalWidth, height: item.full.naturalHeight }
        : { source: item.preview, width: item.preview.width, height: item.preview.height };
    });
  }

  /** Crop per slot. In auto mode photos the user hasn't reframed follow
   *  their cell, so swapping or changing layout keeps faces framed. */
  function slotFocus(forLayout: Layout = layout): Focus[] {
    const framed = autoMode ? autoFocusAll(forLayout, shape, gap, order.map((i) => arrangePhoto(items[i]))) : null;
    return order.map((i, slot) => {
      const id = items[i].id;
      return framed && !touched[id] ? framed[slot] : focus[id];
    });
  }

  $effect(() => {
    const element = canvas;
    const { width, height } = view;
    // Read every input so the effect re-runs when any of them changes.
    const opts = { layout, photos: slotPhotos(false), focus: slotFocus(), gap, radius, background };
    if (!element || !width || items.length === 0) return;
    const dpr = window.devicePixelRatio || 1;
    element.width = Math.round(width * dpr);
    element.height = Math.round(height * dpr);
    const ctx = element.getContext("2d");
    if (!ctx) return;
    rects = renderCollage(ctx, element.width, element.height, opts);
    cellBoxes = rects.map((r) => ({ x: r.x / dpr, y: r.y / dpr, w: r.w / dpr, h: r.h / dpr }));
  });

  /** Small live previews in the layout strip, drawn with the real photos. */
  function layoutThumb(node: HTMLCanvasElement, l: Layout) {
    const draw = (current: Layout) => {
      const { width, height } = canvasSize(shape, 72 * (window.devicePixelRatio || 1));
      node.width = width;
      node.height = height;
      const ctx = node.getContext("2d");
      if (!ctx || items.length === 0) return;
      renderCollage(ctx, width, height, {
        layout: current,
        photos: slotPhotos(false),
        focus: slotFocus(current),
        gap: Math.max(gap, 0.03),
        radius,
        background,
      });
    };
    $effect(() => draw(l));
    return { update: (next: Layout) => draw(next) };
  }

  // ---- interaction -------------------------------------------------------

  function canvasPoint(event: PointerEvent | WheelEvent | MouseEvent) {
    const box = canvas!.getBoundingClientRect();
    return {
      x: ((event.clientX - box.left) * canvas!.width) / box.width,
      y: ((event.clientY - box.top) * canvas!.height) / box.height,
    };
  }

  function onPointerDown(event: PointerEvent) {
    if (!canvas || event.button !== 0) return;
    const p = canvasPoint(event);
    const slot = hitTest(rects, p.x, p.y);
    if (slot < 0) return;
    canvas.setPointerCapture(event.pointerId);
    dragging = true;
    const id = items[order[slot]].id;
    drag = {
      pointerId: event.pointerId,
      slot,
      lastX: p.x,
      lastY: p.y,
      startFocus: { ...slotFocus()[slot] },
      wasTouched: touched[id] === true,
      swapping: false,
    };
  }

  function onPointerMove(event: PointerEvent) {
    if (!canvas) return;
    const p = canvasPoint(event);
    const over = hitTest(rects, p.x, p.y);
    if (!drag || drag.pointerId !== event.pointerId) {
      hoverSlot = over;
      return;
    }
    const id = items[order[drag.slot]].id;
    // Leaving the cell turns the drag into a swap; the crop snaps back.
    if (!drag.swapping && over !== drag.slot) {
      drag.swapping = true;
      focus[id] = drag.startFocus;
      if (!drag.wasTouched) delete touched[id];
    }
    if (drag.swapping) {
      dragView = { origin: drag.slot, target: over === drag.slot ? -1 : over };
      return;
    }
    const r = rects[drag.slot];
    const item = items[order[drag.slot]];
    // First move of a photo that was following its cell: take over its framing.
    if (autoMode && !touched[id]) {
      focus[id] = drag.startFocus;
      touched[id] = true;
    }
    focus[id] = panFocus(focus[id], p.x - drag.lastX, p.y - drag.lastY, item.preview.width, item.preview.height, r.w, r.h);
    drag.lastX = p.x;
    drag.lastY = p.y;
  }

  function onPointerUp(event: PointerEvent) {
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (drag.swapping && dragView && dragView.target >= 0) {
      const next = [...order];
      [next[drag.slot], next[dragView.target]] = [next[dragView.target], next[drag.slot]];
      order = next;
    }
    drag = null;
    dragging = false;
    dragView = null;
  }

  function onDoubleClick(event: MouseEvent) {
    const p = canvasPoint(event);
    const slot = hitTest(rects, p.x, p.y);
    if (slot < 0) return;
    const item = items[order[slot]];
    if (autoMode) delete touched[item.id];
    else focus[item.id] = defaultFocus(item.faces, item.preview.width / item.preview.height);
  }

  // Wheel zoom needs a non-passive listener to stop the dialog scrolling.
  $effect(() => {
    const element = canvas;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      const p = canvasPoint(event);
      const slot = hitTest(rects, p.x, p.y);
      if (slot < 0) return;
      event.preventDefault();
      const id = items[order[slot]].id;
      const current = slotFocus()[slot];
      const zoom = Math.min(4, Math.max(1, current.zoom * Math.exp(-event.deltaY * 0.0015)));
      focus[id] = { ...current, zoom };
      if (autoMode) touched[id] = true;
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  });

  function shuffle() {
    if (order.length < 2) return;
    let next = [...order];
    for (let attempt = 0; attempt < 5 && next.every((v, i) => v === order[i]); attempt++) {
      next = [...order];
      for (let i = next.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [next[i], next[j]] = [next[j], next[i]];
      }
    }
    order = next;
  }

  // ---- output ------------------------------------------------------------

  function renderBlob(longSide: number, type: "image/jpeg" | "image/png"): Promise<Blob> {
    const { width, height } = canvasSize(shape, longSide);
    const out = document.createElement("canvas");
    out.width = width;
    out.height = height;
    const ctx = out.getContext("2d");
    if (!ctx) return Promise.reject(new Error("Couldn't draw the collage."));
    renderCollage(ctx, width, height, { layout, photos: slotPhotos(true), focus: slotFocus(), gap, radius, background });
    return new Promise((resolve, reject) => {
      out.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("Couldn't draw the collage."))), type, 0.92);
    });
  }

  function showFlash(text: string) {
    flash = text;
    if (flashTimer) clearTimeout(flashTimer);
    flashTimer = setTimeout(() => { flash = null; flashTimer = null; }, 2200);
  }

  async function save() {
    if (busy || items.length === 0) return;
    busy = true;
    try {
      const blob = await renderBlob(EXPORT_LONG_SIDE, "image/jpeg");
      const saved = await collageApi.save(new Uint8Array(await blob.arrayBuffer()));
      saveLocation = saved.location;
      savedPath = saved.path;
      showFlash(`Saved to ${saved.location.label}`);
    } catch (error) {
      toasts.error(commandErrorMessage(error));
    } finally {
      busy = false;
    }
  }

  function reveal() {
    if (savedPath) collageApi.reveal(savedPath).catch((error) => toasts.error(commandErrorMessage(error)));
  }

  async function copy() {
    if (busy || items.length === 0) return;
    if (typeof ClipboardItem === "undefined" || !navigator.clipboard?.write) {
      await save();
      return;
    }
    busy = true;
    try {
      // The clipboard API wants a promise of the blob, so the write starts
      // inside the click's user activation even while the PNG encodes.
      await navigator.clipboard.write([new ClipboardItem({ "image/png": renderBlob(COPY_LONG_SIDE, "image/png") })]);
      showFlash("Copied. Paste it anywhere");
    } catch (error) {
      toasts.error(commandErrorMessage(error));
    } finally {
      busy = false;
    }
  }

  /** Ctrl+S saves, Ctrl+C copies. Keys never reach the page behind. */
  function onKey(event: KeyboardEvent) {
    event.stopPropagation();
    if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.altKey) return;
    const key = event.key.toLowerCase();
    if (key === "s") { event.preventDefault(); void save(); }
    else if (key === "c" && !window.getSelection()?.toString()) { event.preventDefault(); void copy(); }
  }

  let outsidePointer: number | null = null;
  function outside(event: PointerEvent) {
    if (!dialog) return false;
    const r = dialog.getBoundingClientRect();
    return event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom;
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="collage-title"
  onpointerdown={(event) => { outsidePointer = outside(event) ? event.pointerId : null; }}
  onpointerup={(event) => { if (outsidePointer === event.pointerId && outside(event)) close(); outsidePointer = null; }}
  onpointercancel={() => { outsidePointer = null; }}
  onclose={close}
  onkeydown={onKey}
>
  <header class="head">
    <div>
      <p class="eyebrow">Collage</p>
      <h2 id="collage-title" class="display">
        {items.length || collage.ids.length} photos, one frame
      </h2>
      <p class="sub">
        {autoMode ? "Arranged for you. Adjust anything right in the preview." : "Adjust the photos right in the preview."}
        {#if skipped > 0}<span class="skipped">{skipped} {skipped === 1 ? "video" : "videos"} left out.</span>{/if}
      </p>
    </div>
    <button class="ghost icon" onclick={close} aria-label="Close" title="Close (Esc)">
      <span class="x" aria-hidden="true">×</span>
    </button>
  </header>

  <div class="body">
    <div class="stage" bind:clientWidth={stageWidth} bind:clientHeight={stageHeight}>
      {#if loadError}
        <p class="state">{loadError}</p>
      {:else if loading || items.length === 0}
        <div class="state loading" style:aspect-ratio={ratio}>Preparing photos…</div>
      {:else}
        <div class="frame" style:width="{view.width}px" style:height="{view.height}px">
          <canvas
            bind:this={canvas}
            class:dragging
            style:cursor={hoverSlot >= 0 ? "grab" : "default"}
            aria-label="Collage preview"
            onpointerdown={onPointerDown}
            onpointermove={onPointerMove}
            onpointerup={onPointerUp}
            onpointercancel={onPointerUp}
            onpointerleave={() => { if (!drag) hoverSlot = -1; }}
            ondblclick={onDoubleClick}
          ></canvas>
          <div class="hint" class:show={hoverSlot >= 0 || dragView != null} aria-hidden="true">
            {#if dragView}
              {dragView.target >= 0 ? "Release to swap" : "Drop on another photo to swap"}
            {:else}
              Drag to reframe · Drag onto another to swap · Scroll to zoom
            {/if}
          </div>
          {#if dragView}
            {@const o = cellBoxes[dragView.origin]}
            {#if o}<div class="cell origin" style:left="{o.x}px" style:top="{o.y}px" style:width="{o.w}px" style:height="{o.h}px"></div>{/if}
            {#if dragView.target >= 0}
              {@const t = cellBoxes[dragView.target]}
              {#if t}<div class="cell target" style:left="{t.x}px" style:top="{t.y}px" style:width="{t.w}px" style:height="{t.h}px"></div>{/if}
            {/if}
          {/if}
        </div>
      {/if}
    </div>

    <aside class="controls" aria-label="Collage options">
      <fieldset>
        <legend>Shape</legend>
        <div class="shapes">
          {#each SHAPES as s (s.id)}
            <button type="button" class="shape" class:on={shape === s.id} aria-pressed={shape === s.id} onclick={() => (shape = s.id)}>
              <span class="glyph" style:aspect-ratio={s.ratio}></span>
              <span class="mono">{s.label}</span>
            </button>
          {/each}
        </div>
      </fieldset>

      <label class="slider">
        <span class="legend">Spacing</span>
        <input type="range" min="0" max="0.045" step="0.0025" bind:value={gap} />
      </label>
      <label class="slider">
        <span class="legend">Corners</span>
        <input type="range" min="0" max="0.06" step="0.0025" bind:value={radius} />
      </label>

      <fieldset>
        <legend>Background</legend>
        <div class="swatches">
          {#each BORDERS as b (b.id)}
            <button type="button" class="swatch" class:on={border === b.id} aria-pressed={border === b.id}
              title={b.id === "match" ? "Match the photos" : b.label} onclick={() => (border = b.id)}>
              <span class="dot" class:match={b.id === "match"}
                style:background={b.id === "match" ? matchColor(items.map((item) => item.average)) : b.color}></span>
              <span>{b.label}</span>
            </button>
          {/each}
        </div>
      </fieldset>

      <button class="ghost shuffle" onclick={shuffle} disabled={items.length < 2}>
        <Shuffle size={14} strokeWidth={1.8} /> Shuffle photos
      </button>
    </aside>
  </div>

  <footer class="foot">
    <div class="layouts" role="radiogroup" aria-label="Layout">
      {#each layouts as l (l.id)}
        <button type="button" role="radio" class="layout" class:on={layout.id === l.id} aria-checked={layout.id === l.id}
          title={l.name} onclick={() => (layoutId = l.id)} disabled={items.length === 0}>
          <canvas use:layoutThumb={l} aria-hidden="true"></canvas>
          <span>{l.name}</span>
        </button>
      {/each}
    </div>
    <div class="actions">
      <div class="flash" aria-live="polite">
        {#if flash}
          <span class="pill"><Check size={13} strokeWidth={2.2} />{flash}</span>
        {:else if savedPath}
          <button class="ghost reveal" onclick={reveal}><FolderOpen size={14} strokeWidth={1.8} /> Show in folder</button>
        {:else if saveLocation}
          <span class="where" title={saveLocation.path}>Saves to {saveLocation.label}</span>
        {/if}
      </div>
      <button class="ghost" onclick={copy} disabled={busy || items.length === 0} title="Copy (Ctrl+C)">
        <Copy size={15} strokeWidth={1.8} /> Copy
      </button>
      <button class="primary" onclick={save} disabled={busy || items.length === 0} title="Save (Ctrl+S)">
        <Download size={15} strokeWidth={1.8} /> {busy ? "Working…" : "Save"}
      </button>
    </div>
  </footer>
</dialog>

<style>
  dialog {
    margin: auto;
    width: min(1060px, calc(100vw - 32px));
    height: min(780px, calc(100vh - 32px));
    padding: var(--s-5);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--bg-paper);
    color: var(--ink);
    box-shadow: 0 24px 60px rgb(0 0 0 / 38%);
    animation: dialog-in var(--t-base-d) var(--ease-out);
  }
  dialog[open] {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: var(--s-4);
  }
  dialog::backdrop { background: rgb(0 0 0 / 60%); }

  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--s-3); }
  h2 { margin: 0; font-size: var(--t-xl); }
  .eyebrow {
    margin: 0 0 4px;
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }
  .sub { margin: 4px 0 0; font-size: var(--t-xs); color: var(--ink-muted); }
  .skipped { margin-left: 6px; color: var(--ink-soft); }
  .icon { width: 28px; height: 28px; padding: 0; border: 0; display: grid; place-items: center; color: var(--ink-muted); }
  .x { font-size: 20px; line-height: 1; }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 196px;
    gap: var(--s-5);
    min-height: 0;
  }
  .stage {
    position: relative;
    min-height: 0;
    display: grid;
    place-items: center;
    border-radius: var(--r-md);
    background:
      radial-gradient(circle at 50% 40%, var(--bg-card), transparent 70%),
      var(--bg);
    overflow: hidden;
  }
  .frame {
    position: relative;
    box-shadow: 0 18px 40px rgb(0 0 0 / 35%), 0 2px 6px rgb(0 0 0 / 25%);
  }
  .frame canvas { display: block; width: 100%; height: 100%; touch-action: none; }
  .frame canvas.dragging { cursor: grabbing !important; }
  .cell { position: absolute; pointer-events: none; border-radius: 2px; }
  .cell.origin { background: rgb(0 0 0 / 35%); }
  .cell.target { outline: 3px solid var(--accent); outline-offset: -3px; background: var(--accent-ghost); }
  .hint {
    position: absolute;
    left: 50%;
    bottom: 12px;
    transform: translate(-50%, 4px);
    padding: 5px 12px;
    border-radius: 999px;
    background: rgb(15 16 21 / 78%);
    color: #e7eaf0;
    font-size: 11px;
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    backdrop-filter: blur(6px);
    transition: opacity var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .hint.show { opacity: 1; transform: translate(-50%, 0); }
  .state { margin: 0; color: var(--ink-muted); font-size: var(--t-sm); text-align: center; padding: var(--s-4); }
  .state.loading {
    height: 60%;
    display: grid;
    place-items: center;
    border-radius: var(--r-md);
    background: var(--bg-card);
    animation: breathe 1.6s var(--ease) infinite alternate;
  }

  .controls { display: flex; flex-direction: column; gap: var(--s-4); min-height: 0; min-width: 0; overflow: hidden auto; }
  fieldset { margin: 0; padding: 0; border: 0; min-width: 0; }
  legend, .legend {
    display: block;
    margin-bottom: 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .shapes { display: grid; grid-template-columns: repeat(4, 1fr); gap: 4px; }
  .shape {
    display: grid;
    justify-items: center;
    align-content: end;
    gap: 6px;
    height: 64px;
    padding: 8px 2px 6px;
    font-size: 10px;
    background: transparent;
    border: 1px solid var(--line);
    color: var(--ink-soft);
  }
  .glyph { display: block; height: 26px; max-width: 30px; border: 1.5px solid currentColor; border-radius: 2px; }
  .shape.on, .swatch.on, .layout.on { border-color: var(--accent); color: var(--ink); background: var(--accent-ghost); }
  .slider { display: block; }
  .slider input { width: 100%; accent-color: var(--accent); }
  .swatches { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }
  .swatch {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    font-size: var(--t-xs);
    background: transparent;
    border: 1px solid var(--line);
    color: var(--ink-soft);
  }
  .dot { width: 14px; height: 14px; border-radius: 50%; box-shadow: inset 0 0 0 1px rgb(127 127 127 / 45%); }
  .dot.match { box-shadow: inset 0 0 0 1px rgb(127 127 127 / 45%), 0 0 0 2px var(--bg-paper), 0 0 0 3px var(--accent-soft); }
  .shuffle { display: inline-flex; align-items: center; justify-content: center; gap: 6px; border: 1px solid var(--line); }

  .foot { display: flex; align-items: flex-end; gap: var(--s-4); }
  .layouts { flex: 1; display: flex; gap: 6px; overflow-x: auto; padding-bottom: 2px; }
  .layout {
    flex: none;
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 8px 8px 6px;
    min-width: 92px;
    font-size: 10px;
    background: transparent;
    border: 1px solid var(--line);
    color: var(--ink-soft);
  }
  .layout canvas { display: block; height: 56px; width: auto; max-width: 84px; object-fit: contain; border-radius: 2px; }
  .actions { display: flex; align-items: center; gap: var(--s-2); }
  .actions button { display: inline-flex; align-items: center; gap: 6px; }
  .flash { min-width: 0; }
  .where { font-size: var(--t-xs); color: var(--ink-muted); white-space: nowrap; }
  .reveal { display: inline-flex; align-items: center; gap: 6px; font-size: var(--t-xs); }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    font-size: var(--t-xs);
    color: var(--ink-soft);
    background: var(--accent-ghost);
    border: 1px solid var(--accent-soft);
    border-radius: 999px;
    white-space: nowrap;
    animation: flash-in var(--t-base-d) var(--ease-out);
  }

  @media (max-width: 760px) {
    .body { grid-template-columns: 1fr; grid-template-rows: minmax(0, 1fr) auto; }
    .controls { flex-direction: row; flex-wrap: wrap; }
    .foot { flex-direction: column; align-items: stretch; }
  }
  @keyframes dialog-in {
    from { opacity: 0; transform: translateY(10px) scale(0.985); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes flash-in {
    from { opacity: 0; transform: translateY(4px) scale(0.96); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes breathe { from { opacity: 0.55; } to { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) {
    dialog, .pill, .state.loading { animation: none; }
  }
</style>
