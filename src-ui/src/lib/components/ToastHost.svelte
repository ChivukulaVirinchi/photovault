<script lang="ts">
  import { toasts } from "../stores/toast.svelte";
  import { CheckCircle2, AlertCircle, Info, Undo2, X } from "lucide-svelte";

  function iconFor(kind: string) {
    if (kind === "success") return CheckCircle2;
    if (kind === "error") return AlertCircle;
    if (kind === "undo") return Undo2;
    return Info;
  }

  async function handleUndo(id: number, onUndo?: () => void | Promise<void>) {
    if (onUndo) {
      try { await onUndo(); } catch { /* the action's caller handles its own errors */ }
    }
    toasts.dismiss(id);
  }
</script>

<div class="host" role="region" aria-label="Notifications" aria-live="polite">
  {#each toasts.list as t (t.id)}
    {@const Icon = iconFor(t.kind)}
    <div class="toast" data-kind={t.kind} role="status">
      <span class="icon"><Icon size={15} strokeWidth={1.75} /></span>
      <span class="msg">{t.message}</span>
      {#if t.kind === "undo" && t.onUndo}
        <button class="undo" onclick={() => handleUndo(t.id, t.onUndo)}>Undo</button>
      {/if}
      <button class="close" onclick={() => toasts.dismiss(t.id)} aria-label="Dismiss">
        <X size={13} strokeWidth={1.75} />
      </button>
    </div>
  {/each}
</div>

<style>
  .host {
    position: fixed;
    left: 50%;
    bottom: var(--s-5);
    transform: translateX(-50%);
    display: flex;
    flex-direction: column-reverse;
    gap: var(--s-2);
    z-index: 90;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    gap: var(--s-3);
    padding: 10px var(--s-3) 10px var(--s-4);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-pill, 999px);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.45),
                0 4px 14px rgba(0, 0, 0, 0.25);
    font-size: var(--t-sm);
    color: var(--ink);
    min-width: 260px;
    max-width: min(520px, 80vw);
    backdrop-filter: blur(10px);
    animation: rise 220ms var(--ease) both;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .icon {
    display: inline-flex;
    align-items: center;
    color: var(--ink-muted);
    flex-shrink: 0;
  }
  .toast[data-kind="success"] .icon { color: var(--success, #6db080); }
  .toast[data-kind="error"]   .icon { color: var(--danger, #d96363); }
  .toast[data-kind="undo"]    .icon { color: var(--accent); }
  .msg {
    flex: 1;
    line-height: 1.35;
    word-break: break-word;
  }
  .undo {
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-pill, 999px);
    padding: 4px 12px;
    color: var(--accent);
    font-weight: 600;
    font-size: var(--t-sm);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--t-fast) var(--ease);
  }
  .undo:hover { background: var(--accent-ghost); }
  .close {
    background: transparent;
    border: none;
    padding: 4px;
    color: var(--ink-muted);
    cursor: pointer;
    flex-shrink: 0;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    transition: background var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
  }
  .close:hover { background: var(--bg-card); color: var(--ink); }
</style>
