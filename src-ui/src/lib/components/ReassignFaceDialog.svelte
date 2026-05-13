<script lang="ts">
  import { onMount } from "svelte";
  import { people, type ClusterSuggestionDto } from "../api/all";
  import type { PersonDto } from "../api/types";
  import { libraryStore } from "../stores/library.svelte";
  import { thumbUrl } from "../thumbnail";
  import { Search, Users } from "lucide-svelte";

  interface Props {
    faceId: number;
    onclose: () => void;
    onsuccess?: () => void;
  }
  let { faceId, onclose, onsuccess }: Props = $props();

  let suggestions = $state<ClusterSuggestionDto[]>([]);
  let allPeople = $state<PersonDto[]>([]);
  let filter = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let activeTab = $state<"suggestions" | "search">("suggestions");
  let inputEl: HTMLInputElement | undefined = $state(undefined);

  const filteredPeople = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return allPeople.filter((c) => {
      if (!q) return true;
      return (c.name ?? "").toLowerCase().includes(q) ||
        `person ${c.id}`.includes(q);
    });
  });

  async function loadSuggestions() {
    try {
      suggestions = await people.faceSuggestClusters(faceId, 5);
    } catch (e) {
      error = String(e);
    }
  }

  async function loadAllPeople() {
    try {
      allPeople = await people.list({});
    } catch (e) {
      error = String(e);
    }
  }

  async function moveTo(clusterId: number) {
    if (busy) return;
    busy = true;
    try {
      await people.faceReassign(faceId, clusterId);
      onsuccess?.();
      onclose();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  function faceCropUrl(faceId: number | null): string | null {
    if (!faceId) return null;
    return thumbUrl(
      libraryStore.driveRoot,
      `.photovault/faces/${faceId}.jpg`,
    );
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "Enter" && activeTab === "search") {
      e.preventDefault();
      const first = filteredPeople[0];
      if (first) moveTo(first.id);
    }
  }

  onMount(() => {
    loadSuggestions();
    loadAllPeople();
    setTimeout(() => inputEl?.focus(), 0);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="overlay"
  onclick={(e) => { if (e.target === e.currentTarget) onclose(); }}
>
  <div
    class="dialog"
    onkeydown={onKey}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="Reassign face"
  >
    <header>
      <h3>Move face to…</h3>
      <p class="hint">
        Pick a target person cluster. The face will be reassigned immediately.
      </p>
    </header>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="tabs">
      <button
        class="tab"
        class:active={activeTab === "suggestions"}
        onclick={() => (activeTab = "suggestions")}
      >
        Suggested
      </button>
      <button
        class="tab"
        class:active={activeTab === "search"}
        onclick={() => {
          activeTab = "search";
          setTimeout(() => inputEl?.focus(), 0);
        }}
      >
        All people
      </button>
    </div>

    {#if activeTab === "suggestions"}
      <ul class="list">
        {#each suggestions as s (s.cluster_id)}
          <li>
            <button
              class="row"
              onclick={() => moveTo(s.cluster_id)}
              disabled={busy}
            >
              <span class="avatar">
                {#if s.representative_face_id}
                  <img
                    src={faceCropUrl(s.representative_face_id) ?? ""}
                    alt=""
                  />
                {:else}
                  <Users size={14} strokeWidth={1.75} />
                {/if}
              </span>
              <span class="info">
                <span class="name">{s.name}</span>
                <span class="meta mono">
                  {s.face_count} face{s.face_count === 1 ? "" : "s"}
                  · {Math.round(s.score * 100)}% match
                </span>
              </span>
            </button>
          </li>
        {/each}
        {#if suggestions.length === 0}
          <li class="empty">No suggestions available.</li>
        {/if}
      </ul>
    {:else}
      <div class="search">
        <Search size={14} strokeWidth={1.75} />
        <input
          bind:this={inputEl}
          bind:value={filter}
          placeholder="Find a person…"
        />
      </div>
      <ul class="list">
        {#each filteredPeople as c (c.id)}
          <li>
            <button
              class="row"
              onclick={() => moveTo(c.id)}
              disabled={busy}
            >
              <span class="avatar">
                {#if c.representative_thumbnail_path}
                  <img
                    src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""}
                    alt=""
                  />
                {:else}
                  <Users size={14} strokeWidth={1.75} />
                {/if}
              </span>
              <span class="name">{c.name ?? `Person ${c.id}`}</span>
              <span class="count mono">{c.photo_count}</span>
            </button>
          </li>
        {/each}
        {#if filteredPeople.length === 0}
          <li class="empty">
            {filter
              ? `No person matches "${filter}".`
              : "No people to move to."}
          </li>
        {/if}
      </ul>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(6px);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dialog {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-lg, 12px);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55);
    width: min(440px, 92vw);
    max-height: 75vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
  }
  header h3 {
    margin: 0;
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
  }
  .hint {
    margin: 4px 0 0;
    font-size: var(--t-xs);
    color: var(--ink-muted);
    line-height: 1.4;
  }
  .error {
    padding: var(--s-2) var(--s-5);
    background: color-mix(in oklab, var(--bg-paper) 70%, var(--hot));
    color: var(--hot);
    font-size: var(--t-xs);
    margin: 0;
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--line-soft);
  }
  .tab {
    flex: 1;
    padding: var(--s-2) var(--s-3);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    font-size: var(--t-sm);
    font-weight: 500;
    color: var(--ink-muted);
    cursor: pointer;
    transition: color var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease);
  }
  .tab.active {
    color: var(--ink);
    border-bottom-color: var(--accent);
  }
  .tab:hover {
    color: var(--ink);
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-3) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
  }
  .search :global(svg) {
    color: var(--ink-muted);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: var(--t-sm);
    color: var(--ink);
    padding: 4px 0;
  }
  .search input:focus {
    outline: none;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 6px 0;
    overflow-y: auto;
    flex: 1;
  }
  .list .empty {
    padding: var(--s-4);
    color: var(--ink-muted);
    font-size: var(--t-sm);
    text-align: center;
    font-style: italic;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    width: 100%;
    padding: 8px var(--s-5);
    background: transparent;
    border: none;
    color: var(--ink);
    font-size: var(--t-sm);
    text-align: left;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease);
  }
  .row:hover,
  .row:focus {
    background: var(--bg-card);
    outline: none;
  }
  .row:disabled {
    opacity: 0.5;
    cursor: wait;
  }
  .avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg-card);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--ink-muted);
    flex-shrink: 0;
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .name {
    flex: 1;
    font-weight: 500;
  }
  .meta {
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
  .count {
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
</style>
