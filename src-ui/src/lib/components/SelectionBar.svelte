<script lang="ts">
  import { FolderMinus, FolderPlus, Trash2, X } from "lucide-svelte";

  interface Props {
    count: number;
    onAddToAlbum: () => void;
    onTrash: () => void;
    onCancel: () => void;
    /// Optional "Remove from this album" action. Only shown when the
    /// caller (AlbumDetail) provides a handler, so other selection
    /// contexts (Timeline, MemoryDetail, PersonDetail) don't get a
    /// non-applicable button.
    onRemoveFromAlbum?: () => void;
  }
  let { count, onAddToAlbum, onTrash, onCancel, onRemoveFromAlbum }: Props = $props();
</script>

<div class="bar" role="region" aria-label="Selection actions">
  <span class="count">
    <span class="num mono">{count}</span>
    <span class="label">selected</span>
  </span>

  <div class="spacer"></div>

  <button class="action" onclick={onAddToAlbum} title="Add to album (A)">
    <FolderPlus size={15} strokeWidth={1.75} />
    <span>Add to album</span>
  </button>

  {#if onRemoveFromAlbum}
    <button class="action" onclick={onRemoveFromAlbum} title="Remove from this album">
      <FolderMinus size={15} strokeWidth={1.75} />
      <span>Remove from album</span>
    </button>
  {/if}

  <button class="action danger" onclick={onTrash} title="Move to trash (Del)">
    <Trash2 size={15} strokeWidth={1.75} />
    <span>Move to trash</span>
  </button>

  <span class="sep"></span>

  <button class="action ghost" onclick={onCancel} title="Cancel (Esc)" aria-label="Cancel selection">
    <X size={15} strokeWidth={1.75} />
  </button>
</div>

<style>
  .bar {
    position: fixed;
    left: 50%;
    bottom: var(--s-5);
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 6px var(--s-3) 6px var(--s-4);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-pill, 999px);
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.45),
                0 4px 12px rgba(0, 0, 0, 0.25);
    z-index: 50;
    backdrop-filter: blur(12px);
    animation: slide-up 180ms var(--ease) both;
  }
  @keyframes slide-up {
    from { opacity: 0; transform: translate(-50%, 12px); }
    to   { opacity: 1; transform: translate(-50%, 0); }
  }
  .count {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    padding-right: var(--s-2);
  }
  .num {
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
  }
  .label { font-size: var(--t-xs); color: var(--ink-muted); }
  .spacer { width: 1px; height: 22px; background: var(--line); }
  .sep { width: 1px; height: 22px; background: var(--line); margin: 0 4px; }
  .action {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 999px;
    font-size: var(--t-sm);
    color: var(--ink-soft);
    cursor: pointer;
    transition: background var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
  }
  .action:hover { background: var(--bg-card); color: var(--ink); }
  .action.danger:hover { color: var(--danger, #d96363); }
  .action.ghost { padding: 6px; }
</style>
