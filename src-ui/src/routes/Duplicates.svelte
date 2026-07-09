<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { commandErrorMessage } from "../lib/api";
  import { duplicates } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import PageHeader from "../lib/components/PageHeader.svelte";

  let groups = $state<Awaited<ReturnType<typeof duplicates.list>>>([]);
  let wasted = $state(0);
  let error = $state<string | null>(null);
  let mounted = true;
  let loadSeq = 0;
  let reloadedCompleteJobIds = new Set<string>();
  const PAGE_SIZE = 200;
  let hasMore = $state(false);
  let loadingMore = $state(false);

  // Detection runs in tokio::spawn_blocking on the backend. Read state
  // from the global jobs store so it survives navigation.
  const dupJob = $derived(jobs.byKind("duplicates"));
  const running = $derived(jobs.isRunning("duplicates"));

  async function load() {
    const seq = ++loadSeq;
    error = null;
    try {
      const [nextGroups, w] = await Promise.all([
        duplicates.list(PAGE_SIZE, 0),
        duplicates.wastedSpace(),
      ]);
      if (!mounted || seq !== loadSeq) return;
      groups = nextGroups;
      hasMore = nextGroups.length === PAGE_SIZE;
      wasted = w.bytes;
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    }
  }

  async function loadMore() {
    if (loadingMore || !hasMore) return;
    const seq = loadSeq;
    loadingMore = true;
    try {
      const nextGroups = await duplicates.list(PAGE_SIZE, groups.length);
      if (!mounted || seq !== loadSeq) return;
      groups = [...groups, ...nextGroups];
      hasMore = nextGroups.length === PAGE_SIZE;
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    } finally {
      if (mounted && seq === loadSeq) loadingMore = false;
    }
  }

  async function run() {
    if (running) return;
    const placeholderId = `pending-dups-${Date.now()}`;
    jobs.register(placeholderId, "duplicates");
    toasts.success("Detecting duplicates — feel free to navigate away.");
    try {
      const r = await duplicates.run(true);
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "duplicates");
    } catch (e) {
      jobs.dismiss(placeholderId);
      if (!mounted) return;
      const msg = commandErrorMessage(e);
      error = msg;
      toasts.error(`Couldn't start: ${msg}`);
    }
  }

  function patchThumbnail(groupId: number, thumbnailPath: string) {
    groups = groups.map((g) => (
      g.id === groupId ? { ...g, cover_thumbnail_path: thumbnailPath } : g
    ));
  }

  $effect(() => {
    if (!dupJob || dupJob.status !== "complete" || reloadedCompleteJobIds.has(dupJob.id)) return;
    reloadedCompleteJobIds.add(dupJob.id);
    load();
  });

  onMount(() => {
    mounted = true;
    load();
    const unlisten = listen<{ stage?: string; message?: string | null }>("duplicates:progress", (e) => {
      if (!mounted) return;
      if (e.payload.stage === "persisted") load();
      if (e.payload.stage === "error") {
        const msg = e.payload.message ?? "Duplicate detection failed.";
        error = msg;
        toasts.error(msg);
      }
    });
    return () => {
      mounted = false;
      loadSeq += 1;
      void unlisten.then((u) => u());
    };
  });
</script>

<PageHeader title="Duplicates">
  <span class="waste mono">
    {(wasted / 1024 / 1024).toFixed(0)}<span class="muted"> MB potentially wasted</span>
  </span>
  {#if running && dupJob && dupJob.total}
    <span class="run-status mono">
      {dupJob.processed.toLocaleString()} / {dupJob.total.toLocaleString()}
    </span>
  {/if}
  <button class="primary" onclick={run} disabled={running}>
    {running ? "Scanning…" : "Scan"}
  </button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if groups.length === 0}
    <div class="empty">
      <p>No duplicates yet. Press scan — it takes a moment for big libraries.</p>
      <button class="primary" onclick={run} disabled={running}>
        {running ? "Scanning…" : "Scan"}
      </button>
    </div>
  {:else}
    <ul class="grid">
      {#each groups as g (g.id)}
        <li>
          <!--
            Single click target — straight to the side-by-side compare
            view, where DuplicateDetail's filmstrip lets the user flip
            through every member at full size before deciding which to
            keep. Splitting this into two paths last round was an
            over-correction; the single landing point is what dup
            review actually needs.
          -->
          <a
            class="card-link"
            href="#/duplicate?id={g.id}"
            aria-label="Compare {g.member_count} duplicates"
            use:thumbnailOnVisible={{
              id: g.cover_photo_id ?? 0,
              thumbnailPath: g.cover_thumbnail_path,
              onReady: (path) => patchThumbnail(g.id, path),
            }}
          >
            <span class="thumb">
              {#if g.cover_thumbnail_path}
                <img
                  src={thumbUrl(libraryStore.driveRoot, g.cover_thumbnail_path) ?? ""}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onerror={(e) => ((e.target as HTMLImageElement).style.display = "none")}
                />
              {/if}
              <span class="badge mono">{g.member_count}×</span>
            </span>
          </a>
        </li>
      {/each}
    </ul>
    {#if hasMore}
      <div class="more-row">
        <button class="ghost" onclick={loadMore} disabled={loadingMore}>
          {loadingMore ? "Loading..." : "Load more groups"}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .waste {
    font-size: var(--t-sm);
    color: var(--ink);
  }
  .run-status {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    align-items: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.55; }
  .grid {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--s-3);
  }
  .grid li { display: contents; }
  .card-link {
    position: relative;
    display: block;
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    text-decoration: none;
    color: inherit;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .card-link:hover {
    border-color: var(--accent);
    box-shadow: 0 6px 22px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .thumb {
    position: relative;
    aspect-ratio: 1;
    display: block;
  }
  .grid img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .grid .badge {
    position: absolute;
    top: var(--s-2);
    right: var(--s-2);
    background: rgba(0, 0, 0, 0.66);
    color: #fff;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: var(--t-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
    z-index: 1;
  }
  .more-row {
    display: flex;
    justify-content: center;
    padding: var(--s-5) 0 0;
  }
</style>
