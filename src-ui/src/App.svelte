<script lang="ts">
  import { onMount } from "svelte";
  import { libraryStore } from "./lib/stores/library.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import { updates } from "./lib/stores/updates.svelte";
  import Welcome from "./routes/Welcome.svelte";
  import Timeline from "./routes/Timeline.svelte";
  import Search from "./routes/Search.svelte";
  import MapView from "./routes/Map.svelte";
  import Lazy, { preload } from "./lib/components/Lazy.svelte";
  import { isTypingTarget, modalOpen } from "./lib/keys";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastHost from "./lib/components/ToastHost.svelte";
  import JobsIndicator from "./lib/components/JobsIndicator.svelte";
  import ShareCardDialog from "./lib/components/ShareCardDialog.svelte";
  import CollageDialog from "./lib/components/CollageDialog.svelte";
  import MilestoneMoment from "./lib/components/MilestoneMoment.svelte";
  import AssistantDrawer from "./lib/components/AssistantDrawer.svelte";
  import Slideshow from "./lib/components/Slideshow.svelte";
  import Shortcuts from "./routes/Shortcuts.svelte";
  import { jobs } from "./lib/stores/jobs.svelte";
  import { browseContext } from "./lib/stores/browseContext.svelte";
  import { selection } from "./lib/stores/selection.svelte";
  import { photoVisibility } from "./lib/stores/photoVisibility.svelte";
  import { slideshow } from "./lib/stores/slideshow.svelte";
  import { assistantStore } from "./lib/stores/assistant.svelte";
  import { assetSetup } from "./lib/stores/assetSetup.svelte";
  import { library } from "./lib/api/library";
  import SmartSetupDialog from "./lib/components/SmartSetupDialog.svelte";

  const loadPeople = () => import("./routes/People.svelte");
  const loadPersonDetail = () => import("./routes/PersonDetail.svelte");
  const loadFaceReview = () => import("./routes/FaceReview.svelte");
  const loadAlbums = () => import("./routes/Albums.svelte");
  const loadAlbumDetail = () => import("./routes/AlbumDetail.svelte");
  const loadMemories = () => import("./routes/Memories.svelte");
  const loadMemoryDetail = () => import("./routes/MemoryDetail.svelte");
  const loadPhotoDetail = () => import("./routes/PhotoDetail.svelte");
  const loadDuplicates = () => import("./routes/Duplicates.svelte");
  const loadDuplicateDetail = () => import("./routes/DuplicateDetail.svelte");
  const loadBursts = () => import("./routes/Bursts.svelte");
  const loadBurstDetail = () => import("./routes/BurstDetail.svelte");
  const loadTrash = () => import("./routes/Trash.svelte");
  const loadInsights = () => import("./routes/Insights.svelte");
  const loadSettings = () => import("./routes/Settings.svelte");
  const PRIMARY_TABS = new Set([
    "/timeline", "/people", "/albums", "/memories", "/search", "/map",
    "/duplicates", "/bursts", "/insights", "/trash", "/settings",
  ]);

  let route = $state<{ path: string; params: Record<string, string> }>({
    path: "/timeline",
    params: {},
  });

  let showShortcuts = $state(false);
  let lastDriveRoot = $state<string | null | undefined>(undefined);
  let lastSession = $state(-1);
  let lastRouteKey: string | null = null;
  let visitedTabs = $state<Set<string>>(new Set());

  function safeDecode(value: string): string {
    try {
      return decodeURIComponent(value);
    } catch {
      return value;
    }
  }

  function parseHash() {
    const raw = window.location.hash.slice(1);
    const [path, q] = raw.split("?");
    const nextPath = path || "/timeline";
    const nextRouteKey = `${nextPath}?${q ?? ""}`;
    if (lastRouteKey !== null && nextRouteKey !== lastRouteKey) {
      selection.clear();
    }
    lastRouteKey = nextRouteKey;
    const params: Record<string, string> = {};
    if (q) for (const kv of q.split("&")) {
      const [k, v] = kv.split("=");
      params[safeDecode(k)] = safeDecode(v ?? "");
    }
    route = { path: nextPath, params };
  }

  function positiveIntParam(name: string): number | null {
    const raw = route.params[name];
    if (!raw || !/^\d+$/.test(raw)) return null;
    const value = Number(raw);
    return Number.isSafeInteger(value) && value > 0 ? value : null;
  }

  function intParam(name: string): number | null {
    const raw = route.params[name];
    if (!raw || !/^-?\d+$/.test(raw)) return null;
    const value = Number(raw);
    return Number.isSafeInteger(value) ? value : null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (document.querySelector("dialog[open]")) return;
    if (
      (e.ctrlKey || e.metaKey) &&
      e.shiftKey &&
      e.key.toLowerCase() === "a" &&
      settingsStore.data?.ai_features_enabled === true &&
      settingsStore.data?.assistant_enabled !== false
    ) {
      assistantStore.show();
      e.preventDefault();
      return;
    }
    if (isTypingTarget(e.target)) return;
    if (e.key === "?") { showShortcuts = !showShortcuts; e.preventDefault(); }
    else if (showShortcuts) { if (e.key === "Escape") { showShortcuts = false; e.preventDefault(); } }
    else if (modalOpen()) { return; }
    else if (e.key === "/") { focusSearch(); e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && e.key === ",") { window.location.hash = "/settings"; e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && /^[1-9]$/.test(e.key)) {
      const href = tabShortcut(Number(e.key));
      if (href) { window.location.hash = href; e.preventDefault(); }
    }
    else if (e.key === "Escape" && route.path !== "/timeline") {
      // Pages handle Escape first (clear a selection, close a drawer). This
      // listener runs before theirs, so decide once the event has finished.
      setTimeout(() => { if (!e.defaultPrevented && !modalOpen()) history.back(); });
    }
  }

  /** Ctrl+1…9 open the sidebar's pages in order. */
  function tabShortcut(n: number): string | null {
    const links = [...document.querySelectorAll<HTMLAnchorElement>("nav a[data-tab]")];
    return links[n - 1]?.getAttribute("href")?.replace(/^#/, "") ?? null;
  }

  /** "/" goes to Search and puts the cursor in its box, every time. */
  function focusSearch() {
    window.location.hash = "/search";
    const focus = (tries: number) => {
      const input = document.querySelector<HTMLInputElement>(".route-panel.active input[type=search], .route-panel.active input.search-input");
      if (input) { input.focus(); input.select(); }
      else if (tries > 0) requestAnimationFrame(() => focus(tries - 1));
    };
    requestAnimationFrame(() => focus(30));
  }

  // The window title names the current page, which screen readers announce
  // on navigation.
  const PAGE_TITLES: Record<string, string> = {
    "/timeline": "Timeline", "/people": "People", "/albums": "Albums", "/memories": "Memories",
    "/search": "Search", "/map": "Map", "/duplicates": "Duplicates", "/bursts": "Bursts",
    "/insights": "Insights", "/trash": "Trash", "/settings": "Settings", "/photo": "Photo",
    "/album": "Album", "/memory": "Memory", "/person": "Person", "/duplicate": "Duplicates",
    "/burst": "Burst", "/review-faces": "Review faces",
  };
  $effect(() => {
    const page = libraryStore.isOpen ? PAGE_TITLES[route.path] : null;
    document.title = page ? `${page} · Smriti` : "Smriti";
  });

  // Pages are separate chunks; fetch them while the app is idle so opening
  // any of them for the first time doesn't wait on a download.
  $effect(() => {
    if (!libraryStore.isOpen) return;
    const warm = () => preload([
      loadPeople, loadAlbums, loadMemories, loadPhotoDetail, loadAlbumDetail, loadMemoryDetail,
      loadPersonDetail, loadTrash, loadSettings, loadDuplicates, loadBursts, loadInsights,
      loadDuplicateDetail, loadBurstDetail, loadFaceReview,
    ]);
    const handle = window.requestIdleCallback ? window.requestIdleCallback(warm, { timeout: 3000 }) : window.setTimeout(warm, 1500);
    return () => (window.cancelIdleCallback ? window.cancelIdleCallback(handle) : window.clearTimeout(handle));
  });

  onMount(() => {
    parseHash();
    window.addEventListener("hashchange", parseHash);
    window.addEventListener("keydown", onKey);
    libraryStore.refresh();
    settingsStore.load();
    // Single global subscription to all long-job progress events. Per-
    // route progress UI reads from this store, so navigation never
    // loses state — the work was already running in the background.
    jobs.install().catch((e) => console.warn("job event subscription failed", e));
    let stopSetup: (() => void) | undefined;
    let stopReconcile: (() => void) | undefined;
    let disposed = false;
    libraryStore.listenForReconcile().then((stop) => { if (disposed) stop(); else stopReconcile = stop; })
      .catch((e) => console.warn("reconcile subscription failed", e));
    assetSetup.install().then((stop) => { if (disposed) stop(); else stopSetup = stop; })
      .catch((e) => console.warn("asset setup subscription failed", e));
    return () => {
      disposed = true;
      stopSetup?.();
      stopReconcile?.();
      window.removeEventListener("hashchange", parseHash);
      window.removeEventListener("keydown", onKey);
    };
  });

  // Opt-in (off by default): one quiet check per launch.
  $effect(() => {
    if (settingsStore.data?.auto_update_check_enabled === true) updates.scheduleAutoCheck();
  });

  $effect(() => {
    const root = libraryStore.driveRoot;
    const previousRoot = lastDriveRoot;
    if (previousRoot !== undefined && (root !== previousRoot || libraryStore.session !== lastSession)) {
      browseContext.clear();
      selection.clear();
      photoVisibility.clear();
      slideshow.close();
      assistantStore.resetForLibrary();
      jobs.clearLibraryScoped();
      visitedTabs = new Set(PRIMARY_TABS.has(route.path) ? [route.path] : ["/timeline"]);
      // Deliberately no route redirect here.
      //
      // This effect also fires on the *first* hydration: libraryStore goes
      // from the initial `driveRoot: null, session: 0` to whatever
      // `library_current` reports, which is a "change" by this comparison.
      // It used to force `#/timeline` in that case, so any deep link —
      // `#/map`, `#/photo?id=…`, an album permalink — was silently thrown
      // away a few hundred milliseconds after load. Opening a library is a
      // route-preserving action; the `{#key}` block below already remounts
      // the shell against the new driveRoot/session.
      //
      // Closing a library needs no redirect either: `isOpen` flips false and
      // the `{#if}` below swaps to Welcome regardless of the current path.
    }
    lastDriveRoot = root;
    lastSession = libraryStore.session;
  });

  $effect(() => {
    const path = route.path;
    if (!libraryStore.isOpen || !PRIMARY_TABS.has(path) || visitedTabs.has(path)) return;
    visitedTabs = new Set([...visitedTabs, path]);
  });

  $effect(() => {
    jobs.jobs;
    libraryStore.session;
    assetSetup.resume();
  });

  // When ingest and ML work goes idle, process whatever is pending with
  // the smart features already installed. Edge-triggered (busy → idle,
  // or a newly opened library) so the frequent thumbnail progress events
  // don't keep pushing the timer back.
  const PROCESSING_KINDS = ["scan", "metadata", "takeout", "faces", "semantic", "assets"] as const;
  let processingWasBusy = true;
  let processingSession = -1;
  let processingTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    jobs.jobs;
    const session = libraryStore.session;
    if (!libraryStore.isOpen) return;
    if (session !== processingSession) {
      processingSession = session;
      processingWasBusy = true;
    }
    const busy = libraryStore.scanPending || PROCESSING_KINDS.some((kind) => jobs.isRunning(kind));
    if (busy) {
      processingWasBusy = true;
      clearTimeout(processingTimer);
      return;
    }
    if (!processingWasBusy) return;
    processingWasBusy = false;
    clearTimeout(processingTimer);
    // The post-scan metadata job starts a moment after the scan job ends;
    // wait so faces aren't detected before orientation is known.
    processingTimer = setTimeout(async () => {
      if (session !== libraryStore.session) return;
      if (await resumeLibraryPasses(session)) return;
      if (session === libraryStore.session) void assetSetup.processPending();
    }, 3000);
  });

  /// Metadata and thumbnail passes cut short by a quit, a pause or an
  /// unplugged drive pick up again on their own: at most once each per
  /// library open, so photos that keep failing can't restart them in a
  /// loop. Returns true while metadata runs, since faces wait for it to
  /// learn each photo's orientation; thumbnails run alongside the rest.
  const resumedPasses = new Set<string>();
  async function resumeLibraryPasses(session: number): Promise<boolean> {
    const passes = [
      { kind: "metadata", pending: library.pendingMetadataCount, start: library.startMetadataExtraction },
      { kind: "thumbnails", pending: library.pendingThumbnailCount, start: library.startThumbnailPass },
    ] as const;
    for (const pass of passes) {
      const key = `${session}:${pass.kind}`;
      if (resumedPasses.has(key) || jobs.isRunning(pass.kind) || jobs.cancelledKinds.has(pass.kind)) continue;
      resumedPasses.add(key);
      try {
        const { pending_photos } = await pass.pending();
        if (pending_photos === 0 || session !== libraryStore.session) continue;
        const { job_id } = await pass.start();
        if (session === libraryStore.session) jobs.register(job_id, pass.kind);
        if (pass.kind === "metadata") return true;
      } catch {
        // Quietly skip; a later scan runs the same passes.
      }
    }
    return false;
  }

</script>

{#if libraryStore.isOpen && !assetSetup.introSeen && assetSetup.ready && !Object.values(assetSetup.ready).every(Boolean) && !libraryStore.scanPending && !jobs.isRunning("scan") && !jobs.isRunning("takeout")}
  <SmartSetupDialog />
{/if}

{#if !libraryStore.isOpen}
  {#if route.path === "/settings"}
    <div class="main no-library-main">
      <Lazy load={loadSettings} title="Settings" />
    </div>
  {:else}
    <Welcome />
  {/if}
{:else}
  {#key `${libraryStore.driveRoot}:${libraryStore.session}`}
    <div class="shell">
      <Sidebar current={route.path} />
      <div class="main">
        {#if visitedTabs.has("/timeline")}
          <div class="route-panel" class:active={route.path === "/timeline"} inert={route.path !== "/timeline"}>
            <Timeline revealId={route.path === "/timeline" ? positiveIntParam("photo") : null} />
          </div>
        {/if}
        {#if visitedTabs.has("/people")}
          <div class="route-panel" class:active={route.path === "/people"} inert={route.path !== "/people"}><Lazy load={loadPeople} title="People" /></div>
        {/if}
        {#if visitedTabs.has("/albums")}
          <div class="route-panel" class:active={route.path === "/albums"} inert={route.path !== "/albums"}><Lazy load={loadAlbums} title="Albums" /></div>
        {/if}
        {#if visitedTabs.has("/memories")}
          <div class="route-panel" class:active={route.path === "/memories"} inert={route.path !== "/memories"}><Lazy load={loadMemories} title="Memories" /></div>
        {/if}
        {#if visitedTabs.has("/search")}
          <div class="route-panel" class:active={route.path === "/search"} inert={route.path !== "/search"}><Search initialQuery={route.params.q ?? ""} /></div>
        {/if}
        {#if visitedTabs.has("/map")}
          <div class="route-panel" class:active={route.path === "/map"} inert={route.path !== "/map"}><MapView active={route.path === "/map"} /></div>
        {/if}
        {#if visitedTabs.has("/duplicates")}
          <div class="route-panel" class:active={route.path === "/duplicates"} inert={route.path !== "/duplicates"}><Lazy load={loadDuplicates} title="Duplicates" /></div>
        {/if}
        {#if visitedTabs.has("/bursts")}
          <div class="route-panel" class:active={route.path === "/bursts"} inert={route.path !== "/bursts"}><Lazy load={loadBursts} title="Bursts" /></div>
        {/if}
        {#if visitedTabs.has("/insights")}
          <div class="route-panel" class:active={route.path === "/insights"} inert={route.path !== "/insights"}><Lazy load={loadInsights} title="Insights" /></div>
        {/if}
        {#if visitedTabs.has("/trash")}
          <div class="route-panel" class:active={route.path === "/trash"} inert={route.path !== "/trash"}><Lazy load={loadTrash} title="Trash" /></div>
        {/if}
        {#if visitedTabs.has("/settings")}
          <div class="route-panel" class:active={route.path === "/settings"} inert={route.path !== "/settings"}><Lazy load={loadSettings} title="Settings" /></div>
        {/if}

        {#if route.path === "/photo" && positiveIntParam("id") != null}
          <Lazy load={loadPhotoDetail} title="Photo" props={{ id: positiveIntParam("id")!, info: route.params.info === "1" }} />
        {:else if route.path === "/review-faces"}
          <Lazy load={loadFaceReview} title="Review faces" />
        {:else if route.path === "/person" && positiveIntParam("id") != null}
          <Lazy load={loadPersonDetail} title="Person" props={{ id: positiveIntParam("id")! }} />
        {:else if route.path === "/album" && intParam("id") != null}
          <Lazy load={loadAlbumDetail} title="Album" props={{ id: intParam("id")! }} />
        {:else if route.path === "/memory" && route.params.id}
          <Lazy load={loadMemoryDetail} title="Memory" props={{ id: route.params.id }} />
        {:else if route.path === "/duplicate" && positiveIntParam("id") != null}
          <Lazy load={loadDuplicateDetail} title="Duplicate group" props={{ id: positiveIntParam("id")! }} />
        {:else if route.path === "/burst" && positiveIntParam("id") != null}
          <Lazy load={loadBurstDetail} title="Burst" props={{ id: positiveIntParam("id")! }} />
        {/if}
      </div>
    </div>
  {/key}
{/if}

{#if showShortcuts}
  <Shortcuts onclose={() => (showShortcuts = false)} />
{/if}

<ToastHost />
<JobsIndicator />
<Slideshow />
<AssistantDrawer />
<ShareCardDialog />
<CollageDialog />
<MilestoneMoment />

<style>
  .shell {
    display: flex;
    height: 100vh;
  }
  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .route-panel {
    display: none;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    flex-direction: column;
  }
  .route-panel.active { display: flex; animation: panel-in 160ms var(--ease-out); }
  @keyframes panel-in { from { opacity: 0; } }
  .no-library-main {
    height: 100vh;
    background: var(--bg);
  }
</style>
