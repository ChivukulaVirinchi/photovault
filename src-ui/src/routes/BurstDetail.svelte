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
  let photoSummaries = $state<Map<number, PhotoSummaryDto>>(new Map());
  let error = $state<string | null>(null);

  async function load() {
    try {
      group = await bursts.getGroup(id);
      const ids = group.members.map(m => m.photo_id);
      const full = await photosApi.getMany(ids);
      const m = new Map<number, PhotoSummaryDto>();
      for (const p of full) m.set(p.id, p);
      photoSummaries = m;
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

<main class="detail">
  <header>
    <a href="#/bursts">← Bursts</a>
    <h2>Burst #{id}</h2>
    {#if group}
      <button class="danger" onclick={trashRest}>Trash {group.members.length - 1} others</button>
    {/if}
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if group}
    <div class="row">
      {#each group.members as m (m.photo_id)}
        {@const summary = photoSummaries.get(m.photo_id)}
        <div class="card" class:best={m.is_suggested_best}>
          {#if summary?.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, summary.thumbnail_path) ?? ""} alt="" />
          {/if}
          <div class="meta">
            <span class="muted small">sharpness {m.sharpness_score?.toFixed(2) ?? "?"}</span>
            {#if m.is_suggested_best}
              <strong>Best</strong>
            {:else}
              <button onclick={() => setBest(m.photo_id)}>Pick best</button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</main>

<style>
  .detail { flex: 1; overflow-y: auto; padding: 20px; }
  header { display: flex; gap: 14px; align-items: center; margin-bottom: 16px; flex-wrap: wrap; }
  h2 { margin: 0; }
  .danger { background: #2a1414; border: 1px solid #4a2222; color: #f87171; }
  .row { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 8px; }
  .card { background: #131316; border-radius: 8px; overflow: hidden; border: 2px solid transparent; }
  .card.best { border-color: #fbbf24; }
  .card img { width: 100%; aspect-ratio: 1; object-fit: cover; }
  .meta { padding: 8px 10px; display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .small { font-size: 11px; }
</style>
