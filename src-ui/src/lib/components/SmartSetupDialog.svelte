<script lang="ts">
  import { onMount } from "svelte";
  import { systemEx, type AssetInventory } from "../api/all";
  import type { AssetHealthDto } from "../api/types";
  import { commandErrorMessage } from "../api";
  import { jobs } from "../stores/jobs.svelte";
  import { toasts } from "../stores/toast.svelte";

  let dialog: HTMLDialogElement;
  let mounted = true;
  let assetHealth = $state<AssetHealthDto | null>(null);
  let assetInventory = $state<AssetInventory | null>(null);
  let assetError = $state<string | null>(null);
  let assetHealthLoading = $state(false);
  const installingAssets = $derived(jobs.isRunning("assets"));
  const assetsJob = $derived(jobs.byKind("assets"));
  const semanticAssetIds = [
    "vision.semantic.visual",
    "vision.semantic.text",
    "vision.semantic.tokenizer",
    "vision.semantic.preprocess",
    "vision.semantic.config",
  ];
  const smartFeaturesReady = $derived(Boolean(
    assetHealth &&
      !assetHealth.missing_face_models &&
      !assetHealth.missing_onnx_runtime &&
      !assetHealth.missing_geonames_db &&
      semanticAssetIds.every((id) =>
        assetInventory?.assets.some((asset) => asset.id === id && asset.active),
      ),
  ));
  let handledAssetJobId: string | null = null;


  async function loadAssetHealth() {
    assetHealthLoading = true;
    try {
      const [health, inventory] = await Promise.all([
        systemEx.assetHealth(),
        systemEx.assetsInventory(),
      ]);
      if (mounted) {
        assetHealth = health;
        assetInventory = inventory;
        assetError = null;
      }
    } catch (error) {
      if (mounted) assetError = commandErrorMessage(error);
    } finally {
      if (mounted) assetHealthLoading = false;
    }
  }

  async function installAssets() {
    if (installingAssets) return;
    assetError = null;
    const placeholderId = `pending-assets-${Date.now()}`;
    jobs.register(placeholderId, "assets");
    try {
      const result = await systemEx.installAssets();
      jobs.dismiss(placeholderId);
      jobs.register(result.job_id, "assets");
    } catch (error) {
      jobs.dismiss(placeholderId);
      if (mounted) assetError = commandErrorMessage(error);
    }
  }


  onMount(() => {
    mounted = true;
    void loadAssetHealth().then(() => {
      if (!mounted) return;
      if (assetError) toasts.error(`Couldn't check smart features: ${assetError}`);
      else if (!smartFeaturesReady && !installingAssets) dialog.showModal();
    });
    return () => { mounted = false; };
  });

  $effect(() => {
    const job = assetsJob;
    if (!job || job.id === handledAssetJobId) return;
    if (job.status === "complete" || job.status === "error") {
      handledAssetJobId = job.id;
      if (job.status === "error") {
        assetError = job.message || "Asset installation failed.";
      } else {
        void loadAssetHealth().then(() => { if (mounted && smartFeaturesReady) dialog.close(); });
      }
    }
  });


</script>

<dialog bind:this={dialog} aria-labelledby="setup-title" aria-describedby="setup-description" onkeydown={(event) => event.stopPropagation()}>
  <h2 id="setup-title" class="display">Enable faces, places, and visual search</h2>
  <p id="setup-description">One click, about 1.8 GB downloaded once. Everything stays on this computer.</p>
  {#if assetError}<p class="error" role="alert">{assetError}</p>{/if}
  <div class="actions">
    <button class="ghost" onclick={() => dialog.close()}>{installingAssets ? "Continue browsing" : "Not now"}</button>
    <button class="primary" onclick={installAssets} disabled={installingAssets || assetHealthLoading}>
      {installingAssets ? "Setting things up…" : "Set up smart features"}
    </button>
  </div>
</dialog>

<style>
  dialog { margin: auto; width: min(440px, calc(100vw - 32px)); padding: var(--s-6); border: 1px solid var(--line); border-radius: var(--r-md); background: var(--bg-paper); color: var(--ink); }
  dialog::backdrop { background: rgb(0 0 0 / 45%); }
  h2 { margin: 0; font-size: var(--t-xl); }
  p { margin: var(--s-3) 0 var(--s-5); color: var(--ink-soft); font-size: var(--t-sm); line-height: 1.6; }
  .error { color: var(--hot); }
  .actions { display: flex; justify-content: flex-end; gap: var(--s-2); flex-wrap: wrap; }
</style>
