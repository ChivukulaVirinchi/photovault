<script lang="ts">
  import { thumbUrl } from "../thumbnail";
  import { libraryStore } from "../stores/library.svelte";
  import Check from "lucide-svelte/icons/check";
  import ArrowLeftRight from "lucide-svelte/icons/arrow-left-right";
  import X from "lucide-svelte/icons/x";
  import EyeOff from "lucide-svelte/icons/eye-off";

  interface Props {
    face: {
      face_id: number;
      user_confirmed: number;
      thumbnail_path: string | null;
    };
    onConfirm?: (faceId: number) => void;
    onReject?: (faceId: number) => void;
    onReassign?: (faceId: number) => void;
    /// "Not a face": shown when given.
    onIgnore?: (faceId: number) => void;
    onClick?: (faceId: number) => void;
    /// Always show action buttons (not just on hover) — used in KSimilarDialog.
    showActions?: boolean;
    /// Side in px. Large cells keep the face clear and put the actions in a
    /// bar along the bottom instead of over it.
    size?: number;
    /// A quiet note in the corner, such as how close a match is.
    caption?: string | null;
    /// Disable all interaction.
    busy?: boolean;
  }

  let {
    face,
    onConfirm,
    onReject,
    onReassign,
    onIgnore,
    onClick,
    showActions = false,
    size = 80,
    caption = null,
    busy = false,
  }: Props = $props();
  const large = $derived(size >= 120);

  let hover = $state(false);

  function faceCropUrl(faceId: number): string | null {
    return thumbUrl(
      libraryStore.driveRoot,
      `.photovault/faces/${faceId}.jpg`,
    );
  }

  const unconfirmed = $derived(face.user_confirmed === 0);
  let focused = $state(false);
</script>

<!-- Role and tabindex are set only when the cell is clickable. -->
<!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
<div
  class="face-cell"
  class:unconfirmed
  class:hover
  class:busy
  class:large
  style:width="{size}px"
  style:height="{size}px"
  onmouseenter={() => (hover = true)}
  onmouseleave={() => (hover = false)}
  onfocusin={() => (focused = true)}
  onfocusout={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) focused = false; }}
  onclick={() => onClick?.(face.face_id)}
  onkeydown={(e) => {
    if (onClick && e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onClick(face.face_id);
    }
  }}
  role={onClick ? "button" : undefined}
  tabindex={onClick ? 0 : undefined}
  aria-label={onClick ? "Face" : undefined}
>
  {#if faceCropUrl(face.face_id)}
    <img src={faceCropUrl(face.face_id) ?? ""} alt="" loading="lazy" />
  {:else}
    <span class="placeholder small">no face</span>
  {/if}

  {#if caption}
    <span class="caption mono">{caption}</span>
  {/if}

  {#if (hover || focused || showActions) && !busy}
    <!-- Only stops clicks on the buttons from also opening the face. -->
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div class="actions" onclick={(e) => e.stopPropagation()}>
      {#if unconfirmed && onConfirm}
        <button
          class="action-btn confirm"
          title="Confirm"
          aria-label="Confirm"
          onclick={() => onConfirm(face.face_id)}
        >
          <Check size={14} strokeWidth={2.25} />
        </button>
      {/if}
      {#if onReassign}
        <button
          class="action-btn reassign"
          title="Move to someone else"
          aria-label="Move to someone else"
          onclick={() => onReassign(face.face_id)}
        >
          <ArrowLeftRight size={14} strokeWidth={2.25} />
        </button>
      {/if}
      {#if onIgnore}
        <button
          class="action-btn reject"
          title="Not a face"
          aria-label="Not a face"
          onclick={() => onIgnore(face.face_id)}
        >
          <EyeOff size={14} strokeWidth={2.25} />
        </button>
      {/if}
      {#if onReject}
        <button
          class="action-btn reject"
          title="Reject"
          aria-label="Reject"
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
  .caption {
    position: absolute;
    top: 6px;
    left: 6px;
    padding: 1px 6px;
    border-radius: var(--r-pill, 999px);
    background: rgb(15 16 21 / 55%);
    color: #e7eaf0;
    font-size: 10px;
    letter-spacing: 0.02em;
  }
  .face-cell.large {
    border-radius: var(--r-lg, 12px);
  }
  .face-cell.large .actions {
    inset: auto 0 0 0;
    padding: 18px 8px 8px;
    gap: 8px;
    background: linear-gradient(to top, rgb(10 11 14 / 72%), transparent);
  }
  .face-cell.large .action-btn {
    width: 32px;
    height: 32px;
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
