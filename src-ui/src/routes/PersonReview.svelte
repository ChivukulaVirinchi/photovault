<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { people } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import { Check, X, SkipForward } from "lucide-svelte";
  import type { ReviewItem } from "../lib/api/all";

  // The clustering pipeline writes ambiguous face → cluster pairs to a
  // review queue. Each user "Same / Different / Skip" tap is a hard
  // anchor that propagates downstream — confirming one face often
  // disambiguates dozens of nearby ones because the resolver pulls
  // co-occurrence and temporal signals from confirmed neighbours.

  let queue = $state<ReviewItem[]>([]);
  let cursor = $state(0);
  let busy = $state(false);
  let confirmed = $state(0);
  let split = $state(0);
  let skipped = $state(0);
  let error = $state<string | null>(null);
  let mounted = true;
  let loadSeq = 0;

  const current = $derived(queue[cursor] ?? null);
  const remaining = $derived(Math.max(0, queue.length - cursor));

  async function load() {
    const seq = ++loadSeq;
    error = null;
    try {
      const items = await people.reviewQueue(50);
      if (!mounted || seq !== loadSeq) return;
      queue = items;
      cursor = 0;
    } catch (e) {
      if (!mounted || seq !== loadSeq) return;
      error = commandErrorMessage(e);
    }
  }

  function faceCropUrl(faceId: number): string | null {
    return thumbUrl(libraryStore.driveRoot, `.photovault/faces/${faceId}.jpg`);
  }

  async function answer(kind: "same" | "different" | "skip") {
    if (!current || busy) return;
    busy = true;
    const item = current;
    try {
      if (kind === "same") {
        await people.reviewSame(item.queue_id);
        if (!mounted) return;
        confirmed += 1;
      } else if (kind === "different") {
        await people.reviewDifferent(item.queue_id);
        if (!mounted) return;
        split += 1;
      } else {
        await people.reviewSkip(item.queue_id);
        if (!mounted) return;
        skipped += 1;
      }
      cursor += 1;
      // Refill when we're getting low — keeps the user reviewing.
      if (cursor >= queue.length - 5) {
        try {
          const more = await people.reviewQueue(50);
          if (!mounted) return;
          // Drop any items already loaded this session.
          const seen = new Set(queue.map((q) => q.queue_id));
          const fresh = more.filter((m) => !seen.has(m.queue_id));
          if (fresh.length > 0) {
            queue = [...queue, ...fresh];
          }
        } catch {}
      }
    } catch (e) {
      if (mounted) error = commandErrorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (busy || !current) return;
    if (e.key === "y" || e.key === "Y" || e.key === "ArrowRight") {
      e.preventDefault();
      answer("same");
    } else if (e.key === "n" || e.key === "N" || e.key === "ArrowLeft") {
      e.preventDefault();
      answer("different");
    } else if (e.key === " " || e.key === "ArrowDown") {
      e.preventDefault();
      answer("skip");
    }
  }

  onMount(() => {
    mounted = true;
    load();
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
      Each yes / no anchors dozens of similar faces downstream. Use Y / N / Space, or click below.
    </span>
  {/snippet}
  {#snippet actions()}
    <span class="counts mono">
      {confirmed} same · {split} different · {skipped} skipped
    </span>
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if !current && queue.length === 0}
    <div class="empty">
      <p>The review queue is empty. Run face detection or come back after the next pass — the resolver only flags ambiguous matches.</p>
    </div>
  {:else if !current}
    <div class="empty">
      <p class="done">All caught up — {confirmed + split} decisions made this session.</p>
    </div>
  {:else}
    {@const c = current}
    <div class="stage" class:busy>
      <div class="left">
        <span class="label small mono">Is this face</span>
        <div class="face-frame">
          {#if faceCropUrl(c.face_id)}
            <img src={faceCropUrl(c.face_id) ?? ""} alt="Face to review" />
          {/if}
        </div>
      </div>
      <div class="connector" aria-hidden="true">
        <span class="q">?</span>
      </div>
      <div class="right">
        <span class="label small mono">
          the same person as
          <span class="name">{c.candidate_cluster_name ?? "this group"}</span>
          ({c.candidate_cluster_size} {c.candidate_cluster_size === 1 ? "face" : "faces"})
        </span>
        <div class="cluster-grid">
          {#each c.candidate_sample_face_ids.slice(0, 6) as fid (fid)}
            <div class="face-frame small">
              {#if faceCropUrl(fid)}
                <img src={faceCropUrl(fid) ?? ""} alt="Cluster sample" />
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </div>

    <div class="actions-row">
      <button class="action no" onclick={() => answer("different")} disabled={busy} title="Different (N or ←)">
        <X size={18} strokeWidth={2.25} />
        <span>Different</span>
      </button>
      <button class="action skip" onclick={() => answer("skip")} disabled={busy} title="Skip (Space)">
        <SkipForward size={16} strokeWidth={2} />
        <span>Skip</span>
      </button>
      <button class="action yes" onclick={() => answer("same")} disabled={busy} title="Same (Y or →)">
        <Check size={18} strokeWidth={2.25} />
        <span>Same</span>
      </button>
    </div>

    <div class="progress">
      <span class="mono small muted">{remaining} more to review</span>
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
    gap: var(--s-6);
    align-items: center;
  }
  .empty {
    padding: var(--s-9) var(--s-5);
    max-width: 48ch;
    text-align: center;
  }
  .empty p { color: var(--ink-soft); line-height: 1.55; }
  .empty .done { color: var(--ink); font-weight: 500; }

  .stage {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: var(--s-5);
    width: 100%;
    max-width: 880px;
    transition: opacity var(--t-fast) var(--ease);
  }
  .stage.busy { opacity: 0.55; }
  .left, .right {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: center;
  }
  .label {
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .label .name {
    text-transform: none;
    letter-spacing: 0;
    color: var(--ink);
    font-weight: 600;
    font-family: var(--font-display);
  }
  .face-frame {
    width: 220px;
    height: 220px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg-card);
    border: 2px solid var(--line);
  }
  .face-frame.small {
    width: 88px;
    height: 88px;
    border-width: 1px;
  }
  .face-frame img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .cluster-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--s-2);
  }
  .connector .q {
    font-family: var(--font-display);
    font-size: 64px;
    color: var(--ink-faint);
    font-weight: 300;
  }

  .actions-row {
    display: flex;
    gap: var(--s-3);
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
  .action:disabled { opacity: 0.5; cursor: default; }
  .action:hover:not(:disabled) { background: var(--bg-card); }
  .action.yes:hover:not(:disabled) {
    border-color: var(--keep, var(--accent));
    color: var(--keep, var(--accent));
  }
  .action.no:hover:not(:disabled) {
    border-color: var(--hot, #d05a4a);
    color: var(--hot, #d05a4a);
  }
  .action.skip { color: var(--ink-muted); }
  .progress { color: var(--ink-faint); }
  .small { font-size: var(--t-xs); }
  .muted { color: var(--ink-muted); }

  @media (max-width: 720px) {
    .stage {
      grid-template-columns: 1fr;
    }
    .connector .q { font-size: 40px; transform: rotate(90deg); }
  }
</style>
