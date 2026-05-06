<script lang="ts">
  import { onMount } from "svelte";
  import { photos } from "../lib/api/photos";
  import { library } from "../lib/api/library";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { PhotoDto } from "../lib/api/types";

  interface Props {
    id: number;
  }
  let { id }: Props = $props();

  let photo = $state<PhotoDto | null>(null);
  let imageUrl = $state<string | null>(null);
  let error = $state<string | null>(null);

  function back() {
    history.back();
  }

  async function load() {
    error = null;
    try {
      photo = await photos.get(id);
      try {
        const { absolute_path } = await library.resolvePath(id);
        imageUrl = convertFileSrc(absolute_path);
      } catch {
        imageUrl = null;
      }
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") back();
    };
    window.addEventListener("keydown", onKey);
    load();
    return () => window.removeEventListener("keydown", onKey);
  });

  $effect(() => {
    // Re-load when id changes.
    void id;
    load();
  });
</script>

<main class="detail">
  <header>
    <button onclick={back} aria-label="Back">← Back</button>
    {#if photo}
      <span class="muted">
        {photo.file_name}
        {#if photo.date_taken}— {new Date(photo.date_taken).toLocaleString()}{/if}
      </span>
    {/if}
  </header>

  <section class="viewer">
    {#if error}
      <p class="error">{error}</p>
    {:else if photo && imageUrl}
      <img src={imageUrl} alt={photo.file_name} />
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </section>

  {#if photo}
    <aside class="meta">
      {#if photo.location}
        <p>
          {photo.location.city ?? "?"}, {photo.location.country ?? "?"}
        </p>
      {/if}
      {#if photo.camera}
        <p class="muted small">
          {photo.camera.make ?? ""} {photo.camera.model ?? ""}
          {#if photo.camera.iso}— ISO {photo.camera.iso}{/if}
          {#if photo.camera.aperture}— {photo.camera.aperture}{/if}
        </p>
      {/if}
      <p class="muted small">{photo.width}×{photo.height} • {(photo.file_size / 1024 / 1024).toFixed(1)} MB</p>
    </aside>
  {/if}
</main>

<style>
  .detail {
    height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
  }
  header {
    display: flex;
    gap: 16px;
    align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid #1f1f22;
  }
  .viewer {
    overflow: hidden;
    background: #000;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .viewer img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .meta {
    padding: 12px 20px;
    border-top: 1px solid #1f1f22;
  }
  .small {
    font-size: 12px;
  }
</style>
