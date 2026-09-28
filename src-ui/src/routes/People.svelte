<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { routeCache } from "../lib/stores/routeCache.svelte";
  import { people, settings, type FaceDetailDto } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { PersonDto } from "../lib/api/types";

  let clusters = $state<PersonDto[]>([]);
  let liveFaces = $state<FaceDetailDto[]>([]);
  // Main grid holds clusters with 2+ photos — the people Smriti has
  // actually grouped. Singletons (clusters with exactly one photo) are
  // shown in a secondary section below; they're faces detected but not
  // yet recognised across multiple photos. Without this split, small
  // libraries where every face is unique end up with an empty People
  // view even after detection ran successfully.
  function isNamed(c: PersonDto): boolean {
    return !!c.name?.trim();
  }

  const mainClusters = $derived(clusters.filter((c) => c.photo_count >= 2 || isNamed(c)));
  const singletons   = $derived(clusters.filter((c) => c.photo_count === 1 && !isNamed(c)));
  let error = $state<string | null>(null);
  let loading = $state(true);
  let pendingPhotos = $state(0);
  let unconfirmedTotal = $state(0);
  let clustersWithUnconfirmed = $state(0);
  let showModelUpgradeBanner = $state(false);
  let faceActionBusy = $state(false);
  let pageEl = $state<HTMLDivElement | undefined>(undefined);
  let mounted = true;
  let loadSeq = 0;
  let liveSeq = 0;
  let pendingSeq = 0;
  let chunkRefreshRaf = 0;
  const scrollStorageKey = $derived(`smriti:people-scroll:${libraryStore.driveRoot ?? "closed"}`);

  function saveScroll() {
    if (!pageEl) return;
    try { sessionStorage.setItem(scrollStorageKey, String(pageEl.scrollTop)); } catch {}
  }

  function restoreScroll() {
    const raw = (() => { try { return sessionStorage.getItem(scrollStorageKey); } catch { return null; } })();
    const y = raw ? Number(raw) : 0;
    if (!Number.isFinite(y) || y <= 0) return;
    requestAnimationFrame(() => {
      if (mounted && pageEl) pageEl.scrollTop = y;
    });
  }
  async function checkModelUpgrade() {
    try {
      const s = await settings.get();
      if (!mounted) return;
      if (s.face_embedder_model === "adaface_ir101_webface12m.onnx" &&
          !localStorage.getItem("smriti_model_upgrade_dismissed")) {
        showModelUpgradeBanner = true;
      }
    } catch {}
  }
  async function reRunFacesFromScratch() {
    if (faceActionBusy || running) return;
    if (
      !confirm(
        "Re-run face detection with the upgraded model?\n\nThis clears existing face groups and names so Smriti can rebuild embeddings from scratch.",
      )
    )
      return;
    faceActionBusy = true;
    try {
      await people.resetAll();
    } catch (e) {
      if (mounted) {
        const msg = commandErrorMessage(e);
        error = msg;
        toasts.error(`Couldn't reset faces: ${msg}`);
      }
      return;
    } finally {
      if (mounted) faceActionBusy = false;
    }
    if (!mounted) return;
    localStorage.setItem("smriti_model_upgrade_dismissed", "1");
    showModelUpgradeBanner = false;
    startFaceProcessing();
  }
  function dismissModelUpgrade() {
    localStorage.setItem("smriti_model_upgrade_dismissed", "1");
    showModelUpgradeBanner = false;
  }
  // Engine emits a `chunks_flushed` counter that bumps every time the
  // writer thread commits a batch to disk. We track the last value the
  // page reloaded against and refetch whenever it advances — that's how
  // newly-found faces stream into the grid mid-run.
  let lastSeenChunks = $state(0);
  let lastSeenJob = $state<string | null>(null);
  let refreshInFlight = false;
  let refreshDirty = false;
  // Track last seen faces_found for the "new faces" toast
  let lastFacesFound = $state(0);

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

  async function load(background = false) {
    const seq = ++loadSeq;
    error = null;
    // Stale-while-revalidate: paint the cached grid instantly on a repeat
    // visit, then always refetch — clusters change while a face job runs
    // (the chunks_flushed watcher above reloads), so the cache is an
    // accelerator, never a source of truth. The faces-job completion
    // hook invalidates it outright.
    const cached = routeCache.peek<typeof clusters>("people:clusters");
    if (cached) {
      clusters = cached;
      restoreScroll();
    } else if (!background) {
      loading = true;
    }
    try {
      const next = await people.list({});
      if (!mounted || seq !== loadSeq) return;
      clusters = next;
      routeCache.put("people:clusters", next);
      restoreScroll();
    } catch (e) {
      if (!mounted || seq !== loadSeq) return;
      error = commandErrorMessage(e);
    } finally {
      if (mounted && seq === loadSeq) loading = false;
    }
  }

  async function loadLiveFaces() {
    const seq = ++liveSeq;
    const job = facesJob?.id;
    try {
      const page = await people.unclusteredFaces(null, 24, true);
      if (!mounted || seq !== liveSeq || facesJob?.id !== job || !running) return;
      liveFaces = page.items;
    } catch {
      // Non-critical: the canonical People grid still loads normally.
    }
  }

  async function loadPending() {
    const seq = ++pendingSeq;
    try {
      const [r, review] = await Promise.all([
        people.pendingFaceCount(),
        people.reviewFaceCount(),
      ]);
      if (!mounted || seq !== pendingSeq) return;
      pendingPhotos = r.pending_photos;
      unconfirmedTotal = review.unconfirmed_total;
      clustersWithUnconfirmed = review.clusters_with_unconfirmed;
    } catch {
      // silent — banner just hides
    }
  }

  async function startFaceProcessing() {
    if (running || faceActionBusy) return;
    const root = libraryStore.driveRoot;
    faceActionBusy = true;
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
      if (libraryStore.driveRoot !== root) return;
      jobs.register(r.job_id, "faces");
    } catch (e) {
      jobs.dismiss(placeholderId);
      if (!mounted || libraryStore.driveRoot !== root) return;
      const msg = commandErrorMessage(e);
      error = msg;
      toasts.error(`Couldn't start face detection: ${msg}`);
    } finally {
      if (mounted) faceActionBusy = false;
    }
  }

  async function refreshLive() {
    if (refreshInFlight) { refreshDirty = true; return; }
    refreshInFlight = true;
    try {
      do {
        refreshDirty = false;
        await Promise.all([load(true), loadLiveFaces()]);
      } while (refreshDirty && mounted && running);
    } finally { refreshInFlight = false; }
  }

  $effect(() => {
    const job = facesJob?.id ?? null;
    if (job !== lastSeenJob) {
      lastSeenJob = job;
      lastSeenChunks = 0;
      liveSeq++;
      liveFaces = [];
      if (chunkRefreshRaf !== 0) cancelAnimationFrame(chunkRefreshRaf);
      chunkRefreshRaf = 0;
    }
    const c = facesJob?.chunks_flushed ?? 0;
    if (c > lastSeenChunks) {
      lastSeenChunks = c;
      // Coalesce writer notifications to a paint; refreshLive serializes
      // any additional work that arrives while the request is running.
      if (chunkRefreshRaf === 0) {
        chunkRefreshRaf = requestAnimationFrame(() => {
          chunkRefreshRaf = 0;
          if (!mounted) return;
          void refreshLive();
        });
      }
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
      liveFaces = [];
      const msg = facesJob.message || "Face detection finished.";
      if (facesJob.message?.toLowerCase().startsWith("face detection failed")) {
        toasts.error(msg);
      } else {
        toasts.success(msg);
      }
    }
  });

  // Toast when new faces are found during a running job
  $effect(() => {
    if (facesJob?.status !== "running") return;
    const currentFound = facesJob?.faces_found ?? 0;
    if (currentFound > lastFacesFound) {
      const diff = currentFound - lastFacesFound;
      lastFacesFound = currentFound;
      toasts.success(`+${diff.toLocaleString()} new face${diff === 1 ? "" : "s"} found`);
    }
  });

  onMount(() => {
    mounted = true;
    load();
    loadLiveFaces();
    loadPending();
    checkModelUpgrade();
    return () => {
      saveScroll();
      mounted = false;
      loadSeq += 1;
      liveSeq += 1;
      pendingSeq += 1;
      if (chunkRefreshRaf !== 0) {
        cancelAnimationFrame(chunkRefreshRaf);
        chunkRefreshRaf = 0;
      }
    };
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
  <span class="count mono">{mainClusters.length}<span class="muted"> people</span></span>
  {#if running && facesJob}
    <span class="run-status mono">
      {facesJob.processed.toLocaleString()}{facesJob.total ? ` / ${facesJob.total.toLocaleString()}` : ""}
      <span class="muted">·</span>
      {(facesJob.faces_found ?? 0).toLocaleString()} face{(facesJob.faces_found ?? 0) === 1 ? "" : "s"}
    </span>
    <button class="ghost" disabled>Finding…</button>
  {:else}
    <a class="ghost review-link" href="#/review-faces">Review faces</a>
    <button class="primary" onclick={startFaceProcessing} disabled={running || faceActionBusy}>Find faces</button>
  {/if}
</PageHeader>

{#if running && facesJob && progressPct != null}
  <div class="progress" aria-label="Face detection progress">
    <div class="bar"><div class="fill" style="width: {progressPct}%"></div></div>
  </div>
{/if}

{#if !running && showModelUpgradeBanner}
  <div class="resume-banner" style="border-color: var(--accent); background: color-mix(in oklab, var(--bg-card) 80%, var(--accent) 15%)">
    <div class="resume-text">
      <strong>Model upgraded</strong>
      We've upgraded the face recognition model to AdaFace.
      Re-run face detection to apply the improved embeddings.
    </div>
    <button class="primary" onclick={reRunFacesFromScratch} disabled={running || faceActionBusy}>Re-run detection</button>
    <button class="ghost" onclick={dismissModelUpgrade} disabled={faceActionBusy}>Dismiss</button>
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
    <button class="primary" onclick={startFaceProcessing} disabled={running || faceActionBusy}>Resume detection</button>
  </div>
{/if}

{#if !running && unconfirmedTotal > 0}
  <div class="resume-banner verify-banner">
    <div class="resume-text">
      <strong>{unconfirmedTotal.toLocaleString()}</strong>
      face{unconfirmedTotal === 1 ? "" : "s"} need verification across {clustersWithUnconfirmed} {clustersWithUnconfirmed === 1 ? "person" : "people"}.
      <span class="hint">
        Confirm or reject faces that might not belong to their assigned person.
      </span>
    </div>
    <a class="primary" href="#/review-faces" style="text-decoration:none;display:inline-flex;align-items:center;padding:7px 14px;border-radius:var(--r-md);font-weight:600">Verify now</a>
  </div>
{/if}

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page" bind:this={pageEl} onscroll={saveScroll}>
    {#if running && liveFaces.length > 0}
      <section class="live-faces">
        <header class="live-head">
          <h3 class="live-title">
            Faces just detected
            <span class="singletons-count mono">{(facesJob?.faces_found ?? 0).toLocaleString()}</span>
          </h3>
        </header>
        <div class="singletons-grid">
          {#each liveFaces as f (f.face_id)}
            <a class="singleton-card" href="#/photo?id={f.photo_id}">
              <div class="singleton-frame">
                {#if f.thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, f.thumbnail_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder small">Â·</span>
                {/if}
              </div>
            </a>
          {/each}
        </div>
      </section>
    {/if}


  {#if loading}
    <div class="empty">
      <p class="working">Loading people...</p>
    </div>
  {:else if clusters.length === 0 && !running}
    <div class="empty">
      <p>No faces yet. Run face detection to start finding the people in your library.</p>
      <button class="primary" onclick={startFaceProcessing} disabled={running || faceActionBusy}>Find faces</button>
    </div>
  {:else if clusters.length === 0 && running}
    <div class="empty">
      <p class="working">Looking through your photos. Faces will appear here as they're found — feel free to navigate away.</p>
    </div>
  {:else}
    {#if mainClusters.length > 0}
      <div class="grid">
        {#each mainClusters as c (c.id)}
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

    {#if singletons.length > 0}
      <section class="singletons">
        <header class="singletons-head">
          <h3 class="singletons-title">
            Faces seen only once
            <span class="singletons-count mono">{singletons.length}</span>
          </h3>
          <p class="singletons-hint">
            Detected in a single photo each. Click any face to name them — or open one and use <em>Merge</em> to join it to a person above.
          </p>
        </header>
        <div class="singletons-grid">
          {#each singletons as c (c.id)}
            <a class="singleton-card" href="#/person?id={c.id}">
              <div class="singleton-frame">
                {#if c.representative_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, c.representative_thumbnail_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder small">·</span>
                {/if}
              </div>
              {#if c.name}
                <span class="singleton-name">{c.name}</span>
              {/if}
            </a>
          {/each}
        </div>
      </section>
    {/if}
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
    content-visibility: auto;
    contain-intrinsic-size: 180px 220px;
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

  .live-faces {
    margin-top: var(--s-6);
    padding-top: var(--s-4);
    border-top: 1px solid var(--line-soft);
  }
  .live-head {
    margin-bottom: var(--s-3);
  }
  .live-title {
    font-family: var(--font-display);
    font-size: var(--t-base);
    font-weight: 500;
    color: var(--ink-soft);
    margin: 0;
    display: inline-flex;
    align-items: baseline;
    gap: var(--s-2);
  }

  /* Singletons section — faces detected in a single photo each.
     De-emphasized vs the main grid so a long tail of one-shot faces
     doesn't drown the primary content. */
  .singletons {
    margin-top: var(--s-8);
    padding-top: var(--s-5);
    border-top: 1px solid var(--line-soft);
  }
  .singletons-head {
    margin-bottom: var(--s-4);
    max-width: 64ch;
  }
  .singletons-title {
    font-family: var(--font-display);
    font-size: var(--t-lg);
    font-weight: 500;
    font-variation-settings: "opsz" 24;
    color: var(--ink-soft);
    margin: 0 0 var(--s-2);
    letter-spacing: -0.012em;
    display: inline-flex;
    align-items: baseline;
    gap: var(--s-2);
  }
  .singletons-count {
    font-size: var(--t-xs);
    color: var(--ink-faint);
    font-weight: 400;
    letter-spacing: 0.04em;
  }
  .singletons-hint {
    color: var(--ink-muted);
    font-size: var(--t-sm);
    line-height: 1.55;
    margin: 0;
  }
  .singletons-hint em {
    font-style: normal;
    color: var(--ink-soft);
    font-weight: 500;
  }
  .singletons-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: var(--s-3) var(--s-2);
  }
  .singleton-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: center;
    text-decoration: none;
    content-visibility: auto;
    contain-intrinsic-size: 92px 120px;
    color: inherit;
    opacity: 0.78;
    transition: opacity var(--t-fast) var(--ease);
  }
  .singleton-card:hover { opacity: 1; }
  .singleton-frame {
    aspect-ratio: 1;
    min-width: 0;
    background: var(--bg-card);
    border-radius: 50%;
    overflow: hidden;
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .singleton-frame img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .singleton-card:hover .singleton-frame {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .singleton-name {
    font-size: var(--t-2xs);
    color: var(--ink-muted);
    font-family: var(--font-body);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 var(--s-1);
  }
</style>
