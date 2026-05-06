<script lang="ts">
  import { onMount } from "svelte";
  import { photos } from "../lib/api/photos";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { PhotoDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let photo = $state<PhotoDto | null>(null);
  let imageUrl = $state<string | null>(null);
  let error = $state<string | null>(null);
  let metaOpen = $state(true);

  function back() { history.back(); }

  async function load() {
    error = null; imageUrl = null;
    try {
      photo = await photos.get(id);
      try {
        const { absolute_path } = await library.resolvePath(id);
        imageUrl = convertFileSrc(absolute_path);
      } catch {}
    } catch (e) { error = JSON.stringify(e); }
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") back();
      if (e.key === "i" || e.key === "I") metaOpen = !metaOpen;
    };
    window.addEventListener("keydown", onKey);
    load();
    return () => window.removeEventListener("keydown", onKey);
  });

  $effect(() => { void id; load(); });

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

<main class="detail">
  <header class="bar">
    <button class="ghost" onclick={back}>← Back</button>
    <span class="filename mono">
      {photo?.file_name ?? "loading..."}
    </span>
    <button class="ghost" onclick={() => (metaOpen = !metaOpen)}>
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

        <hr class="rule" />

        <dl class="specs">
          {#if photo.camera}
            <dt>Camera</dt>
            <dd>{photo.camera.make ?? ""} {photo.camera.model ?? ""}</dd>
            {#if photo.camera.lens}
              <dt>Lens</dt><dd>{photo.camera.lens}</dd>
            {/if}
            {#if photo.camera.iso}
              <dt>ISO</dt><dd class="mono">{photo.camera.iso}</dd>
            {/if}
            {#if photo.camera.aperture}
              <dt>Aperture</dt><dd class="mono">{photo.camera.aperture}</dd>
            {/if}
            {#if photo.camera.shutter_speed}
              <dt>Shutter</dt><dd class="mono">{photo.camera.shutter_speed}</dd>
            {/if}
            {#if photo.camera.focal_length}
              <dt>Focal</dt><dd class="mono">{photo.camera.focal_length}</dd>
            {/if}
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
          <span class="eyebrow">
            <span class="ornament"></span>
            <span>TRANSCRIBED TEXT</span>
          </span>
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
    grid-template-columns: 1fr 360px;
    overflow: hidden;
  }

  .viewer {
    background: #08060490;
    background-image: radial-gradient(
      ellipse at center,
      transparent 0%,
      rgba(0, 0, 0, 0.35) 100%
    );
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--s-5);
    overflow: hidden;
  }
  .viewer img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    box-shadow: 0 30px 60px rgba(0,0,0,0.5);
    animation: rise var(--t-slow) var(--ease-out);
  }

  .meta {
    background: var(--bg-paper);
    border-left: 1px solid var(--line-soft);
    padding: var(--s-6) var(--s-5);
    overflow-y: auto;
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
    font-variation-settings: "opsz" 14, "SOFT" 60;
  }
  .where em {
    font-style: italic;
    color: var(--accent);
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
    font-variation-settings: "opsz" 14, "SOFT" 100;
    font-size: var(--t-sm);
    background: var(--bg-card);
    padding: var(--s-4);
    border-radius: var(--r-md);
    border-left: 2px solid var(--accent);
    color: var(--ink-soft);
    line-height: 1.6;
  }

  .loading {
    font-size: var(--t-sm);
    letter-spacing: 0.08em;
  }

  @media (max-width: 920px) {
    .viewer-row { grid-template-columns: 1fr; grid-template-rows: 1fr auto; }
    .meta { border-left: none; border-top: 1px solid var(--line-soft); max-height: 40vh; }
  }
</style>
