<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    backHref: string;
    backLabel: string;
    title: Snippet;
    subtitle?: Snippet;
    actions?: Snippet;
  }
  let { backHref, backLabel, title, subtitle, actions }: Props = $props();
</script>

<header class="detail-header">
  <a class="back" href={backHref}>
    <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
      <path d="M7.5 2.5L4 6L7.5 9.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
    <span>{backLabel}</span>
  </a>
  <div class="row">
    <div class="title-wrap">{@render title()}</div>
    {#if actions}
      <div class="actions">{@render actions()}</div>
    {/if}
  </div>
  {#if subtitle}
    <div class="subtitle-wrap">{@render subtitle()}</div>
  {/if}
</header>

<style>
  .detail-header {
    padding: var(--s-4) var(--s-7) var(--s-4);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    flex-shrink: 0;
    background: var(--bg);
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--t-xs);
    color: var(--ink-muted);
    text-decoration: none;
    width: max-content;
    margin-bottom: 2px;
    transition: color var(--t-fast) var(--ease);
  }
  .back:hover { color: var(--ink); }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    justify-content: space-between;
  }
  .title-wrap {
    flex: 1;
    min-width: 0;
  }
  .title-wrap :global(h1) {
    font-size: var(--t-2xl);
    font-weight: 600;
    margin: 0;
    color: var(--ink);
    line-height: 1.15;
    letter-spacing: -0.015em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title-wrap :global(input) {
    font-size: var(--t-xl);
    font-weight: 600;
    color: var(--ink);
    width: 100%;
    max-width: 480px;
  }
  .actions {
    display: flex;
    gap: var(--s-2);
    align-items: center;
    flex-shrink: 0;
  }
  .subtitle-wrap {
    font-size: var(--t-sm);
    color: var(--ink-muted);
    display: flex;
    align-items: center;
    gap: var(--s-3);
    flex-wrap: wrap;
  }

  @media (max-width: 720px) {
    .detail-header { padding: var(--s-3) var(--s-5); }
  }
</style>
