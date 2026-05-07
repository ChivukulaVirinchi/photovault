<script lang="ts">
  import { duplicates } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import type { DupMember } from "../lib/api/all";

  interface Props { id: number }
  let { id }: Props = $props();

  let group = $state<{ id: number; members: DupMember[] } | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try { group = await duplicates.getGroup(id); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function setKeep(photoId: number) {
    try { group = await duplicates.setKeep(id, photoId); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function trashOthers() {
    if (!confirm("Trash all non-keep duplicates?")) return;
    await duplicates.trashOthers(id);
    window.location.hash = "/duplicates";
  }

  async function dismiss() {
    await duplicates.dismiss(id);
    window.location.hash = "/duplicates";
  }

  $effect(() => { void id; load(); });

  function fmtSize(b: number | null): string {
    if (b === null) return "—";
    return (b / 1024 / 1024).toFixed(1) + " MB";
  }
</script>

<DetailHeader backHref="#/duplicates" backLabel="Duplicates">
  {#snippet title()}
    <h1>The same photograph</h1>
  {/snippet}
  {#snippet subtitle()}
    {#if group}
      <span class="mono">{group.members.length} copies</span>
      <span class="hint">Pick one to keep — trash the rest.</span>
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if group && group.members.length > 1}
      <button class="ghost" onclick={dismiss}>Dismiss</button>
      <button class="danger" onclick={trashOthers}>
        Trash {group.members.length - 1} other{group.members.length - 1 === 1 ? "" : "s"}
      </button>
    {/if}
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

{#if group}
  <div class="grid">
    {#each group.members as m (m.photo_id)}
      <div class="card" class:keep={m.is_suggested_keep}>
        <a class="frame" href="#/photo?id={m.photo_id}" aria-label="Open photo">
          {#if m.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, m.thumbnail_path) ?? ""} alt="" />
          {/if}
          {#if m.is_suggested_keep}<span class="badge">Keep</span>{/if}
        </a>
        <dl>
          <dt>Size</dt><dd class="mono">{fmtSize(m.file_size)}</dd>
          <dt>Date</dt><dd class="mono">{m.date_taken ?? "—"}</dd>
          <dt>Path</dt><dd class="mono path" title={m.file_path ?? ""}>{m.file_path ?? "—"}</dd>
        </dl>
        <div class="actions">
          {#if m.is_suggested_keep}
            <button class="primary keep-btn" disabled>Keeping</button>
          {:else}
            <button class="primary keep-btn" onclick={() => setKeep(m.photo_id)}>Keep this</button>
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
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: var(--s-4);
  }
  .card {
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    transition: border-color var(--t-fast) var(--ease);
  }
  .card.keep {
    border-color: var(--keep);
    box-shadow: 0 0 0 1px var(--keep) inset;
  }
  .frame {
    aspect-ratio: 4 / 3;
    background: var(--bg-elev);
    position: relative;
    display: block;
    text-decoration: none;
    color: inherit;
  }
  .frame img { width: 100%; height: 100%; object-fit: contain; background: #000; }
  .badge {
    position: absolute;
    top: var(--s-3);
    left: var(--s-3);
    background: var(--keep);
    color: #fff;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: var(--t-xs);
    font-weight: 600;
  }

  dl {
    display: grid;
    grid-template-columns: 60px 1fr;
    gap: 4px var(--s-3);
    padding: var(--s-3) var(--s-4);
    margin: 0;
  }
  dt {
    font-size: var(--t-xs);
    color: var(--ink-muted);
    padding-top: 2px;
  }
  dd {
    margin: 0;
    font-size: var(--t-xs);
    color: var(--ink-soft);
  }
  dd.path {
    color: var(--ink-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    padding: 0 var(--s-4) var(--s-4);
    margin-top: auto;
  }
  .keep-btn { width: 100%; }
  .keep-btn:disabled { opacity: 0.7; cursor: default; }
</style>
