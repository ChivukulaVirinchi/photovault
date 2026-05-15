<script lang="ts">
  interface Props { onclose: () => void }
  let { onclose }: Props = $props();

  type Group = { title: string; rows: Array<[string, string]> };

  const groups: Group[] = [
    {
      title: "Photo viewer",
      rows: [
        ["←", "Previous photo"],
        ["→", "Next photo"],
        ["Esc", "Back to gallery"],
        ["I", "Toggle info panel"],
        ["+ / −", "Zoom in / out"],
        ["0", "Fit to screen"],
        ["1", "Actual size (1:1)"],
        ["[ / ]", "Rotate left / right"],
        ["F", "Toggle fullscreen"],
      ],
    },
    {
      title: "Anywhere",
      rows: [
        ["?", "Toggle this overlay"],
        ["/", "Focus search"],
        ["Esc", "Back / close overlay"],
      ],
    },
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
    {#each groups as group}
      <section>
        <h3 class="group-title">{group.title}</h3>
        <ul>
          {#each group.rows as [key, label]}
            <li>
              <kbd>{key}</kbd>
              <span>{label}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
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
    width: min(460px, calc(100vw - 32px));
    max-width: 90vw;
    max-height: 90vh;
    overflow-y: auto;
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
  section {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .group-title {
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0 0 4px;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: grid;
    grid-template-columns: 96px 1fr;
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
