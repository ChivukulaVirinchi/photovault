<script lang="ts">
  import { onMount } from "svelte";
  import { libraryStore } from "./lib/stores/library.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import Welcome from "./routes/Welcome.svelte";
  import Timeline from "./routes/Timeline.svelte";
  import Search from "./routes/Search.svelte";
  import MapView from "./routes/Map.svelte";
  import Lazy from "./lib/components/Lazy.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastHost from "./lib/components/ToastHost.svelte";
  import JobsIndicator from "./lib/components/JobsIndicator.svelte";
  import ShareCardDialog from "./lib/components/ShareCardDialog.svelte";
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

  const loadPeople = () => import("./routes/People.svelte");
  const loadPersonDetail = () => import("./routes/PersonDetail.svelte");
  const loadPersonReview = () => import("./routes/PersonReview.svelte");
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
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (e.key === "?") { showShortcuts = !showShortcuts; e.preventDefault(); }
    else if (e.key === "/") { window.location.hash = "/search"; e.preventDefault(); }
    else if (e.key === "Escape") {
      if (showShortcuts) { showShortcuts = false; }
      else if (document.querySelector('[role="dialog"][aria-modal="true"]')) { return; }
      else if (route.path !== "/timeline") { history.back(); }
    }
  }

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
    return () => {
      window.removeEventListener("hashchange", parseHash);
      window.removeEventListener("keydown", onKey);
    };
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

</script>

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
        {:else if route.path === "/people/review"}
          <Lazy load={loadPersonReview} title="Review people" />
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
  .route-panel.active { display: flex; }
  .no-library-main {
    height: 100vh;
    background: var(--bg);
  }
</style>
