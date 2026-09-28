<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { gridWindow } from "../gridWindow";
  let { count, scrollEl, children }: {
    count: number; scrollEl: HTMLElement | undefined;
    children: Snippet<[number, number]>;
  } = $props();
  let root: HTMLDivElement;
  let width = $state(800);
  let top = $state(0);
  let height = $state(600);
  const window = $derived(gridWindow(count, width, top, height));
  onMount(() => {
    const el = scrollEl;
    if (!el) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      width = root.clientWidth;
      top = el.getBoundingClientRect().top - root.getBoundingClientRect().top;
      height = el.clientHeight;
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(measure); };
    const observer = new ResizeObserver(schedule);
    observer.observe(root);
    observer.observe(el);
    // Earlier result sections can change height independently of this grid.
    const mutation = new MutationObserver(schedule);
    mutation.observe(el, { childList: true, subtree: true });
    el.addEventListener("scroll", schedule, { passive: true });
    measure();
    return () => {
      observer.disconnect(); mutation.disconnect();
      el.removeEventListener("scroll", schedule);
      if (frame) cancelAnimationFrame(frame);
    };
  });
</script>

<div bind:this={root} style:padding-top={window.paddingTop + "px"} style:padding-bottom={window.paddingBottom + "px"}>
  <div class="virtual-grid" style:grid-template-columns={`repeat(${window.columns}, minmax(0, 1fr))`}>
    {@render children(window.start, window.end)}
  </div>
</div>
<style>
  .virtual-grid { display: grid; gap: 4px; }
</style>
