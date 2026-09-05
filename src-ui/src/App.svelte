<script lang="ts">
  import { onMount } from "svelte";
  import { libraryStore } from "./lib/stores/library.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import Welcome from "./routes/Welcome.svelte";
  import Timeline from "./routes/Timeline.svelte";
  import People from "./routes/People.svelte";
  import PersonDetail from "./routes/PersonDetail.svelte";
  import PersonReview from "./routes/PersonReview.svelte";
  import FaceReview from "./routes/FaceReview.svelte";
  import Albums from "./routes/Albums.svelte";
  import AlbumDetail from "./routes/AlbumDetail.svelte";
  import Search from "./routes/Search.svelte";
  import Memories from "./routes/Memories.svelte";
  import MemoryDetail from "./routes/MemoryDetail.svelte";
  import PhotoDetail from "./routes/PhotoDetail.svelte";
  import Duplicates from "./routes/Duplicates.svelte";
  import DuplicateDetail from "./routes/DuplicateDetail.svelte";
  import Bursts from "./routes/Bursts.svelte";
  import BurstDetail from "./routes/BurstDetail.svelte";
  import MapView from "./routes/Map.svelte";
  import Trash from "./routes/Trash.svelte";
  import Insights from "./routes/Insights.svelte";
  import Settings from "./routes/Settings.svelte";
  import Shortcuts from "./routes/Shortcuts.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import AssistantDrawer from "./lib/components/AssistantDrawer.svelte";
  import ToastHost from "./lib/components/ToastHost.svelte";
  import JobsIndicator from "./lib/components/JobsIndicator.svelte";
  import Slideshow from "./lib/components/Slideshow.svelte";
  import { jobs } from "./lib/stores/jobs.svelte";
  import { browseContext } from "./lib/stores/browseContext.svelte";
  import { selection } from "./lib/stores/selection.svelte";
  import { photoVisibility } from "./lib/stores/photoVisibility.svelte";
  import { slideshow } from "./lib/stores/slideshow.svelte";
  import { assistantStore } from "./lib/stores/assistant.svelte";

  let route = $state<{ path: string; params: Record<string, string> }>({
    path: "/timeline",
    params: {},
  });

  let showShortcuts = $state(false);
  let lastDriveRoot = $state<string | null | undefined>(undefined);
  let lastSession = $state(-1);
  let lastRouteKey: string | null = null;

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
      if (previousRoot !== null && root !== null && libraryStore.isOpen && route.path !== "/timeline") {
        window.location.hash = "/timeline";
      }
    }
    lastDriveRoot = root;
    lastSession = libraryStore.session;
  });

</script>

{#if !libraryStore.isOpen}
  {#if route.path === "/settings"}
    <div class="main no-library-main">
      <Settings />
    </div>
  {:else}
    <Welcome />
  {/if}
{:else}
  {#key `${libraryStore.driveRoot}:${libraryStore.session}`}
    <div class="shell">
      <Sidebar current={route.path} />
      <div class="main">
        {#if route.path === "/photo" && positiveIntParam("id") != null}
          <PhotoDetail id={positiveIntParam("id")!} />
        {:else if route.path === "/people"}
          <People />
        {:else if route.path === "/people/review"}
          <PersonReview />
        {:else if route.path === "/review-faces"}
          <FaceReview />
        {:else if route.path === "/person" && positiveIntParam("id") != null}
          <PersonDetail id={positiveIntParam("id")!} />
        {:else if route.path === "/albums"}
          <Albums />
        {:else if route.path === "/album" && intParam("id") != null}
          <AlbumDetail id={intParam("id")!} />
        {:else if route.path === "/search"}
          <Search initialQuery={route.params.q ?? ""} />
        {:else if route.path === "/memories"}
          <Memories />
        {:else if route.path === "/memory" && route.params.id}
          <MemoryDetail id={route.params.id} />
        {:else if route.path === "/duplicates"}
          <Duplicates />
        {:else if route.path === "/duplicate" && positiveIntParam("id") != null}
          <DuplicateDetail id={positiveIntParam("id")!} />
        {:else if route.path === "/bursts"}
          <Bursts />
        {:else if route.path === "/burst" && positiveIntParam("id") != null}
          <BurstDetail id={positiveIntParam("id")!} />
        {:else if route.path === "/trash"}
          <Trash />
        {:else if route.path === "/insights"}
          <Insights />
        {:else if route.path === "/settings"}
          <Settings />
        {:else if route.path === "/map"}
          <MapView />
        {:else}
          <Timeline revealId={positiveIntParam("photo")} />
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
  .no-library-main {
    height: 100vh;
    background: var(--bg);
  }
</style>
