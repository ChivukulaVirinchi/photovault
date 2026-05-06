<script lang="ts">
  /// The signature page header. Every section opens with the same shape:
  /// an editorial section number + label in mono, then a Bricolage display
  /// title with an animated draw-in rule beneath it. Optional subtitle and
  /// action slot.
  ///
  /// The draw-in rule is the unifying gesture across the app — a 1 px
  /// ink-coloured line that scales from 0 → 100% on view enter.
  interface Props {
    num: string;
    label: string;
    title: string;
    subtitle?: string;
  }
  let { num, label, title, subtitle, children }: Props & { children?: any } = $props();
</script>

<header class="page-header">
  <div class="head-top">
    <div class="eyebrow">
      <span class="num">№&nbsp;{num}</span>
      <span class="ornament"></span>
      <span>{label}</span>
    </div>
    <div class="actions">{@render children?.()}</div>
  </div>
  <div class="title-block">
    <h1>{title}</h1>
    <span class="rule" aria-hidden="true"></span>
  </div>
  {#if subtitle}
    <p class="subtitle">{subtitle}</p>
  {/if}
</header>

<style>
  .page-header {
    padding: var(--s-7) var(--s-7) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
  }
  .head-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-5);
    flex-wrap: wrap;
  }
  .actions {
    display: flex;
    gap: var(--s-2);
    align-items: center;
  }
  .title-block {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }
  h1 {
    font-size: var(--t-4xl);
    font-weight: 700;
    font-variation-settings: "opsz" 96, "wdth" 92;
  }
  .rule {
    display: block;
    height: 1px;
    width: 100%;
    background: var(--ink);
    transform: scaleX(0);
    transform-origin: left center;
    animation: draw-in var(--t-slow) var(--ease-out) 80ms forwards;
  }
  .subtitle {
    margin-top: -8px;
  }
  @media (max-width: 720px) {
    .page-header { padding: var(--s-5) var(--s-5) var(--s-4); }
    h1 { font-size: var(--t-3xl); }
  }
</style>
