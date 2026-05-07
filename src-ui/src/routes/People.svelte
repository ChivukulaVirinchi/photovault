<script lang="ts">
  import { onMount } from "svelte";
  import { people } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { PersonDto } from "../lib/api/types";

  let clusters = $state<PersonDto[]>([]);
  let error = $state<string | null>(null);
  let pendingPhotos = $state(0);
  // Engine emits a `chunks_flushed` counter that bumps every time the
  // writer thread commits a batch to disk. We track the last value the
  // page reloaded against and refetch whenever it advances — that's how
  // newly-found faces stream into the grid mid-run.
  let lastSeenChunks = $state(0);

  // Live state from the global jobs store. The job runs in tokio::spawn
  // and is independent of this component's lifecycle, so reading from
  // the store is the only way to know what's actually happening.
  const facesJob = $derived(jobs.byKind("faces"));
  const running = $derived(jobs.isRunning("faces"));
  const progressPct = $derived.by(() => {
    const j = facesJob;
    if (!j || !j.total || j.total <= 0) return null;
    return Math.min(100, Math.round((j.processed / j.total) * 100));
  });

  async function load() {
    try {
      clusters = await people.list({ minPhotos: 2 });
    } catch (e) {
      error = JSON.stringify(e);
    }
  }

  async function loadPending() {
    try {
      const r = await people.pendingFaceCount();
      pendingPhotos = r.pending_photos;
    } catch {
      // silent — banner just hides
    }
  }

  async function startFaceProcessing() {
    if (running) return;
    // Optimistic placeholder so the user sees the click registered
    // even on a cold-started ONNX worker (which can take 2-3 s before
    // the first real progress event lands). Replaced with the real
    // job-id as soon as the IPC returns.
    const placeholderId = `pending-faces-${Date.now()}`;
    jobs.register(placeholderId, "faces");
    toasts.success("Looking for faces — feel free to navigate away.");
    try {
      const r = await people.startProcessing();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "faces");
    } catch (e) {
      jobs.dismiss(placeholderId);
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      error = msg;
      toasts.error(`Couldn't start face detection: ${msg}`);
    }
  }

  $effect(() => {
    const c = facesJob?.chunks_flushed ?? 0;
    if (c > lastSeenChunks) {
      lastSeenChunks = c;
      load();
    }
  });

  // When a faces job finishes, refetch to pick up the final cluster
  // set, AND surface the engine's result message as a toast so a fast
  // run (e.g., "0 photos to process" or "model not found") isn't
  // visually silent. We track ids we've already toasted so we don't
  // re-toast on every reactivity tick after completion.
  let toastedJobIds = new Set<string>();
  $effect(() => {
    if (!facesJob) return;
    if (facesJob.status === "complete" && !toastedJobIds.has(facesJob.id)) {
      toastedJobIds.add(facesJob.id);
      load();
      const msg = facesJob.message || "Face detection finished.";
      if (facesJob.message?.toLowerCase().startsWith("face detection failed")) {
        toasts.error(msg);
      } else {
        toasts.success(msg);
      }
    }
  });

  onMount(() => {
    load();
    loadPending();
  });

  // Refresh pending count when a face job completes (so the banner
  // reflects what's actually left after the run).
  $effect(() => {
    if (facesJob?.status === "complete") {
      loadPending();
    }
  });
</script>

<PageHeader title="People">
  <span class="count mono">{clusters.length}<span class="muted"> people</span></span>
  {#if running && facesJob}
    <span class="run-status mono">
      {facesJob.processed.toLocaleString()}{facesJob.total ? ` / ${facesJob.total.toLocaleString()}` : ""}
      <span class="muted">·</span>
      {(facesJob.faces_found ?? 0).toLocaleString()} face{(facesJob.faces_found ?? 0) === 1 ? "" : "s"}
    </span>
    <button class="ghost" disabled>Finding…</button>
  {:else}
    <a class="ghost review-link" href="#/people/review">Review faces</a>
    <button class="primary" onclick={startFaceProcessing}>Find faces</button>
  {/if}
</PageHeader>

{#if running && facesJob && progressPct != null}
  <div class="progress" aria-label="Face detection progress">
    <div class="bar"><div class="fill" style="width: {progressPct}%"></div></div>
  </div>
{/if}

{#if !running && pendingPhotos > 0}
  <div class="resume-banner">
    <div class="resume-text">
      <strong>{pendingPhotos.toLocaleString()}</strong>
      photo{pendingPhotos === 1 ? "" : "s"} still need face detection.
      <span class="hint">
        Pick up where you left off — works even if you moved the drive from another machine.
      </span>
    </div>
    <button class="primary" onclick={startFaceProcessing}>Resume detection</button>
  </div>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if clusters.length === 0 && !running}
    <div class="empty">
      <p>No faces yet. Run face detection to start finding the people in your library.</p>
      <button class="primary" onclick={startFaceProcessing}>Find faces</button>
    </div>
  {:else if clusters.length === 0 && running}
    <div class="empty">
      <p class="working">Looking through your photos. Faces will appear here as they're found — feel free to navigate away.</p>
    </div>
  {:else}
    <div class="grid">
      {#each clusters as c (c.id)}
        <a class="card" href="#/person?id={c.id}">
          <div class="frame">
            {#if c.representative_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="placeholder small">no face</span>
            {/if}
          </div>
          <div class="caption">
            <strong class="name">{c.name ?? "Unnamed"}</strong>
            <span class="count mono">{c.photo_count} photos</span>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page {
    padding: var(--s-5) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
  }
  .count {
    font-size: var(--t-sm);
    color: var(--ink);
  }
  .run-status {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .review-link {
    text-decoration: none;
    font-size: var(--t-sm);
    padding: 6px 14px;
  }
  .progress {
    padding: 0 var(--s-7);
  }
  .bar {
    height: 3px;
    background: var(--bg-card);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar .fill {
    height: 100%;
    background: var(--accent);
    transition: width 280ms cubic-bezier(0.22, 0.61, 0.36, 1);
  }
  .resume-banner {
    margin: var(--s-3) var(--s-7) var(--s-2);
    padding: var(--s-3) var(--s-4);
    border: 1px solid color-mix(in oklab, var(--line) 60%, var(--accent) 40%);
    background: color-mix(in oklab, var(--bg-card) 80%, var(--accent) 20%);
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: var(--s-4);
  }
  .resume-banner .resume-text {
    flex: 1;
    color: var(--ink);
    font-size: var(--t-sm);
  }
  .resume-banner .resume-text strong {
    color: var(--ink);
    font-weight: 600;
  }
  .resume-banner .resume-text .hint {
    display: block;
    margin-top: 2px;
    color: var(--ink-muted);
    font-size: var(--t-xs);
    line-height: 1.4;
  }

  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    align-items: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p {
    color: var(--ink-soft);
    line-height: 1.55;
  }
  .empty p.working { color: var(--ink); }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--s-5) var(--s-3);
  }
  .card {
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    text-align: center;
    text-decoration: none;
  }
  .frame {
    aspect-ratio: 1;
    min-width: 0;
    background: var(--bg-card);
    border-radius: 50%;
    overflow: hidden;
    position: relative;
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .card:hover .frame {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .frame img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .placeholder {
    color: var(--ink-faint);
  }
  .caption {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 0 var(--s-1);
  }
  .name {
    font-family: var(--font-display);
    font-size: var(--t-base);
    font-weight: 500;
    font-variation-settings: "opsz" 18;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption .count {
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }
  .small { font-size: var(--t-xs); }
</style>
