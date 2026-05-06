<script lang="ts">
  import { libraryStore } from "../stores/library.svelte";
  import { settingsStore } from "../stores/settings.svelte";

  interface Props { current: string }
  let { current }: Props = $props();

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

  const collapsed = $derived(settingsStore.sidebarCollapsed);
</script>

<aside class="sidebar" class:collapsed>
  <header>
    <button
      class="collapse"
      onclick={() => settingsStore.toggleSidebar()}
      aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      title={collapsed ? "Expand" : "Collapse"}
    >
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
        {#if collapsed}
          <path d="M5 3L9 7L5 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        {:else}
          <path d="M9 3L5 7L9 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        {/if}
      </svg>
    </button>

    <div class="brand" class:hide={collapsed}>
      <span class="masthead">PhotoVault</span>
      <span class="strap mono">LOCAL · PRIVATE · YOURS</span>
    </div>
    {#if !collapsed}
      <div class="lib">
        <span class="lib-label mono">CURRENTLY READING</span>
        <strong title={libraryStore.driveRoot ?? ""}>
          {shortRoot(libraryStore.driveRoot)}
        </strong>
        <span class="muted small mono">
          {libraryStore.photoCount.toLocaleString()} photos
        </span>
      </div>
    {/if}
  </header>

  <nav>
    {#each items as item, i (item.path)}
      <a
        href="#{item.path}"
        class:active={isActive(item.path)}
        title={collapsed ? item.label : ""}
        style="--i: {i}"
      >
        <span class="n mono">{item.n}</span>
        <span class="label">{item.label}</span>
        <span class="rule" aria-hidden="true"></span>
      </a>
    {/each}
  </nav>

  <footer>
    {#if !collapsed}
      <button class="ghost" onclick={() => libraryStore.close()}>
        Switch library
      </button>
    {/if}
  </footer>
</aside>

<style>
  .sidebar {
    width: 252px;
    flex-shrink: 0;
    background: var(--bg-paper);
    border-right: 1px solid var(--line-soft);
    padding: var(--s-5) var(--s-3) var(--s-3);
    display: flex;
    flex-direction: column;
    height: 100vh;
    position: relative;
    transition: width var(--t-base-d) var(--ease);
  }
  .sidebar.collapsed {
    width: 64px;
    padding: var(--s-5) var(--s-2) var(--s-3);
  }

  header {
    padding: 0 var(--s-3) var(--s-4);
    border-bottom: 1px solid var(--line-soft);
    margin-bottom: var(--s-4);
    position: relative;
  }
  .sidebar.collapsed header { padding: 0 0 var(--s-4); border-bottom: none; }

  .collapse {
    position: absolute;
    top: 0;
    right: var(--s-2);
    width: 28px;
    height: 28px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    color: var(--ink-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: color var(--t-fast) var(--ease),
                background var(--t-fast) var(--ease);
  }
  .sidebar.collapsed .collapse {
    position: static;
    margin: 0 auto var(--s-3);
  }
  .collapse:hover { color: var(--ink); background: var(--bg-card); }

  .brand {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: var(--s-4);
    transition: opacity var(--t-fast) var(--ease);
  }
  .brand.hide { display: none; }
  .masthead {
    font-family: var(--font-display);
    font-weight: 600;
    font-variation-settings: "opsz" 24, "wdth" 100;
    font-size: var(--t-2xl);
    letter-spacing: -0.025em;
    line-height: 1;
    color: var(--ink);
  }
  .strap {
    font-size: 9.5px;
    letter-spacing: 0.16em;
    color: var(--ink-faint);
  }

  .lib {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .lib-label {
    font-size: 9.5px;
    letter-spacing: 0.16em;
    color: var(--ink-faint);
  }
  .lib strong {
    font-family: var(--font-display);
    font-variation-settings: "opsz" 18;
    font-weight: 500;
    font-size: var(--t-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small { font-size: var(--t-xs); }

  nav {
    display: flex;
    flex-direction: column;
    gap: 0;
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--s-1);
  }
  nav a {
    display: grid;
    grid-template-columns: 28px 1fr;
    align-items: center;
    gap: var(--s-3);
    padding: 9px var(--s-3);
    border-radius: var(--r-sm);
    color: var(--ink-soft);
    transition: background var(--t-fast) var(--ease),
                color var(--t-fast) var(--ease);
    position: relative;
  }
  .sidebar.collapsed nav a {
    grid-template-columns: 1fr;
    justify-items: center;
    padding: 10px 0;
  }
  nav a:hover {
    background: var(--bg-card);
    color: var(--ink);
    text-decoration: none;
  }
  nav a.active {
    color: var(--ink);
  }
  nav a.active .rule {
    transform: scaleX(1);
  }

  nav a .n {
    font-size: 10px;
    color: var(--ink-faint);
    letter-spacing: 0.05em;
  }
  nav a.active .n { color: var(--accent); }

  nav a .label {
    font-size: var(--t-sm);
    font-weight: 500;
    transition: opacity var(--t-fast) var(--ease);
  }
  .sidebar.collapsed nav a .label { display: none; }

  /* The signature: a thin lemon line drawing in across the active item */
  nav a .rule {
    position: absolute;
    left: var(--s-3);
    right: var(--s-3);
    bottom: 6px;
    height: 1px;
    background: var(--accent);
    transform: scaleX(0);
    transform-origin: left center;
    transition: transform var(--t-slow) var(--ease-out);
  }
  .sidebar.collapsed nav a .rule { left: 8px; right: 8px; }

  footer {
    padding-top: var(--s-3);
    border-top: 1px solid var(--line-soft);
    margin-top: var(--s-3);
  }
  .sidebar.collapsed footer { display: none; }
  footer button { width: 100%; }
</style>
