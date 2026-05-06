<script lang="ts">
  import { libraryStore } from "../stores/library.svelte";

  interface Props { current: string }
  let { current }: Props = $props();

  // Numbered like a magazine table of contents.
  const items: Array<{ path: string; n: string; label: string }> = [
    { path: "/timeline", n: "01", label: "Timeline" },
    { path: "/people", n: "02", label: "People" },
    { path: "/albums", n: "03", label: "Albums" },
    { path: "/memories", n: "04", label: "Memories" },
    { path: "/search", n: "05", label: "Search" },
    { path: "/map", n: "06", label: "Map" },
    { path: "/duplicates", n: "07", label: "Duplicates" },
    { path: "/bursts", n: "08", label: "Bursts" },
    { path: "/documents", n: "09", label: "Documents" },
    { path: "/insights", n: "10", label: "Insights" },
    { path: "/health", n: "11", label: "Library health" },
    { path: "/trash", n: "12", label: "Trash" },
    { path: "/settings", n: "13", label: "Settings" },
  ];

  function isActive(path: string) {
    if (current === path) return true;
    // Detail routes resolve to their parent: /album → /albums, /person → /people, etc.
    if (path === "/people" && current === "/person") return true;
    if (path === "/albums" && current === "/album") return true;
    if (path === "/duplicates" && current === "/duplicate") return true;
    if (path === "/bursts" && current === "/burst") return true;
    if (path === "/memories" && current === "/memory") return true;
    if (path === "/timeline" && current === "/photo") return true;
    return false;
  }

  function shortRoot(p: string | null): string {
    if (!p) return "";
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? p;
  }
</script>

<aside class="sidebar">
  <header>
    <div class="brand">
      <span class="masthead">PhotoVault</span>
      <span class="strap">a personal journal of light</span>
    </div>
    <div class="lib">
      <span class="lib-label">CURRENTLY READING</span>
      <strong title={libraryStore.driveRoot ?? ""}>
        {shortRoot(libraryStore.driveRoot)}
      </strong>
      <span class="muted small mono">
        {libraryStore.photoCount.toLocaleString()} photos
      </span>
    </div>
  </header>

  <nav>
    {#each items as item, i}
      <a
        href="#{item.path}"
        class:active={isActive(item.path)}
        style="--i: {i}"
      >
        <span class="n mono">{item.n}</span>
        <span class="label">{item.label}</span>
        <span class="dot" aria-hidden="true"></span>
      </a>
    {/each}
  </nav>

  <footer>
    <button class="ghost" onclick={() => libraryStore.close()}>
      Switch library
    </button>
  </footer>
</aside>

<style>
  .sidebar {
    width: 252px;
    flex-shrink: 0;
    background: var(--bg-paper);
    border-right: 1px solid var(--line-soft);
    padding: var(--s-6) var(--s-4) var(--s-4);
    display: flex;
    flex-direction: column;
    height: 100vh;
    position: relative;
  }

  /* Decorative vertical typographic mark — the journal's spine */
  .sidebar::before {
    content: "";
    position: absolute;
    top: var(--s-7);
    bottom: var(--s-7);
    right: -1px;
    width: 1px;
    background: linear-gradient(
      180deg,
      transparent 0%,
      var(--line) 20%,
      var(--line) 80%,
      transparent 100%
    );
  }

  header {
    padding: 0 var(--s-3) var(--s-5);
    border-bottom: 1px solid var(--line-soft);
    margin-bottom: var(--s-5);
  }
  .brand {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: var(--s-5);
  }
  .masthead {
    font-family: var(--font-display);
    font-weight: 500;
    font-variation-settings: "opsz" 60, "SOFT" 50, "WONK" 1;
    font-size: var(--t-2xl);
    letter-spacing: -0.025em;
    line-height: 1;
    color: var(--ink);
  }
  .strap {
    font-family: var(--font-display);
    font-style: italic;
    font-variation-settings: "opsz" 12, "SOFT" 100;
    font-weight: 300;
    font-size: var(--t-sm);
    color: var(--ink-muted);
    letter-spacing: 0.01em;
  }
  .lib {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .lib-label {
    font-family: var(--font-mono);
    font-size: 9.5px;
    letter-spacing: 0.16em;
    color: var(--ink-faint);
    text-transform: uppercase;
  }
  .lib strong {
    font-family: var(--font-display);
    font-variation-settings: "opsz" 24;
    font-weight: 500;
    font-size: var(--t-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--s-1);
  }
  nav a {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--s-3);
    padding: 8px var(--s-3);
    border-radius: var(--r-md);
    color: var(--ink-soft);
    transition: background var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
    position: relative;
    opacity: 0;
    animation: rise var(--t-slow) var(--ease-out) forwards;
    animation-delay: calc(var(--i) * 24ms + 60ms);
  }
  nav a:hover {
    background: var(--bg-card);
    color: var(--ink);
    text-decoration: none;
  }
  nav a.active {
    background: var(--bg-card);
    color: var(--ink);
  }
  nav a .n {
    font-size: 10px;
    color: var(--ink-faint);
    width: 22px;
    letter-spacing: 0.05em;
  }
  nav a.active .n { color: var(--accent); }
  nav a .label {
    font-size: var(--t-sm);
    font-weight: 500;
  }
  nav a .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0;
    transform: scale(0.5);
    transition: opacity var(--t-base-d) var(--ease),
                transform var(--t-base-d) var(--ease-out);
  }
  nav a.active .dot {
    opacity: 1;
    transform: scale(1);
  }

  footer {
    padding-top: var(--s-4);
    border-top: 1px solid var(--line-soft);
    margin-top: var(--s-3);
  }
  footer button { width: 100%; }
</style>
