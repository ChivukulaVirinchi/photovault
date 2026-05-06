<script lang="ts">
  interface Props { onclose: () => void }
  let { onclose }: Props = $props();

  const shortcuts: Array<[string, string]> = [
    ["?", "Toggle this overlay"],
    ["/", "Focus search"],
    ["Esc", "Close overlay / back"],
    ["J / K", "Navigate timeline"],
    ["Y / N", "Same / Different (face review)"],
  ];
</script>

<div class="overlay" onclick={onclose} role="presentation">
  <div class="card" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Keyboard shortcuts">
    <h2>Keyboard shortcuts</h2>
    <ul>
      {#each shortcuts as [key, label]}
        <li><kbd>{key}</kbd> {label}</li>
      {/each}
    </ul>
    <button onclick={onclose}>Close</button>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 50; }
  .card { background: #131316; padding: 28px; border-radius: 12px; min-width: 400px; max-width: 90%; }
  h2 { margin: 0 0 18px; }
  ul { list-style: none; padding: 0; margin: 0 0 18px; display: flex; flex-direction: column; gap: 8px; }
  li { display: flex; gap: 12px; align-items: center; }
  kbd { background: #1d1d22; padding: 4px 10px; border-radius: 4px; min-width: 60px; text-align: center; font-family: monospace; }
</style>
