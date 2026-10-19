<script lang="ts">
  import { countOf } from "../format";
  import X from "lucide-svelte/icons/x";
  import type { CollageSuggestion, CollageSuggestionPhoto } from "../api/all";
  import {
    AUTO_GAP,
    SHAPES,
    autoArrange,
    autoFocusAll,
    layoutsFor,
    renderCollage,
    type ArrangePhoto,
    type ShapeId,
  } from "../collage";
  import { libraryStore } from "../stores/library.svelte";
  import { thumbUrl } from "../thumbnail";

  interface Props {
    suggestion: CollageSuggestion;
    /** Open the collage editor with the photos in slot order. */
    onopen: (ids: number[], layoutId: string, shape: ShapeId) => void;
    ondismiss: () => void;
  }
  let { suggestion, onopen, ondismiss }: Props = $props();

  /** The collage sits on the card like a print, with room around it. */
  const PRINT_SCALE = 0.84;

  /** Display aspect from stored (sensor) dimensions and EXIF orientation. */
  function aspectOf(p: CollageSuggestionPhoto): number {
    if (!p.width || !p.height) return 4 / 3;
    return p.orientation >= 5 && p.orientation <= 8 ? p.height / p.width : p.width / p.height;
  }

  const arrangement = $derived(
    autoArrange(suggestion.photos.map((p) => ({ aspect: aspectOf(p), faces: p.faces }))),
  );
  const slotted = $derived(arrangement.order.map((i) => suggestion.photos[i]));
  const layout = $derived(layoutsFor(slotted.length).find((l) => l.id === arrangement.layoutId) ?? layoutsFor(slotted.length)[0]);
  const ratio = $derived(SHAPES.find((s) => s.id === arrangement.shape)?.ratio ?? 1);

  let canvas = $state<HTMLCanvasElement | undefined>();
  let stageWidth = $state(0);
  let images = $state.raw<HTMLImageElement[] | null>(null);
  let failed = $state(false);

  const frame = $derived.by(() => {
    const box = stageWidth * PRINT_SCALE;
    return ratio >= 1 ? { width: box, height: box / ratio } : { width: box * ratio, height: box };
  });

  $effect(() => {
    const root = libraryStore.driveRoot;
    const photos = slotted;
    let cancelled = false;
    images = null;
    failed = false;
    Promise.all(
      photos.map(async (p) => {
        const src = p.thumbnail_path ? thumbUrl(root, p.thumbnail_path) : null;
        if (!src) throw new Error("no thumbnail");
        const img = new Image();
        img.decoding = "async";
        img.src = src;
        await img.decode();
        return img;
      }),
    ).then(
      (loaded) => { if (!cancelled) images = loaded; },
      () => { if (!cancelled) failed = true; },
    );
    return () => { cancelled = true; };
  });

  $effect(() => {
    const element = canvas;
    const loaded = images;
    const { width, height } = frame;
    if (!element || !loaded || !width) return;
    const dpr = window.devicePixelRatio || 1;
    element.width = Math.round(width * dpr);
    element.height = Math.round(height * dpr);
    const ctx = element.getContext("2d");
    if (!ctx) return;
    const photos: ArrangePhoto[] = loaded.map((img, slot) => ({
      aspect: img.naturalWidth / img.naturalHeight,
      faces: slotted[slot].faces,
    }));
    renderCollage(ctx, element.width, element.height, {
      layout,
      photos: loaded.map((img) => ({ source: img, width: img.naturalWidth, height: img.naturalHeight })),
      focus: autoFocusAll(layout, arrangement.shape, AUTO_GAP, photos),
      gap: AUTO_GAP,
      radius: 0,
      background: "#ffffff",
    });
  });

  function open() {
    onopen(slotted.map((p) => p.id), layout.id, arrangement.shape);
  }
</script>

{#if !failed}
  <article class="card">
    <button class="open" onclick={open} disabled={!images} aria-label="Open collage: {suggestion.title}">
      <div class="stage" bind:clientWidth={stageWidth}>
        {#if images}
          <canvas bind:this={canvas} style:width="{frame.width}px" style:height="{frame.height}px" aria-hidden="true"></canvas>
        {:else}
          <div class="placeholder" style:width="{frame.width}px" style:height="{frame.height}px"></div>
        {/if}
      </div>
      <div class="caption">
        <span class="eyebrow mono">{suggestion.eyebrow}</span>
        <span class="title">{suggestion.title}</span>
        <span class="count mono">{countOf(suggestion.photos.length, "photo")}</span>
      </div>
    </button>
    <button class="dismiss" onclick={ondismiss} title="Not this one" aria-label="Don't suggest this collage again">
      <X size={14} strokeWidth={2} />
    </button>
  </article>
{/if}

<style>
  .card { position: relative; min-width: 0; }
  .open {
    display: grid;
    width: 100%;
    padding: 0;
    gap: var(--s-3);
    text-align: left;
    color: inherit;
    background: transparent;
    border: 0;
    cursor: pointer;
  }
  .open:disabled { cursor: progress; }
  .stage {
    aspect-ratio: 1;
    display: grid;
    place-items: center;
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background:
      radial-gradient(circle at 50% 35%, var(--bg-card), transparent 75%),
      var(--bg);
    overflow: hidden;
    transition: box-shadow var(--t-base-d) var(--ease);
  }
  .open:hover .stage, .open:focus-visible .stage { box-shadow: 0 0 0 2px var(--accent-ghost); }
  canvas, .placeholder {
    display: block;
    box-shadow: 0 14px 30px rgb(0 0 0 / 35%), 0 2px 6px rgb(0 0 0 / 22%);
    transition: transform var(--t-slow) var(--ease);
  }
  .open:hover canvas { transform: rotate(-0.6deg) scale(1.015); }
  .placeholder { background: var(--bg-card); animation: breathe 1.6s var(--ease) infinite alternate; }
  .caption { display: grid; gap: 3px; padding: 0 2px; }
  .eyebrow {
    font-size: var(--t-xs);
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-weight: 600;
  }
  .title {
    font-family: var(--font-display);
    font-size: var(--t-base);
    line-height: 1.2;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count { font-size: var(--t-xs); color: var(--ink-muted); }
  .dismiss {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 26px;
    height: 26px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 0;
    color: #e7eaf0;
    background: rgb(15 16 21 / 62%);
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease);
  }
  .card:hover .dismiss, .dismiss:focus-visible { opacity: 1; }
  @keyframes breathe { from { opacity: 0.55; } to { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) {
    canvas, .placeholder { transition: none; animation: none; }
    .open:hover canvas { transform: none; }
  }
</style>
