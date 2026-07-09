<script lang="ts">
  import { onMount } from "svelte";
  import { people, type FaceDetailDto } from "../api/all";
  import { commandErrorMessage } from "../api";
  import FaceCell from "./FaceCell.svelte";
  import { X } from "lucide-svelte";

  interface Props {
    clusterId: number;
    onclose: () => void;
  }
  let { clusterId, onclose }: Props = $props();

  let faces = $state<FaceDetailDto[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let mounted = true;

  async function load() {
    try {
      const next = await people.kSimilarToCluster(clusterId, 20);
      if (!mounted) return;
      faces = next;
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    }
  }

  async function confirmFace(faceId: number) {
    if (busy) return;
    busy = true;
    try {
      await people.faceConfirmToCluster(faceId, clusterId);
      if (!mounted) return;
      faces = faces.filter((f) => f.face_id !== faceId);
      message = "Face confirmed.";
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }

  async function rejectFace(faceId: number) {
    if (busy) return;
    busy = true;
    try {
      await people.faceReject(faceId, clusterId);
      if (!mounted) return;
      faces = faces.filter((f) => f.face_id !== faceId);
      message = "Face rejected.";
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }

  function requestClose() {
    if (!busy) onclose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      requestClose();
    }
  }

  onMount(() => {
    mounted = true;
    load();
    return () => {
      mounted = false;
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="overlay"
  onclick={(e) => { if (e.target === e.currentTarget) requestClose(); }}
>
  <div
    class="dialog"
    onkeydown={onKey}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="Find more like this"
  >
    <header>
      <h3>Find more like this</h3>
      <p class="hint">
        These faces may belong to this person but weren't clustered together.
        Confirm or reject each one.
      </p>
      <button
        class="close-btn"
        onclick={requestClose}
        disabled={busy}
        aria-label="Close"
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {/if}
    {#if message}
      <p class="msg">{message}</p>
    {/if}

    <div class="grid-scroll">
      {#if faces.length > 0}
        <div class="grid">
          {#each faces as f (f.face_id)}
            <FaceCell
              face={{ face_id: f.face_id, user_confirmed: f.user_confirmed, thumbnail_path: f.thumbnail_path }}
              onConfirm={confirmFace}
              onReject={rejectFace}
              showActions={true}
              busy={busy}
            />
          {/each}
        </div>
      {:else if !busy}
        <p class="empty">No similar faces found.</p>
      {/if}
    </div>

    <footer>
      <span class="count mono">{faces.length} face{faces.length === 1 ? "" : "s"}</span>
      <button class="ghost" onclick={requestClose} disabled={busy}>Done</button>
    </footer>
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
    width: min(520px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  header {
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    position: relative;
    flex-shrink: 0;
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
  .close-btn {
    position: absolute;
    right: var(--s-4);
    top: var(--s-4);
    width: 28px;
    height: 28px;
    background: transparent;
    border: none;
    color: var(--ink-muted);
    border-radius: var(--r-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .close-btn:hover {
    background: var(--bg-card);
    color: var(--ink);
  }
  .error {
    padding: var(--s-2) var(--s-5);
    background: color-mix(in oklab, var(--bg-paper) 70%, var(--hot));
    color: var(--hot);
    font-size: var(--t-xs);
    margin: 0;
  }
  .msg {
    padding: var(--s-2) var(--s-5);
    background: color-mix(in oklab, var(--bg-paper) 80%, var(--keep));
    color: var(--keep);
    font-size: var(--t-xs);
    margin: 0;
  }
  .grid-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: var(--s-4) var(--s-5);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(80px, 1fr));
    gap: var(--s-2);
  }
  .empty {
    padding: var(--s-6) var(--s-4);
    color: var(--ink-muted);
    font-size: var(--t-sm);
    text-align: center;
    font-style: italic;
  }
  footer {
    padding: var(--s-3) var(--s-5);
    border-top: 1px solid var(--line-soft);
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .count {
    color: var(--ink-muted);
    font-size: var(--t-xs);
  }
</style>
