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
    <h2>Keyboard shortcuts</h2>
    <ul>
      {#each shortcuts as [key, label]}
        <li>
          <kbd>{key}</kbd>
          <span>{label}</span>
        </li>
      {/each}
    </ul>
    <div class="row">
      <button class="ghost" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in oklab, var(--bg) 65%, transparent);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
    animation: fade-in var(--t-base-d) var(--ease-out);
  }
  .card {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    padding: var(--s-6);
    border-radius: var(--r-md);
    min-width: 420px;
    max-width: 90%;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.40),
                0 4px 16px rgba(0, 0, 0, 0.30);
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
  }
  h2 {
    font-size: var(--t-xl);
    font-weight: 600;
    margin: 0;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: var(--s-3);
    align-items: center;
  }
  li span {
    font-size: var(--t-sm);
    color: var(--ink-soft);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--s-2);
  }
</style>
