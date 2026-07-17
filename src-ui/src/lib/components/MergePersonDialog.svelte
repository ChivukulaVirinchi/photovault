<script lang="ts">
  import { onMount } from "svelte";
  import { people } from "../api/all";
  import { commandErrorMessage } from "../api";
  import type { PersonDto } from "../api/types";
  import { libraryStore } from "../stores/library.svelte";
  import { thumbUrl } from "../thumbnail";
  import { Search, Users } from "lucide-svelte";

  interface Props {
    source: PersonDto;
    onclose: () => void;
    onsuccess?: (target: PersonDto) => void;
  }
  let { source, onclose, onsuccess }: Props = $props();

  let all = $state<PersonDto[]>([]);
  let filter = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let inputEl: HTMLInputElement | undefined;
  let mounted = true;
  let focusTimer: ReturnType<typeof setTimeout> | null = null;

  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return all.filter((c) => {
      if (c.id === source.id) return false;
      if (!q) return true;
      return (c.name ?? "").toLowerCase().includes(q);
    });
  });

  async function load() {
    try {
      const next = await people.list({});
      if (!mounted) return;
      all = next;
    }
    catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    }
  }

  async function pick(target: PersonDto) {
    if (busy) return;
    const sourceLabel = source.name ?? `Person ${source.id}`;
    const targetLabel = target.name ?? `Person ${target.id}`;
    if (!confirm(`Merge ${sourceLabel} into ${targetLabel}? This can't be undone.`)) return;
    busy = true;
    try {
      const merged = await people.merge(source.id, target.id);
      if (!mounted) return;
      onsuccess?.(merged);
      onclose();
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
      busy = false;
    }
  }

  function requestClose() {
    if (!busy) onclose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); requestClose(); }
    else if (e.key === "Enter") {
      e.preventDefault();
      const first = filtered[0];
      if (first) pick(first);
    }
  }

  onMount(() => {
    mounted = true;
    load();
    focusTimer = setTimeout(() => inputEl?.focus(), 0);
    return () => {
      mounted = false;
      if (focusTimer != null) clearTimeout(focusTimer);
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={(e) => { if (e.target === e.currentTarget) requestClose(); }}>
  <div class="dialog" onkeydown={onKey} role="dialog" tabindex="-1" aria-modal="true" aria-label="Merge person">
    <header>
      <h3>Merge {source.name ?? `Person ${source.id}`} into…</h3>
      <p class="hint">Pick the person this is the same as. Faces and identities will combine into the chosen target.</p>
    </header>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="search">
      <Search size={14} strokeWidth={1.75} />
      <input bind:this={inputEl} bind:value={filter} placeholder="Find a person…" />
    </div>

    <ul class="list">
      {#each filtered as c (c.id)}
        <li>
          <button class="row" onclick={() => pick(c)} disabled={busy}>
            <span class="avatar">
              {#if c.representative_thumbnail_path}
                <img src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""} alt="" />
              {:else}
                <Users size={14} strokeWidth={1.75} />
              {/if}
            </span>
            <span class="name">{c.name ?? `Person ${c.id}`}</span>
            <span class="count mono">{c.photo_count}</span>
          </button>
        </li>
      {/each}
      {#if filtered.length === 0}
        <li class="empty">{filter ? `No person matches "${filter}".` : "No other people to merge into."}</li>
      {/if}
    </ul>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.55);
    backdrop-filter: blur(6px);
    z-index: 100;
    display: flex; align-items: center; justify-content: center;
    padding: var(--s-4);
  }
  .dialog {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-lg, 12px);
    box-shadow: 0 24px 64px rgba(0,0,0,0.55);
    width: min(440px, 100%);
    max-height: min(75vh, calc(100vh - 2 * var(--s-4)));
    display: flex; flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  header {
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  header h3 { margin: 0; font-size: var(--t-base); font-weight: 600; color: var(--ink); }
  .hint {
    margin: 4px 0 0; font-size: var(--t-xs); color: var(--ink-muted); line-height: 1.4;
  }
  .error {
    padding: var(--s-2) var(--s-5);
    background: color-mix(in oklab, var(--bg-paper) 70%, var(--danger, #d96363));
    color: var(--danger, #d96363);
    font-size: var(--t-xs);
    margin: 0;
  }
  .search {
    display: flex; align-items: center; gap: var(--s-2);
    padding: var(--s-3) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  .search :global(svg) { color: var(--ink-muted); }
  .search input {
    flex: 1; border: none; background: transparent;
    font-size: var(--t-sm); color: var(--ink); padding: 4px 0;
  }
  .search input:focus { outline: none; }
  .list { list-style: none; margin: 0; padding: 6px 0; overflow-y: auto; flex: 1 1 auto; min-height: 0; }
  .list .empty {
    padding: var(--s-4);
    color: var(--ink-muted);
    font-size: var(--t-sm);
    text-align: center;
    font-style: italic;
  }
  .row {
    display: flex; align-items: center; gap: var(--s-3);
    width: 100%; padding: 8px var(--s-5);
    background: transparent; border: none;
    color: var(--ink); font-size: var(--t-sm); text-align: left;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease);
  }
  .row:hover, .row:focus { background: var(--bg-card); outline: none; }
  .row:disabled { opacity: 0.5; cursor: wait; }
  .avatar {
    width: 28px; height: 28px;
    border-radius: 50%; overflow: hidden;
    background: var(--bg-card);
    display: inline-flex; align-items: center; justify-content: center;
    color: var(--ink-muted);
    flex-shrink: 0;
  }
  .avatar img { width: 100%; height: 100%; object-fit: cover; }
  .name { flex: 1; }
  .count { color: var(--ink-muted); font-size: var(--t-xs); }
</style>
