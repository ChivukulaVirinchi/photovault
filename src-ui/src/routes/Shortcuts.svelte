<script lang="ts">
  interface Props { onclose: () => void }
  let { onclose }: Props = $props();

  const shortcuts: Array<[string, string]> = [
    ["?", "Toggle this overlay"],
    ["/", "Focus search"],
    ["Esc", "Close overlay / back"],
    ["I", "Toggle photo details"],
    ["J / K", "Navigate timeline (coming)"],
    ["Y / N", "Same / Different (face review, coming)"],
  ];

  function onBackdrop(e: KeyboardEvent | MouseEvent) {
    if (e instanceof KeyboardEvent && e.key !== "Enter" && e.key !== " ") return;
    onclose();
  }
</script>

<div
  class="overlay"
  onclick={onBackdrop}
  onkeydown={onBackdrop}
  role="button"
  tabindex="-1"
  aria-label="Close shortcuts"
>
  <div class="card" role="dialog" aria-label="Keyboard shortcuts">
    <span class="eyebrow">
      <span class="ornament"></span>
      <span>KEYBOARD</span>
    </span>
    <h2>Quick keys.</h2>
    <ul>
      {#each shortcuts as [key, label]}
        <li>
          <kbd>{key}</kbd>
          <span>{label}</span>
        </li>
      {/each}
    </ul>
    <button class="ghost" onclick={onclose}>Close</button>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(10, 7, 5, 0.6);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
    animation: rise var(--t-base-d) var(--ease-out);
  }
  .card {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    padding: var(--s-7) var(--s-6);
    border-radius: var(--r-lg);
    min-width: 480px;
    max-width: 90%;
    box-shadow: var(--shadow-lift);
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
  }
  h2 { font-size: var(--t-2xl); }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  li {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: var(--s-3);
    align-items: center;
  }
  li span {
    font-family: var(--font-display);
    font-size: var(--t-base);
    color: var(--ink-soft);
  }
</style>
