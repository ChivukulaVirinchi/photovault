<script lang="ts">
  import type { Component } from "svelte";
  import PageHeader from "./PageHeader.svelte";

  // Routes have different props; the caller still owns the correctly shaped
  // props object, while this small boundary only forwards it.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  type RouteComponent = Component<any>;

  interface Props {
    load: () => Promise<{ default: RouteComponent }>;
    props?: Record<string, unknown>;
    title: string;
  }

  let { load, props = {}, title }: Props = $props();
  const pending = $derived(load());
</script>

{#await pending}
  <PageHeader {title} />
  <div class="loading" role="status">Opening {title.toLowerCase()}…</div>
{:then module}
  {@const Route = module.default}
  <Route {...props} />
{:catch error}
  <PageHeader {title} />
  <p class="error">{error instanceof Error ? error.message : String(error)}</p>
{/await}

<style>
  .loading, .error { padding: var(--s-5) var(--s-7); color: var(--ink-muted); }
</style>
