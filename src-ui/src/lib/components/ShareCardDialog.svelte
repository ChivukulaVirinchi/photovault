<script lang="ts">
  import { onDestroy } from "svelte";
  import { Check, Copy } from "lucide-svelte";
  import { commandErrorMessage } from "../api";
  import { burstConfetti } from "../confetti";
  import { shareCard } from "../stores/shareCard.svelte";
  import { toasts } from "../stores/toast.svelte";
  import { ensureCardFonts, readCardTheme, renderShareCard } from "../shareCard";

  let dialog = $state<HTMLDialogElement | undefined>();
  let canvas = $state<HTMLCanvasElement | undefined>();
  let busy = $state(false);
  let flash = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  onDestroy(() => {
    if (flashTimer) clearTimeout(flashTimer);
  });

  function showFlash(text: string) {
    flash = text;
    if (flashTimer) clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      flash = null;
      flashTimer = null;
    }, 1900);
  }

  /// Draw the card whenever the dialog opens for a new library. Fonts have to
  /// finish loading first, or the hero number renders in a fallback serif.
  $effect(() => {
    const data = shareCard.data;
    const open = shareCard.open;
    const element = canvas;
    if (!open || !data || !element) return;
    let cancelled = false;
    void ensureCardFonts().then(() => {
      if (cancelled) return;
      renderShareCard(element, data, readCardTheme());
    });
    return () => { cancelled = true; };
  });

  $effect(() => {
    const element = dialog;
    if (!element) return;
    if (shareCard.open && !element.open) element.showModal();
    if (!shareCard.open && element.open) element.close();
  });

  function close() {
    shareCard.open = false;
  }

  function toBlob(): Promise<Blob> {
    return new Promise((resolve, reject) => {
      if (!canvas) return reject(new Error("The card is still rendering."));
      canvas.toBlob((blob) => {
        blob ? resolve(blob) : reject(new Error("The card could not be rendered."));
      }, "image/png");
    });
  }

  /// Last resort on platforms without image clipboard support: hand the user
  /// the PNG directly rather than leaving them with nothing.
  function download(blob: Blob) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `smriti-library-${shareCard.data?.heatmap_year ?? "card"}.png`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }

  async function copyCard() {
    if (busy) return;
    busy = true;
    try {
      if (typeof ClipboardItem === "undefined" || !navigator.clipboard?.write) {
        download(await toBlob());
        showFlash("Saved as a PNG — this system can't copy images");
        return;
      }
      await navigator.clipboard.write([new ClipboardItem({ "image/png": await toBlob() })]);
      showFlash("Card copied — paste it anywhere");
      if (dialog) burstConfetti(dialog, { count: 38, origin: { x: 0.5, y: 0.38 } });
    } catch (error) {
      toasts.error(commandErrorMessage(error));
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="share-card-title"
  onclose={close}
  onkeydown={(event) => event.stopPropagation()}
>
  <header class="head">
    <div>
      <h2 id="share-card-title" class="display">Your library card</h2>
      <p class="sub">Counts only — no photos, no faces, no names.</p>
    </div>
    <button class="ghost icon" onclick={close} aria-label="Close">
      <span class="x" aria-hidden="true">×</span>
    </button>
  </header>

  <div class="preview">
    <canvas bind:this={canvas} width="1080" height="1350" aria-label="Preview of your library card"></canvas>
  </div>

  <div class="flash" aria-live="polite">
    {#if flash}
      <span class="pill"><Check size={13} strokeWidth={2.2} />{flash}</span>
    {/if}
  </div>

  <div class="actions">
    <button class="primary" onclick={copyCard} disabled={busy}>
      <Copy size={15} strokeWidth={1.8} />
      Copy card
    </button>
  </div>
</dialog>

<style>
  dialog {
    position: relative;
    margin: auto;
    width: min(720px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    overflow: auto;
    padding: var(--s-5);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--bg-paper);
    color: var(--ink);
    box-shadow: 0 24px 60px rgb(0 0 0 / 38%);
    animation: dialog-in var(--t-base-d) var(--ease-out);
  }
  dialog::backdrop { background: rgb(0 0 0 / 55%); }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--s-3);
    margin-bottom: var(--s-4);
  }
  h2 { margin: 0; font-size: var(--t-xl); }
  .sub {
    margin: 4px 0 0;
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }
  .icon {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    display: grid;
    place-items: center;
    color: var(--ink-muted);
  }
  .x { font-size: 20px; line-height: 1; }

  .preview {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg);
    line-height: 0;
  }
  canvas {
    width: 100%;
    height: auto;
    display: block;
  }

  /* Reserved height so the note and buttons never jump when the flash lands. */
  .flash {
    height: 26px;
    display: grid;
    place-items: center;
    margin-top: var(--s-3);
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    font-size: var(--t-xs);
    color: var(--ink-soft);
    background: var(--accent-ghost);
    border: 1px solid var(--accent-soft);
    border-radius: 999px;
    animation: flash-in var(--t-base-d) var(--ease-out);
  }

  .actions {
    display: flex;
  }
  .actions button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  @keyframes dialog-in {
    from { opacity: 0; transform: translateY(10px) scale(0.985); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes flash-in {
    from { opacity: 0; transform: translateY(4px) scale(0.96); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  @media (prefers-reduced-motion: reduce) {
    dialog, .pill { animation: none; }
  }
</style>
