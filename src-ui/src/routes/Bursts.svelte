<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { bursts } from "../lib/api/all";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import PageHeader from "../lib/components/PageHeader.svelte";

  let groups = $state<Awaited<ReturnType<typeof bursts.list>>>([]);
  let error = $state<string | null>(null);

  // Detection runs in tokio::spawn on the backend. Local "running"
  // booleans were resetting on remount, so the UI lied about what
  // was happening. Reading from the global jobs store fixes that —
  // the user can navigate away and the button still says "Detecting".
  const burstsJob = $derived(jobs.byKind("bursts"));
  const running = $derived(jobs.isRunning("bursts"));

  async function load() {
    try { groups = await bursts.list(); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function run() {
    if (running) return;
    const placeholderId = `pending-bursts-${Date.now()}`;
    jobs.register(placeholderId, "bursts");
    toasts.success("Detecting bursts — feel free to navigate away.");
    try {
      const r = await bursts.run();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "bursts");
    } catch (e) {
      jobs.dismiss(placeholderId);
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      error = msg;
      toasts.error(`Couldn't start: ${msg}`);
    }
  }

  // Reload list whenever a bursts job completes. (Could also tick on
  // each progress event, but groups don't appear until the writer has
  // committed everything — completion is the right moment.)
  $effect(() => {
    if (burstsJob && burstsJob.status === "complete") load();
  });

  onMount(() => {
    load();
    const unlisten = listen<{ stage?: string }>("bursts:progress", (e) => {
      if (e.payload.stage === "persisted") load();
    });
    return () => {
      void unlisten.then((u) => u());
    };
  });

  function fmtTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleString("en", {
      day: "numeric", month: "short", year: "numeric",
      hour: "numeric", minute: "2-digit",
    });
  }
</script>

<PageHeader title="Bursts">
  <span class="count mono">{groups.length}<span class="muted"> groups</span></span>
  {#if running && burstsJob && burstsJob.total}
    <span class="run-status mono">
      {burstsJob.processed.toLocaleString()} / {burstsJob.total.toLocaleString()}
    </span>
  {/if}
  <button class="primary" onclick={run} disabled={running}>
    {running ? "Detecting…" : "Detect bursts"}
  </button>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if groups.length === 0}
    <div class="empty">
      <p>No burst groups yet. Run detection — it'll look for shots taken in quick succession.</p>
      <button class="primary" onclick={run} disabled={running}>
        {running ? "Detecting…" : "Detect bursts"}
      </button>
    </div>
  {:else}
    <ul class="card-list">
      {#each groups as g (g.id)}
        {@const memberIds = g.member_photo_ids.length > 0 ? g.member_photo_ids : g.cover_photo_ids}
        <li class="burst-card">
          <header class="card-head">
            <a href="#/burst?id={g.id}" class="head-link">
              <span class="when">{fmtTime(g.start_time)}</span>
              <span class="badge mono">{g.photo_count} shots</span>
            </a>
          </header>
          <!--
            Horizontal filmstrip — every thumb is its own link to
            PhotoDetail with `browseContext` scoped to this burst, so
            arrow navigation in the viewer stays inside the group.
            Click on the header opens the burst detail (the existing
            "compare and pick" surface).
          -->
          <div class="strip">
            {#each g.cover_thumbnail_paths as path, i (path + i)}
              {@const pid = g.cover_photo_ids[i]}
              <a
                class="strip-cell"
                href="#/photo?id={pid}"
                onclick={() => browseContext.set(`burst:${g.id}`, memberIds)}
                aria-label="Open photo {pid}"
              >
                <img src={thumbUrl(libraryStore.driveRoot, path) ?? ""} alt="" loading="lazy" />
              </a>
            {/each}
            {#if g.photo_count > g.cover_thumbnail_paths.length}
              <a class="strip-more" href="#/burst?id={g.id}">
                +{g.photo_count - g.cover_thumbnail_paths.length} more
              </a>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .count { font-size: var(--t-sm); color: var(--ink); }
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
  .run-status { font-size: var(--t-sm); color: var(--ink-soft); }

  .card-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }
  .burst-card {
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    transition: border-color var(--t-fast) var(--ease);
  }
  .burst-card:hover { border-color: var(--accent); }
  .card-head {
    border-bottom: 1px solid var(--line-soft);
  }
  .head-link {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    color: inherit;
    text-decoration: none;
  }
  .head-link:hover { background: var(--bg-paper); }
  .burst-card .when {
    font-size: var(--t-base);
    font-weight: 500;
    color: var(--ink);
  }
  .burst-card .badge {
    font-size: var(--t-xs);
    color: var(--ink-muted);
  }
  .strip {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    padding: 2px;
    scrollbar-width: thin;
  }
  .strip-cell {
    flex: 0 0 auto;
    width: 140px;
    height: 140px;
    background: var(--bg-elev);
    overflow: hidden;
    border-radius: var(--r-sm);
    display: block;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .strip-cell:hover {
    filter: brightness(1.06);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .strip-cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .strip-more {
    flex: 0 0 auto;
    align-self: center;
    padding: 0 var(--s-4);
    font-size: var(--t-sm);
    color: var(--ink-muted);
    text-decoration: none;
    white-space: nowrap;
  }
  .strip-more:hover { color: var(--accent); }
</style>
