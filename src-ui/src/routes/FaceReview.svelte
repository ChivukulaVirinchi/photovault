<script lang="ts">
  import { ignoreShortcut } from "../lib/keys";
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { people, type FaceDetailDto, type ReviewFaceCountDto } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import Check from "lucide-svelte/icons/check";
  import X from "lucide-svelte/icons/x";
  import SkipForward from "lucide-svelte/icons/skip-forward";
  import EyeOff from "lucide-svelte/icons/eye-off";
  import Undo2 from "lucide-svelte/icons/undo-2";

  let stats = $state<ReviewFaceCountDto | null>(null);
  let faces = $state<FaceDetailDto[]>([]);
  let cursor = $state(0);
  let busy = $state(false);
  let confirmed = $state(0);
  let rejected = $state(0);
  let skipped = $state(0);
  let error = $state<string | null>(null);
  let mounted = true;
  let loadSeq = 0;
  let skippedFaceIds = $state(new Set<number>());
  type Answer = "confirm" | "reject" | "skip" | "ignore";
  /// The last answer, so a misclick can be taken back (U or Ctrl+Z).
  let last = $state<{ token: string; item: FaceDetailDto; kind: Answer } | null>(null);

  const current = $derived(faces[cursor] ?? null);
  const remaining = $derived(Math.max(0, faces.length - cursor));

  function faceCropUrl(faceId: number): string | null {
    return thumbUrl(libraryStore.driveRoot, `.photovault/faces/${faceId}.jpg`);
  }

  /// Find the first person with unconfirmed faces and load them.
  async function loadNextCluster() {
    const seq = ++loadSeq;
    error = null;
    try {
      const nextStats = await people.reviewFaceCount();
      if (!mounted || seq !== loadSeq) return;
      stats = nextStats;

      if (nextStats.unconfirmed_total === 0) {
        faces = [];
        cursor = 0;
        return;
      }

      const page = await people.nextUnconfirmedFaces(200, Array.from(skippedFaceIds));
      if (!mounted || seq !== loadSeq) return;
      if (page.items.length > 0) {
        faces = page.items;
        cursor = 0;
        return;
      }
      faces = [];
      cursor = 0;
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function answer(kind: Answer) {
    if (!current || busy) return;
    busy = true;
    const item = current;
    try {
      let token: string;
      if (kind === "confirm") {
        token = await people.faceConfirm(item.face_id);
        if (!mounted) return;
        confirmed += 1;
      } else if (kind === "reject") {
        token = await people.faceReject(item.face_id);
        if (!mounted) return;
        rejected += 1;
      } else if (kind === "ignore") {
        token = await people.faceIgnore(item.face_id);
        if (!mounted) return;
        rejected += 1;
      } else {
        token = await people.faceSkip(item.face_id);
        if (!mounted) return;
        skippedFaceIds.add(item.face_id);
        skipped += 1;
      }
      last = { token, item, kind };
      // Remove current from list
      faces.splice(cursor, 1);
      // If the list is empty, move to next cluster
      if (faces.length === 0) {
        await loadNextCluster();
      }
    } catch (e) {
      if (mounted) error = commandErrorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }

  /// Take back the last answer: the face comes back up next.
  async function undoLast() {
    const answered = last;
    if (!answered || busy) return;
    busy = true;
    try {
      await people.faceUndo(answered.token);
      if (!mounted) return;
      last = null;
      if (answered.kind === "confirm") confirmed -= 1;
      else if (answered.kind === "skip") {
        skipped -= 1;
        skippedFaceIds.delete(answered.item.face_id);
      } else rejected -= 1;
      faces.splice(cursor, 0, answered.item);
    } catch (e) {
      if (mounted) error = commandErrorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (ignoreShortcut(e)) return;
    if (e.key === "u" || e.key === "U" || ((e.ctrlKey || e.metaKey) && (e.key === "z" || e.key === "Z"))) {
      e.preventDefault();
      void undoLast();
      return;
    }
    if (busy || !current) {
      if (e.key === "Escape" || e.key === "q" || e.key === "Q") {
        e.preventDefault();
        window.location.hash = "/people";
      }
      return;
    }
    if (e.key === "y" || e.key === "Y") {
      e.preventDefault();
      answer("confirm");
    } else if (e.key === "n" || e.key === "N") {
      e.preventDefault();
      answer("reject");
    } else if (e.key === "s" || e.key === "S") {
      e.preventDefault();
      answer("skip");
    } else if (e.key === "f" || e.key === "F") {
      e.preventDefault();
      answer("ignore");
    } else if (e.key === "Escape" || e.key === "q" || e.key === "Q") {
      e.preventDefault();
      window.location.hash = "/people";
    }
  }

  onMount(() => {
    void loadNextCluster();
    window.addEventListener("keydown", onKey);
    return () => {
      mounted = false;
      loadSeq += 1;
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<DetailHeader backHref="#/people" backLabel="People">
  {#snippet title()}
    <h1>Review faces</h1>
  {/snippet}
  {#snippet subtitle()}
    <span class="hint">
      Is this the same person?
      Y = yes, N = no, F = not a face, S = not sure, U = undo, Esc = close.
    </span>
  {/snippet}
  {#snippet actions()}
    <span class="counts mono">
      {confirmed} yes · {rejected} no · {skipped} skipped
    </span>
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if !current && faces.length === 0}
    <div class="empty">
      {#if (stats?.unconfirmed_total ?? 0) > 0 && skippedFaceIds.size > 0}
        <p class="done">All done. Faces you skipped won't come back.</p>
      {:else}
        <p class="done">All done. Nothing left to check.</p>
      {/if}
    </div>
  {:else if !current}
    <div class="empty">
      <p class="slow-reveal">Loading…</p>
    </div>
  {:else}
    {@const c = current}
    <div class="stage" class:busy>
      <div class="face-large">
        {#if faceCropUrl(c.face_id)}
          <img src={faceCropUrl(c.face_id) ?? ""} alt="Face to review" />
        {/if}
      </div>
      <div class="cluster-info">
        <span class="cluster-name">{c.cluster_name ?? "Unnamed"}</span>
        <span class="confidence mono">
          Looks like them · {(c.confidence * 100).toFixed(0)}%
        </span>
      </div>
    </div>

    <div class="actions-row">
      <button
        class="action no"
        onclick={() => answer("reject")}
        disabled={busy}
        title="Not this person (N)"
      >
        <X size={18} strokeWidth={2.25} />
        <span>Not same</span>
      </button>
      <button
        class="action skip"
        onclick={() => answer("ignore")}
        disabled={busy}
        title="Not a face (F)"
      >
        <EyeOff size={16} strokeWidth={2} />
        <span>Not a face</span>
      </button>
      <button
        class="action skip"
        onclick={() => answer("skip")}
        disabled={busy}
        title="Not sure (S)"
      >
        <SkipForward size={16} strokeWidth={2} />
        <span>Not sure</span>
      </button>
      <button
        class="action yes"
        onclick={() => answer("confirm")}
        disabled={busy}
        title="Same person (Y)"
      >
        <Check size={18} strokeWidth={2.25} />
        <span>Confirm</span>
      </button>
    </div>

    <div class="progress">
      <span class="mono small muted">{remaining} more to review in this cluster</span>
    </div>
  {/if}
  {#if last}
    <button class="undo" onclick={undoLast} disabled={busy} title="Undo (U)">
      <Undo2 size={14} strokeWidth={2} />
      <span>Undo</span>
    </button>
  {/if}
</div>

<style>
  .hint {
    color: var(--ink-muted);
    font-style: italic;
  }
  .counts {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .page {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-6) var(--s-7) var(--s-7);
    display: flex;
    flex-direction: column;
    gap: var(--s-5);
    align-items: center;
  }
  .empty {
    padding: var(--s-9) var(--s-5);
    max-width: 48ch;
    text-align: center;
  }
  .empty p {
    color: var(--ink-soft);
    line-height: 1.55;
  }
  .empty .done {
    color: var(--ink);
    font-weight: 500;
  }

  .stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-4);
    transition: opacity var(--t-fast) var(--ease);
  }
  .stage.busy {
    opacity: 0.55;
  }
  .face-large {
    width: 260px;
    height: 260px;
    border-radius: var(--r-lg);
    overflow: hidden;
    background: var(--bg-card);
    border: 2px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .face-large img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .cluster-info {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .cluster-name {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 500;
    color: var(--ink);
  }
  .confidence {
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }

  .undo {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: var(--s-4);
    padding: 6px 12px;
    border: 0;
    border-radius: var(--r-pill, 999px);
    background: transparent;
    color: var(--ink-muted);
    font-size: var(--t-sm);
    animation: undo-in var(--t-base-d, 200ms) var(--ease) both;
    transition: color var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  .undo:hover, .undo:focus-visible {
    color: var(--ink);
    background: var(--bg-card);
  }
  @keyframes undo-in { from { opacity: 0; transform: translateY(-2px); } }
  @media (prefers-reduced-motion: reduce) { .undo { animation: none; } }
  .actions-row {
    display: flex;
    gap: var(--s-3);
    align-items: center;
  }
  .action {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    padding: 12px 22px;
    border-radius: var(--r-pill, 999px);
    border: 1px solid var(--line);
    background: var(--bg-paper);
    color: var(--ink);
    font-size: var(--t-base);
    font-weight: 500;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
  }
  .action:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .action:hover:not(:disabled) {
    background: var(--bg-card);
  }
  .action.yes:hover:not(:disabled) {
    border-color: var(--keep, var(--accent));
    color: var(--keep, var(--accent));
  }
  .action.no:hover:not(:disabled) {
    border-color: var(--hot, #d05a4a);
    color: var(--hot, #d05a4a);
  }
  .action.skip {
    color: var(--ink-muted);
  }
  .progress {
    color: var(--ink-faint);
  }

  @media (max-width: 720px) {
    .face-large {
      width: 200px;
      height: 200px;
    }
    .action {
      padding: 10px 16px;
    }
  }
</style>
