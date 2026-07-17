<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { bursts } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
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
  let actionBusy = $state(false);
  let loadSeq = 0;
  let mounted = true;

  async function load() {
    const seq = ++loadSeq;
    const groupId = id;
    error = null;
    group = null;
    summaries = new Map();
    try {
      const nextGroup = await bursts.getGroup(groupId);
      const ids = nextGroup.members.map(m => m.photo_id);
      const full = await photosApi.getMany(ids);
      if (!mounted || seq !== loadSeq) return;
      const m = new Map<number, PhotoSummaryDto>();
      for (const p of full) m.set(p.id, p);
      group = nextGroup;
      summaries = m;
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function setBest(photoId: number) {
    if (actionBusy) return;
    const seq = loadSeq;
    const groupId = id;
    try {
      actionBusy = true;
      const nextGroup = await bursts.setBest(groupId, photoId);
      if (!mounted || seq !== loadSeq || groupId !== id) return;
      group = nextGroup;
    }
    catch (e) { if (mounted && seq === loadSeq && groupId === id) error = commandErrorMessage(e); }
    finally { if (mounted && seq === loadSeq && groupId === id) actionBusy = false; }
  }

  function patchThumbnail(photoId: number, thumbnailPath: string) {
    const current = summaries.get(photoId);
    if (!current) return;
    const next = new Map(summaries);
    next.set(photoId, { ...current, thumbnail_path: thumbnailPath });
    summaries = next;
  }

  async function trashRest() {
    if (!group || actionBusy) return;
    if (!confirm("Trash all non-best?")) return;
    const seq = loadSeq;
    const groupId = id;
    const trashedIds = group.members
      .filter((m) => !m.is_suggested_best)
      .map((m) => m.photo_id);
    try {
      actionBusy = true;
      await bursts.trashNonBest(groupId);
      if (!mounted || seq !== loadSeq || groupId !== id) return;
      photoVisibility.markTrashed(trashedIds);
      browseContext.remove(trashedIds);
      window.location.hash = "/bursts";
    } catch (e) {
      if (mounted && seq === loadSeq && groupId === id) error = commandErrorMessage(e);
    } finally {
      if (mounted && seq === loadSeq && groupId === id) actionBusy = false;
    }
  }

  onMount(() => {
    mounted = true;
    return () => {
      mounted = false;
      loadSeq += 1;
    };
  });

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
      <button class="danger" onclick={trashRest} disabled={actionBusy}>
        Trash {group.members.length - 1} other{group.members.length - 1 === 1 ? "" : "s"}
      </button>
    {/if}
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

{#if group}
  {@const memberIds = group.members.map((m) => m.photo_id)}
  <div class="grid">
    {#each group.members as m (m.photo_id)}
      {@const summary = summaries.get(m.photo_id)}
      <div class="card" class:best={m.is_suggested_best}>
        <a
          class="frame"
          href="#/photo?id={m.photo_id}"
          aria-label="Open photo"
          onclick={() => browseContext.set(`burst:${id}`, memberIds)}
          use:thumbnailOnVisible={{
            id: m.photo_id,
            thumbnailPath: summary?.thumbnail_path ?? null,
            mediaType: summary?.media_type,
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
            <a
              class="open-link"
              href="#/photo?id={m.photo_id}"
              onclick={() => browseContext.set(`burst:${id}`, memberIds)}
            >Open</a>
          {:else}
            <a
              class="open-link"
              href="#/photo?id={m.photo_id}"
              onclick={() => browseContext.set(`burst:${id}`, memberIds)}
            >Open</a>
            <button class="pick" onclick={() => setBest(m.photo_id)} disabled={actionBusy}>Pick this</button>
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
    min-height: 0;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--s-3);
    align-content: start;
    grid-auto-rows: max-content;
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
