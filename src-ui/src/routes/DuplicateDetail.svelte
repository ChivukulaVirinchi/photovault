<script lang="ts">
  import { duplicates } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
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

<div class="masthead">
  <a class="back" href="#/duplicates">← Duplicates</a>
  <span class="eyebrow">
    <span class="num">№&nbsp;{String(id).padStart(3, "0")}</span>
    <span class="ornament"></span>
    <span>GROUP</span>
  </span>
  <h1>The same photograph.</h1>
  {#if group}
    <p class="subtitle">{group.members.length} copies. Pick one to keep — trash the rest.</p>
    <div class="row">
      <button class="danger" onclick={trashOthers}>Trash {group.members.length - 1} others</button>
      <button class="ghost" onclick={dismiss}>Dismiss</button>
    </div>
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if group}
  <div class="row stagger">
    {#each group.members as m, i (m.photo_id)}
      <div class="card" class:keep={m.is_suggested_keep} style="--i: {i}">
        <div class="frame">
          {#if m.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, m.thumbnail_path) ?? ""} alt="" />
          {/if}
          {#if m.is_suggested_keep}<span class="badge mono">KEEP</span>{/if}
        </div>
        <dl>
          <dt>Size</dt><dd class="mono">{fmtSize(m.file_size)}</dd>
          <dt>Date</dt><dd class="mono">{m.date_taken ?? "—"}</dd>
          <dt>Path</dt><dd class="mono path" title={m.file_path ?? ""}>{m.file_path ?? "—"}</dd>
        </dl>
        {#if !m.is_suggested_keep}
          <button class="primary" onclick={() => setKeep(m.photo_id)}>Keep this one</button>
        {/if}
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
  .row { display: flex; gap: var(--s-2); flex-wrap: wrap; }

  .row.stagger {
    padding: var(--s-5) var(--s-7) var(--s-7);
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
  .card.keep { border-color: var(--keep); box-shadow: 0 0 0 1px var(--keep) inset; }
  .frame {
    aspect-ratio: 1;
    background: #000;
    position: relative;
  }
  .frame img { width: 100%; height: 100%; object-fit: cover; }
  .badge {
    position: absolute;
    top: var(--s-3); left: var(--s-3);
    background: var(--keep);
    color: var(--bg);
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 10px;
    letter-spacing: 0.16em;
    font-weight: 600;
  }

  dl {
    display: grid;
    grid-template-columns: 60px 1fr;
    gap: 4px var(--s-3);
    padding: var(--s-4);
    margin: 0;
  }
  dt {
    font-family: var(--font-mono);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-faint);
    padding-top: 2px;
  }
  dd { margin: 0; font-size: var(--t-xs); color: var(--ink-soft); }
  dd.path { font-size: 10.5px; color: var(--ink-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .card button { margin: 0 var(--s-4) var(--s-4); }
</style>
