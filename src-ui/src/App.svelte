<script lang="ts">
  import { onMount } from "svelte";
  import { libraryStore } from "./lib/stores/library.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import Welcome from "./routes/Welcome.svelte";
  import Timeline from "./routes/Timeline.svelte";
  import PhotoDetail from "./routes/PhotoDetail.svelte";
  import People from "./routes/People.svelte";
  import PersonDetail from "./routes/PersonDetail.svelte";
  import PersonReview from "./routes/PersonReview.svelte";
  import Albums from "./routes/Albums.svelte";
  import AlbumDetail from "./routes/AlbumDetail.svelte";
  import Search from "./routes/Search.svelte";
  import Memories from "./routes/Memories.svelte";
  import MemoryDetail from "./routes/MemoryDetail.svelte";
  import Duplicates from "./routes/Duplicates.svelte";
  import DuplicateDetail from "./routes/DuplicateDetail.svelte";
  import Bursts from "./routes/Bursts.svelte";
  import BurstDetail from "./routes/BurstDetail.svelte";
  import Trash from "./routes/Trash.svelte";
  // Documents tab deferred — see Sidebar.svelte. Route stub kept
  // commented in case it returns; the engine still classifies content
  // categories silently for the timeline badge.
  // import Documents from "./routes/Documents.svelte";
  import Insights from "./routes/Insights.svelte";
  import Settings from "./routes/Settings.svelte";
  import MapView from "./routes/Map.svelte";
  import Cull from "./routes/Cull.svelte";
  import Shortcuts from "./routes/Shortcuts.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastHost from "./lib/components/ToastHost.svelte";
  import JobsIndicator from "./lib/components/JobsIndicator.svelte";
  import { jobs } from "./lib/stores/jobs.svelte";

  let route = $state<{ path: string; params: Record<string, string> }>({
    path: "/timeline",
    params: {},
  });

  let showShortcuts = $state(false);

  function parseHash() {
    const raw = window.location.hash.slice(1);
    const [path, q] = raw.split("?");
    const params: Record<string, string> = {};
    if (q) for (const kv of q.split("&")) {
      const [k, v] = kv.split("=");
      params[decodeURIComponent(k)] = decodeURIComponent(v ?? "");
    }
    route = { path: path || "/timeline", params };
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (e.key === "?") { showShortcuts = !showShortcuts; e.preventDefault(); }
    else if (e.key === "/") { window.location.hash = "/search"; e.preventDefault(); }
    else if (e.key === "Escape") {
      if (showShortcuts) { showShortcuts = false; }
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
    jobs.install();
    return () => {
      window.removeEventListener("hashchange", parseHash);
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

{#if !libraryStore.isOpen}
  <Welcome />
{:else}
  <div class="shell">
    <Sidebar current={route.path} />
    <div class="main">
      {#if route.path === "/photo"}
        <PhotoDetail id={Number(route.params.id)} />
      {:else if route.path === "/people"}
        <People />
      {:else if route.path === "/people/review"}
        <PersonReview />
      {:else if route.path === "/person"}
        <PersonDetail id={Number(route.params.id)} />
      {:else if route.path === "/albums"}
        <Albums />
      {:else if route.path === "/album"}
        <AlbumDetail id={Number(route.params.id)} />
      {:else if route.path === "/search"}
        <Search initialQuery={route.params.q ?? ""} />
      {:else if route.path === "/memories"}
        <Memories />
      {:else if route.path === "/memory"}
        <MemoryDetail id={route.params.id} />
      {:else if route.path === "/duplicates"}
        <Duplicates />
      {:else if route.path === "/duplicate"}
        <DuplicateDetail id={Number(route.params.id)} />
      {:else if route.path === "/bursts"}
        <Bursts />
      {:else if route.path === "/burst"}
        <BurstDetail id={Number(route.params.id)} />
      {:else if route.path === "/trash"}
        <Trash />
      <!-- {:else if route.path === "/documents"}
        <Documents /> -->
      {:else if route.path === "/insights"}
        <Insights />
      {:else if route.path === "/health"}
        <Settings />
      {:else if route.path === "/settings"}
        <Settings />
      {:else if route.path === "/map"}
        <MapView />
      {:else if route.path === "/cull"}
        <Cull />
      {:else}
        <Timeline />
      {/if}
    </div>
  </div>
{/if}

{#if showShortcuts}
  <Shortcuts onclose={() => (showShortcuts = false)} />
{/if}

<ToastHost />
<JobsIndicator />

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
</style>
