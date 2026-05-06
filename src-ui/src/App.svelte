<script lang="ts">
  import { onMount } from "svelte";
  import { libraryStore } from "./lib/stores/library.svelte";
  import Welcome from "./routes/Welcome.svelte";
  import Timeline from "./routes/Timeline.svelte";
  import PhotoDetail from "./routes/PhotoDetail.svelte";

  let route = $state<{ path: string; params: Record<string, string> }>({
    path: "/timeline",
    params: {},
  });

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

  onMount(() => {
    parseHash();
    window.addEventListener("hashchange", parseHash);
    libraryStore.refresh();
    return () => window.removeEventListener("hashchange", parseHash);
  });
</script>

{#if !libraryStore.isOpen}
  <Welcome />
{:else if route.path === "/photo"}
  <PhotoDetail id={Number(route.params.id)} />
{:else}
  <Timeline />
{/if}

<style>
</style>
