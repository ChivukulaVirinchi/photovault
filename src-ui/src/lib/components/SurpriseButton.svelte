<script lang="ts">
  import { onDestroy } from "svelte";
  import { Sparkles } from "lucide-svelte";
  import { slideshow } from "../stores/slideshow.svelte";
  import { libraryStore } from "../stores/library.svelte";
  import { toasts } from "../stores/toast.svelte";
  import { commandErrorMessage } from "../api";

  let { albumId = null, label = "Your library", disabled = false }: {
    albumId?: number | null; label?: string; disabled?: boolean;
  } = $props();
  let alive = true;
  onDestroy(() => { alive = false; });

  async function start() {
    const root = libraryStore.driveRoot;
    if (!root) return;
    const session = libraryStore.session;
    const album = albumId;
    const current = () => alive && session === libraryStore.session && album === albumId;
    try {
      const count = await slideshow.surprise(root, album, label, current);
      if (count === 0 && current()) toasts.info("No photos to rediscover here yet.");
    } catch (error) {
      if (current()) toasts.error(`Couldn't start Surprise me: ${commandErrorMessage(error)}`);
    }
  }
</script>

<button class="ghost icon-action" onclick={start} disabled={disabled || slideshow.starting}
  title={slideshow.starting ? "Finding a memory…" : "Surprise me — a slow slideshow"}
  aria-label="Surprise me" aria-busy={slideshow.starting}>
  <Sparkles size={15} strokeWidth={1.8} />
</button>

<style>
  .icon-action {
    width: 32px; height: 32px; padding: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: var(--r-sm); color: var(--ink-soft);
  }
  .icon-action:hover:not(:disabled) { color: var(--ink); background: var(--bg-card); }
  .icon-action:disabled { opacity: 0.45; }
</style>
