<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    children?: Snippet;
  }
  let { title, children }: Props = $props();
</script>

<header class="page-header">
  <h2 class="title">{title}</h2>
  {#if children}
    <div class="actions">{@render children()}</div>
  {/if}
</header>

<style>
  .page-header {
    display: flex;
    align-items: center;
    gap: var(--s-5);
    padding: var(--s-4) var(--s-7);
    border-bottom: 1px solid var(--line-soft);
    background: var(--bg);
    flex-shrink: 0;
    min-height: 60px;
  }
  .title {
    font-size: var(--t-xl);
    font-weight: 600;
    letter-spacing: 0;
    color: var(--ink);
    flex: 1;
    min-width: 0;
    line-height: 1.2;
  }
  .actions {
    display: flex;
    gap: var(--s-2);
    align-items: center;
    flex-shrink: 0;
  }
  @media (max-width: 720px) {
    .page-header {
      padding: var(--s-3) var(--s-5);
      gap: var(--s-3);
      min-height: 52px;
    }
  }
</style>
