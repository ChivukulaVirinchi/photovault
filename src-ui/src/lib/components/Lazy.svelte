<script module lang="ts">
  import type { Component } from "svelte";

  // Routes have different props; the caller still owns the correctly shaped
  // props object, while this small boundary only forwards it.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export type RouteComponent = Component<any>;
  export type RouteLoader = () => Promise<{ default: RouteComponent }>;

  /** Modules already loaded, so revisiting a page renders at once instead
   *  of going through a loading state. */
  const loaded = new Map<RouteLoader, RouteComponent>();

  /** Load pages ahead of time (at idle), so even first visits are instant. */
  export function preload(loaders: RouteLoader[]) {
    for (const load of loaders) {
      if (loaded.has(load)) continue;
      load().then((m) => loaded.set(load, m.default), () => {});
    }
  }
</script>

<script lang="ts">
  import PageHeader from "./PageHeader.svelte";

  interface Props {
    load: RouteLoader;
    props?: Record<string, unknown>;
    title: string;
  }

  let { load, props = {}, title }: Props = $props();
  const ready = $derived(loaded.get(load));
  const pending = $derived(ready ? null : load().then((m) => (loaded.set(load, m.default), m.default)));
</script>

{#if ready}
  {@const Route = ready}
  <Route {...props} />
{:else}
  {#await pending}
    <PageHeader {title} />
    <!-- Only shown if loading is slow; a quick load shows nothing at all. -->
    <div class="loading" role="status" aria-label="Loading"></div>
  {:then Route}
    {#if Route}<Route {...props} />{/if}
  {:catch error}
    <PageHeader {title} />
    <p class="error">{error instanceof Error ? error.message : String(error)}</p>
  {/await}
{/if}

<style>
  .error { padding: var(--s-5) var(--s-7); color: var(--ink-muted); }
  .loading {
    height: 2px;
    margin: 0 var(--s-7);
    background: linear-gradient(90deg, transparent, var(--accent-soft), transparent);
    background-size: 40% 100%;
    background-repeat: no-repeat;
    opacity: 0;
    animation: appear 0s linear 400ms forwards, sweep 1.1s var(--ease) 400ms infinite;
  }
  @keyframes appear { to { opacity: 1; } }
  @keyframes sweep { from { background-position: -40% 0; } to { background-position: 140% 0; } }
  @media (prefers-reduced-motion: reduce) { .loading { animation: appear 0s linear 400ms forwards; } }
</style>
