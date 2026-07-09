<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { people, type FaceDetailDto, type ReviewFaceCountDto } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import { Check, X, SkipForward } from "lucide-svelte";

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
  let skippedFaceIds = new Set<number>();

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

  async function answer(kind: "confirm" | "reject" | "skip") {
    if (!current || busy) return;
    busy = true;
    const item = current;
    try {
      if (kind === "confirm") {
        await people.faceConfirm(item.face_id);
        if (!mounted) return;
        confirmed += 1;
      } else if (kind === "reject") {
        await people.faceReject(item.face_id);
        if (!mounted) return;
        rejected += 1;
      } else {
        if (!mounted) return;
        skippedFaceIds.add(item.face_id);
        skipped += 1;
      }
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

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
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
      Confirm or reject unconfirmed faces within each person cluster.
      Y = confirm, N = reject, S = skip, Esc = close.
    </span>
  {/snippet}
  {#snippet actions()}
    <span class="counts mono">
      {confirmed} confirmed · {rejected} rejected · {skipped} skipped
    </span>
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if !current && faces.length === 0}
    <div class="empty">
      {#if (stats?.unconfirmed_total ?? 0) > 0 && skippedFaceIds.size > 0}
        <p class="done">All caught up for this session — skipped faces will stay available for a later pass.</p>
      {:else}
        <p class="done">All caught up — no unconfirmed faces left to review.</p>
      {/if}
    </div>
  {:else if !current}
    <div class="empty">
      <p>Loading…</p>
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
          {(c.confidence * 100).toFixed(0)}% confidence
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
        onclick={() => answer("skip")}
        disabled={busy}
        title="Skip (S)"
      >
        <SkipForward size={16} strokeWidth={2} />
        <span>Skip</span>
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
