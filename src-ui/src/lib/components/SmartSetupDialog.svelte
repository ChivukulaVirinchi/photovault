<script lang="ts">
  import { onMount } from "svelte";
  import { assetSetup } from "../stores/assetSetup.svelte";
  import AssetSetup from "./AssetSetup.svelte";
  let element: HTMLDialogElement;
  function dismiss() { assetSetup.dismissIntro(); }
  onMount(() => { element.showModal(); });
</script>

<dialog bind:this={element} aria-label="Enable smart features" aria-modal="true" oncancel={dismiss} onclose={dismiss}>
  <AssetSetup feature="all" dialog onclose={dismiss} />
</dialog>

<style>
  dialog { max-width: 440px; width: calc(100vw - 48px); border: 1px solid var(--line); border-radius: 16px; padding: 28px; background: var(--bg-paper); color: var(--ink); box-shadow: 0 16px 60px #0006; }
  dialog::backdrop { background: #0006; }
</style>
