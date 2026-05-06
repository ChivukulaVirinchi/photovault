<script lang="ts">
  /// The signature page header — every section opens with the same shape:
  /// an eyebrow (№ NN — SECTION) and a serif display title, optional subtitle.
  ///
  /// This is the unifying visual element across the app. Consistency here is
  /// what makes it feel like one curated journal rather than a UI kit.
  interface Props {
    num: string; // "01", "02", ...
    label: string; // "ALBUMS"
    title: string; // "Your collections"
    subtitle?: string;
  }
  let { num, label, title, subtitle, children }: Props & { children?: any } = $props();
</script>

<header class="page-header">
  <div class="eyebrow">
    <span class="num">№&nbsp;{num}</span>
    <span class="ornament"></span>
    <span>{label}</span>
  </div>
  <div class="title-row">
    <h1>{title}</h1>
    <div class="actions">{@render children?.()}</div>
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
    gap: var(--s-3);
  }
  .title-row {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s-5);
    flex-wrap: wrap;
  }
  .actions {
    display: flex;
    gap: var(--s-2);
    align-items: center;
  }
  h1 {
    font-size: var(--t-4xl);
  }
  @media (max-width: 720px) {
    .page-header { padding: var(--s-5) var(--s-5) var(--s-4); }
    h1 { font-size: var(--t-3xl); }
  }
</style>
