<script lang="ts">
  import { onMount } from "svelte";
  import { systemEx } from "../api/all";
  import { commandErrorMessage } from "../api";
  import { assetSetup, type AssetFeature } from "../stores/assetSetup.svelte";
  import { jobs } from "../stores/jobs.svelte";

  let { feature, dialog = false, onclose = () => {} }: {
    feature: AssetFeature; dialog?: boolean; onclose?: () => void;
  } = $props();
  const labels = { all: "smart features", faces: "face recognition", visual: "visual search", places: "offline place names" };
  const descriptions = {
    all: "Find people, search by what’s in a photo, and organise photos by place.",
    faces: "Find and group people in your photos. Existing people remain available.",
    visual: "Search by what’s in a photo. Names, dates, places, and albums already work.",
    places: "Identify towns and countries without an internet connection. Your GPS map already works.",
  };
  let bytes = $state<number | null>(null);
  let quoteError = $state<string | null>(null);
  let checking = $state(false);
  let mounted = true;
  const ready = $derived(assetSetup.ready && (feature === "all"
    ? Object.values(assetSetup.ready).every(Boolean) : assetSetup.ready[feature]));
  const error = $derived(assetSetup.errors[feature] ?? (!ready ? assetSetup.errors.all : undefined) ?? quoteError);
  const busy = $derived(assetSetup.requesting || jobs.isRunning("assets"));

  async function quote() {
    checking = true;
    quoteError = null;
    try {
      const size = await systemEx.assetDownloadSize(feature);
      if (mounted) bytes = size;
    } catch (e) { if (mounted) quoteError = commandErrorMessage(e); }
    finally { if (mounted) checking = false; }
  }
  onMount(() => () => { mounted = false; });
  $effect(() => {
    if (assetSetup.ready && (!ready || error) && bytes === null && !checking && !quoteError) void quote();
  });

  function enable() {
    void assetSetup.enable(feature);
    onclose();
  }
</script>

{#if assetSetup.ready && (!ready || error)}
  <section class:dialog aria-label={`Enable ${labels[feature]}`}>
    {#if dialog}<h2>Enable smart features?</h2>{/if}
    <p>{descriptions[feature]}</p>
    <p class="detail">{#if bytes !== null}{bytes === 0 ? "Files are already downloaded." : `${(bytes / 1_000_000).toLocaleString(undefined, { maximumFractionDigits: 0 })} MB one-time download.`}{:else if checking}Checking download size…{/if} Processing stays on your device. You can keep browsing during setup.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      {#if quoteError}
        <button class="ghost" onclick={quote} disabled={checking}>Retry</button>
      {:else}
        <button onclick={enable} disabled={busy || checking || bytes === null}>{busy ? "Setting up…" : error ? "Retry" : `Enable ${labels[feature]}`}</button>
      {/if}
      {#if dialog}<button class="ghost" onclick={onclose}>Later</button>{/if}
    </div>
  </section>
{/if}

<style>
  section { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 4px 16px; align-items: center; margin: var(--s-3) var(--s-7); padding: 12px 16px; border: 1px solid var(--line); border-radius: 12px; background: var(--bg-paper); }
  section.dialog { display: block; margin: 0; padding: 0; border: 0; }
  h2 { margin: 0 0 var(--s-4); }
  p { margin: 0; }
  .dialog p { margin-bottom: var(--s-3); }
  .detail { grid-column: 1; color: var(--ink-muted); font-size: 12px; line-height: 1.6; }
  .error { grid-column: 1 / -1; }
  .actions { grid-column: 2; grid-row: 1 / span 2; display: flex; gap: var(--s-3); align-items: center; }
  @media (max-width: 700px) { .actions { grid-column: 1; grid-row: auto; } }
</style>
