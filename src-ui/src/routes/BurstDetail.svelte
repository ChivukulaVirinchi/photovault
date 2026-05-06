<script lang="ts">
  import { bursts } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { photos as photosApi } from "../lib/api/photos";
  import { thumbUrl } from "../lib/thumbnail";
  import type { BurstMember } from "../lib/api/all";
  import type { PhotoSummaryDto } from "../lib/api/types";

  interface Props { id: number }
  let { id }: Props = $props();

  let group = $state<{ id: number; members: BurstMember[] } | null>(null);
  let summaries = $state<Map<number, PhotoSummaryDto>>(new Map());
  let error = $state<string | null>(null);

  async function load() {
    try {
      group = await bursts.getGroup(id);
      const ids = group.members.map(m => m.photo_id);
      const full = await photosApi.getMany(ids);
      const m = new Map<number, PhotoSummaryDto>();
      for (const p of full) m.set(p.id, p);
      summaries = m;
    } catch (e) { error = JSON.stringify(e); }
  }

  async function setBest(photoId: number) {
    try { group = await bursts.setBest(id, photoId); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function trashRest() {
    if (!confirm("Trash all non-best?")) return;
    await bursts.trashNonBest(id);
    window.location.hash = "/bursts";
  }

  $effect(() => { void id; load(); });
</script>

<div class="masthead">
  <a class="back" href="#/bursts">← Bursts</a>
  <span class="eyebrow">
    <span class="num">№&nbsp;{String(id).padStart(3, "0")}</span>
    <span class="ornament"></span>
    <span>BURST</span>
  </span>
  <h1>Pick the sharpest.</h1>
  {#if group}
    <p class="subtitle">{group.members.length} shots. The gold border marks our pick — change it if you disagree.</p>
    <div class="row">
      <button class="danger" onclick={trashRest}>Trash {group.members.length - 1} others</button>
    </div>
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if group}
  <div class="grid stagger">
    {#each group.members as m, i (m.photo_id)}
      {@const summary = summaries.get(m.photo_id)}
      <div class="card" class:best={m.is_suggested_best} style="--i: {i}">
        <div class="frame">
          {#if summary?.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, summary.thumbnail_path) ?? ""} alt="" />
          {/if}
          {#if m.is_suggested_best}<span class="badge mono">BEST</span>{/if}
        </div>
        <div class="meta">
          <span class="muted small mono">sharpness {m.sharpness_score?.toFixed(2) ?? "—"}</span>
          {#if !m.is_suggested_best}
            <button onclick={() => setBest(m.photo_id)}>Pick this</button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .masthead {
    padding: var(--s-7) var(--s-7) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: flex-start;
  }
  .back {
    font-family: var(--font-mono);
    font-size: var(--t-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-muted);
  }
  h1 { font-size: var(--t-3xl); }
  .row { display: flex; gap: var(--s-2); }

  .grid {
    padding: var(--s-5) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--s-3);
  }
  .card {
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    transition: border-color var(--t-fast) var(--ease);
  }
  .card.best {
    border-color: var(--keep);
    box-shadow: 0 0 0 1px var(--keep) inset;
  }
  .frame { aspect-ratio: 1; background: #000; position: relative; }
  .frame img { width: 100%; height: 100%; object-fit: cover; }
  .badge {
    position: absolute;
    top: var(--s-2); right: var(--s-2);
    background: var(--keep);
    color: var(--bg);
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 10px;
    letter-spacing: 0.18em;
    font-weight: 600;
  }
  .meta {
    padding: var(--s-3);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-2);
  }
  .small { font-size: var(--t-xs); }
</style>
