<script lang="ts">
  import { trapFocus } from "../lib/actions/trapFocus";

  interface Props { onclose: () => void }
  let { onclose }: Props = $props();

  type Group = { title: string; rows: Array<[string, string]> };

  const groups: Group[] = [
    {
      title: "Anywhere",
      rows: [
        ["Ctrl+1…9", "Go to a page"],
        ["Ctrl+,", "Settings"],
        ["/", "Search"],
        ["Esc", "Back"],
        ["?", "This list"],
      ],
    },
    {
      title: "Photo grids",
      rows: [
        ["Arrows", "Move (Timeline)"],
        ["Enter", "Open photo"],
        ["Ctrl+A", "Select all"],
        ["Esc", "Clear selection"],
        ["A", "Add selection to album"],
        ["Delete", "Trash selection"],
      ],
    },
    {
      title: "Photo viewer",
      rows: [
        ["← / →", "Previous / next"],
        ["I", "Info"],
        ["S", "Favourite"],
        ["A", "Add to album"],
        ["+ / − / 0", "Zoom in / out / fit"],
        ["1", "Actual size"],
        ["[ / ]", "Rotate"],
        ["F", "Fullscreen"],
        ["Delete", "Trash"],
        ["Esc", "Back"],
      ],
    },
    {
      title: "Slideshow",
      rows: [
        ["Space", "Play / pause"],
        ["← / →", "Previous / next"],
        ["F", "Fullscreen"],
        ["Esc", "Close"],
      ],
    },
    {
      title: "Collage",
      rows: [
        ["Ctrl+S", "Save"],
        ["Ctrl+C", "Copy"],
        ["Double-click", "Reset a photo"],
      ],
    },
    {
      title: "Reviewing faces",
      rows: [
        ["Y", "Same person"],
        ["N", "Not this person"],
        ["S", "Skip"],
        ["Esc", "Done"],
      ],
    },
  ];

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onclose(); }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" onclick={(e) => { if (e.target === e.currentTarget) onclose(); }}>
  <div
    class="card pv-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="shortcuts-title"
    tabindex="-1"
    onkeydown={onKey}
    use:trapFocus
  >
    <h2 id="shortcuts-title">Keyboard shortcuts</h2>
    <div class="groups">
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
    </div>
    <div class="row">
      <button class="ghost" onclick={onclose} title="Close (Esc)">Close</button>
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
    width: min(720px, calc(100vw - 32px));
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
  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s-5) var(--s-6);
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
    grid-template-columns: 104px 1fr;
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
