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
</script>

<main class="detail">
  <header>
    <a href="#/duplicates">← Duplicates</a>
    <h2>Group #{id}</h2>
    {#if group}
      <button onclick={trashOthers} class="danger">Trash {group.members.length - 1} others</button>
      <button onclick={dismiss} class="ghost">Dismiss</button>
    {/if}
  </header>
  {#if error}<p class="error">{error}</p>{/if}
  {#if group}
    <div class="row">
      {#each group.members as m}
        <div class="card" class:keep={m.is_suggested_keep}>
          {#if m.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, m.thumbnail_path) ?? ""} alt="" />
          {/if}
          <div class="meta">
            <span class="muted small">{(m.file_size ?? 0) / 1024 / 1024 | 0} MB</span>
            <span class="muted small">{m.date_taken ?? "no date"}</span>
            <span class="muted small">{m.file_path ?? ""}</span>
            {#if m.is_suggested_keep}
              <strong>Keep</strong>
            {:else}
              <button onclick={() => setKeep(m.photo_id)}>Keep this</button>
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
  .ghost { background: transparent; border: 1px solid #2a2a2d; }
  .danger { background: #2a1414; border: 1px solid #4a2222; color: #f87171; }
  .row { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 12px; }
  .card { background: #131316; border-radius: 8px; overflow: hidden; border: 2px solid transparent; }
  .card.keep { border-color: #4ade80; }
  .card img { width: 100%; aspect-ratio: 1; object-fit: cover; }
  .meta { padding: 12px 14px; display: flex; flex-direction: column; gap: 4px; }
  .small { font-size: 12px; }
</style>
