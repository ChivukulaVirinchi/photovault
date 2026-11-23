<script lang="ts">
  import { trapFocus } from "../actions/trapFocus";
  import { onMount } from "svelte";
  import { people, type FaceDetailDto } from "../api/all";
  import { commandErrorMessage } from "../api";
  import FaceCell from "./FaceCell.svelte";
  import { offerFaceUndo } from "../faceUndo";
  import X from "lucide-svelte/icons/x";

  interface Props {
    clusterId: number;
    onclose: () => void;
  }
  let { clusterId, onclose }: Props = $props();

  let faces = $state<FaceDetailDto[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let mounted = true;

  async function load() {
    try {
      // Enough to work through in one sitting without a "load more".
      const next = await people.kSimilarToCluster(clusterId, 48);
      if (!mounted) return;
      faces = next;
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    }
  }

  /// A decision on one face, with Undo that puts it back in its place.
  async function decide(faceId: number, act: () => Promise<string>, done: string) {
    if (busy) return;
    busy = true;
    const index = faces.findIndex((f) => f.face_id === faceId);
    const face = faces[index];
    try {
      const token = await act();
      if (!mounted) return;
      faces = faces.filter((f) => f.face_id !== faceId);
      offerFaceUndo(token, done, () => {
        if (!mounted || !face) return;
        const next = [...faces];
        next.splice(Math.min(index, next.length), 0, face);
        faces = next;
      });
    } catch (e) {
      if (!mounted) return;
      error = commandErrorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }

  const confirmFace = (faceId: number) =>
    decide(faceId, () => people.faceConfirmToCluster(faceId, clusterId), "Added to this person");
  const rejectFace = (faceId: number) =>
    decide(faceId, () => people.faceReject(faceId, clusterId), "Not this person");

  /// How close a face is to this person's photos, as a share of a perfect match.
  const closeness = (score: number) => `${Math.round(Math.max(0, score) * 100)}%`;

  /// The face's photo, for context; decisions made so far are kept.
  function openPhoto(photoId: number) {
    if (!photoId || busy) return;
    onclose();
    window.location.hash = `#/photo?id=${photoId}`;
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
    class="dialog pv-dialog"
    onkeydown={onKey}
    role="dialog" use:trapFocus
    tabindex="-1"
    aria-modal="true"
    aria-label="Find more like this"
  >
    <header>
      <h3>Find more like this</h3>
      <p class="hint">
        Faces that look like this person, closest first. Keep the ones that are
        them; click a face to see its photo.
      </p>
      <button
        class="close-btn"
        onclick={requestClose}
        disabled={busy}
        aria-label="Close" title="Close (Esc)"
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="grid-scroll">
      {#if faces.length > 0}
        <div class="grid">
          {#each faces as f (f.face_id)}
            <FaceCell
              face={{ face_id: f.face_id, user_confirmed: f.user_confirmed, thumbnail_path: f.thumbnail_path }}
              size={148}
              caption={closeness(f.confidence)}
              onConfirm={confirmFace}
              onReject={rejectFace}
              onClick={() => openPhoto(f.photo_id)}
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
    padding: var(--s-4);
  }
  .dialog {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-lg, 12px);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55);
    width: min(880px, 92vw);
    max-height: min(80vh, calc(100vh - 2 * var(--s-4)));
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
  .grid-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: var(--s-4) var(--s-5);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
    justify-items: center;
    gap: var(--s-3);
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
