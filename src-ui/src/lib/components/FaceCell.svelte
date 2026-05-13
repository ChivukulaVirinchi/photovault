<script lang="ts">
  import { thumbUrl } from "../thumbnail";
  import { libraryStore } from "../stores/library.svelte";
  import { Check, ArrowLeftRight, X } from "lucide-svelte";

  interface Props {
    face: {
      face_id: number;
      user_confirmed: number;
      thumbnail_path: string | null;
    };
    onConfirm?: (faceId: number) => void;
    onReject?: (faceId: number) => void;
    onReassign?: (faceId: number) => void;
    onClick?: (faceId: number) => void;
    /// Always show action buttons (not just on hover) — used in KSimilarDialog.
    showActions?: boolean;
    /// Disable all interaction.
    busy?: boolean;
  }

  let {
    face,
    onConfirm,
    onReject,
    onReassign,
    onClick,
    showActions = false,
    busy = false,
  }: Props = $props();

  let hover = $state(false);

  function faceCropUrl(faceId: number): string | null {
    return thumbUrl(
      libraryStore.driveRoot,
      `.photovault/faces/${faceId}.jpg`,
    );
  }

  const unconfirmed = $derived(face.user_confirmed === 0);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="face-cell"
  class:unconfirmed
  class:hover
  class:busy
  onmouseenter={() => (hover = true)}
  onmouseleave={() => (hover = false)}
  onclick={() => onClick?.(face.face_id)}
  role={onClick ? "button" : undefined}
  tabindex={onClick ? 0 : undefined}
>
  {#if faceCropUrl(face.face_id)}
    <img src={faceCropUrl(face.face_id) ?? ""} alt="" loading="lazy" />
  {:else}
    <span class="placeholder small">no face</span>
  {/if}

  {#if (hover || showActions) && !busy}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="actions" onclick={(e) => e.stopPropagation()}>
      {#if unconfirmed && onConfirm}
        <button
          class="action-btn confirm"
          title="Confirm"
          onclick={() => onConfirm(face.face_id)}
        >
          <Check size={14} strokeWidth={2.25} />
        </button>
      {/if}
      {#if onReassign}
        <button
          class="action-btn reassign"
          title="Move to another person"
          onclick={() => onReassign(face.face_id)}
        >
          <ArrowLeftRight size={14} strokeWidth={2.25} />
        </button>
      {/if}
      {#if onReject}
        <button
          class="action-btn reject"
          title="Reject"
          onclick={() => onReject(face.face_id)}
        >
          <X size={14} strokeWidth={2.25} />
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .face-cell {
    width: 80px;
    height: 80px;
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-card);
    position: relative;
    cursor: default;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: box-shadow var(--t-fast) var(--ease);
  }
  .face-cell[role="button"] {
    cursor: pointer;
  }
  .face-cell.unconfirmed {
    box-shadow: inset 0 0 0 2px color-mix(in oklab, var(--accent) 60%, transparent);
  }
  .face-cell:hover img {
    filter: brightness(1.08);
  }
  .face-cell.busy {
    opacity: 0.55;
    pointer-events: none;
  }
  .face-cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    transition: filter var(--t-fast) var(--ease);
  }
  .placeholder {
    color: var(--ink-faint);
  }

  .actions {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    background: rgba(0, 0, 0, 0.45);
    animation: fade-in 120ms var(--ease);
  }
  .action-btn {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: none;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease),
              color var(--t-fast) var(--ease);
    background: rgba(255, 255, 255, 0.18);
    color: white;
  }
  .action-btn:hover {
    background: rgba(255, 255, 255, 0.34);
  }
  .action-btn.confirm:hover {
    background: var(--keep, #7ea291);
    color: white;
  }
  .action-btn.reassign:hover {
    background: var(--accent);
    color: var(--invert-ink);
  }
  .action-btn.reject:hover {
    background: var(--hot, #c47373);
    color: white;
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
