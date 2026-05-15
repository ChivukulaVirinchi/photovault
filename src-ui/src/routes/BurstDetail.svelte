<script lang="ts">
  import { bursts } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { photos as photosApi } from "../lib/api/photos";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
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

  function patchThumbnail(photoId: number, thumbnailPath: string) {
    const current = summaries.get(photoId);
    if (!current) return;
    const next = new Map(summaries);
    next.set(photoId, { ...current, thumbnail_path: thumbnailPath });
    summaries = next;
  }

  async function trashRest() {
    if (!confirm("Trash all non-best?")) return;
    await bursts.trashNonBest(id);
    window.location.hash = "/bursts";
  }

  $effect(() => { void id; load(); });
</script>

<DetailHeader backHref="#/bursts" backLabel="Bursts">
  {#snippet title()}
    <h1>Pick the sharpest</h1>
  {/snippet}
  {#snippet subtitle()}
    {#if group}
      <span class="mono">{group.members.length} shots</span>
      <span class="hint">The bordered card is our pick — change it if you disagree.</span>
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if group && group.members.length > 1}
      <button class="danger" onclick={trashRest}>
        Trash {group.members.length - 1} other{group.members.length - 1 === 1 ? "" : "s"}
      </button>
    {/if}
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

{#if group}
  <div class="grid">
    {#each group.members as m (m.photo_id)}
      {@const summary = summaries.get(m.photo_id)}
      <div class="card" class:best={m.is_suggested_best}>
        <a
          class="frame"
          href="#/photo?id={m.photo_id}"
          aria-label="Open photo"
          use:thumbnailOnVisible={{
            id: m.photo_id,
            thumbnailPath: summary?.thumbnail_path ?? null,
            onReady: (path) => patchThumbnail(m.photo_id, path),
          }}
        >
          {#if summary?.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, summary.thumbnail_path) ?? ""} alt="" />
          {/if}
          {#if m.is_suggested_best}<span class="badge">Best</span>{/if}
        </a>
        <div class="meta">
          {#if m.is_suggested_best}
            <a class="open-link" href="#/photo?id={m.photo_id}">Open</a>
          {:else}
            <a class="open-link" href="#/photo?id={m.photo_id}">Open</a>
            <button class="pick" onclick={() => setBest(m.photo_id)}>Pick this</button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .hint {
    color: var(--ink-muted);
    font-style: italic;
  }
  .grid {
    padding: var(--s-4) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
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
    box-shadow:
      0 0 0 2px var(--keep) inset,
      0 4px 16px color-mix(in oklab, var(--keep) 35%, transparent);
  }
  .frame {
    aspect-ratio: 4 / 3;
    background: var(--bg-elev);
    position: relative;
    display: block;
    text-decoration: none;
    color: inherit;
  }
  .frame img { width: 100%; height: 100%; object-fit: cover; }
  .badge {
    position: absolute;
    top: var(--s-2);
    right: var(--s-2);
    background: var(--keep);
    color: #fff;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .meta {
    padding: var(--s-2) var(--s-3);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-2);
    font-size: var(--t-xs);
  }
  .open-link {
    color: var(--ink-muted);
    text-decoration: none;
    font-size: var(--t-xs);
  }
  .open-link:hover { color: var(--accent); }
  .pick {
    font-size: var(--t-xs);
    padding: 4px 10px;
  }
</style>
