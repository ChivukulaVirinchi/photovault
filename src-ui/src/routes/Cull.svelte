<script lang="ts">
  // Interactive cull deck (drag-through stack with keyboard nav,
  // mark-for-trash, undo, batch confirm) is a polish-pass deliverable.
</script>

<main class="cull">
  <div class="frame">
    <h2>Quick keep-or-trash deck</h2>
    <p>Coming in the polish pass — a focused review, one photo at a time.</p>
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
    border: 1px dashed var(--line);
    border-radius: var(--r-md);
    padding: var(--s-7) var(--s-6);
    text-align: center;
    max-width: 440px;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    align-items: center;
  }
  h2 {
    font-size: var(--t-2xl);
    font-weight: 600;
    color: var(--ink);
  }
  p {
    color: var(--ink-soft);
    line-height: 1.55;
    max-width: 36ch;
  }
</style>
