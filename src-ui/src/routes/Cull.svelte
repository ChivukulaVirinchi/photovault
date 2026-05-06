<script lang="ts">
  // Real interactive cull deck (drag-through stack with keyboard nav,
  // mark-for-trash, undo, batch confirm) is a polish-pass deliverable.
</script>

<main class="cull">
  <div class="frame">
    <span class="eyebrow"><span class="ornament"></span>CULL MODE</span>
    <h2>Quick keep-or-trash deck.</h2>
    <p class="subtitle">Coming in the polish pass — a focused review one photo at a time.</p>
  </div>
</main>

<style>
  .cull {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--s-7);
  }
  .frame {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-xl);
    padding: var(--s-8) var(--s-7);
    text-align: center;
    max-width: 480px;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: center;
  }
  h2 { font-size: var(--t-3xl); }
</style>
