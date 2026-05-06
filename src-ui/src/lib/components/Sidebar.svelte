<script lang="ts">
  import { libraryStore } from "../stores/library.svelte";
  import { settingsStore } from "../stores/settings.svelte";

  interface Props { current: string }
  let { current }: Props = $props();

  const items: Array<{ path: string; label: string }> = [
    { path: "/timeline",   label: "Timeline"  },
    { path: "/people",     label: "People"    },
    { path: "/albums",     label: "Albums"    },
    { path: "/memories",   label: "Memories"  },
    { path: "/search",     label: "Search"    },
    { path: "/map",        label: "Map"       },
    { path: "/duplicates", label: "Duplicates"},
    { path: "/bursts",     label: "Bursts"    },
    { path: "/documents",  label: "Documents" },
    { path: "/insights",   label: "Insights"  },
    { path: "/health",     label: "Health"    },
    { path: "/trash",      label: "Trash"     },
    { path: "/settings",   label: "Settings"  },
  ];

  function isActive(path: string) {
    if (current === path) return true;
    if (path === "/people"     && current === "/person")    return true;
    if (path === "/albums"     && current === "/album")     return true;
    if (path === "/duplicates" && current === "/duplicate") return true;
    if (path === "/bursts"     && current === "/burst")     return true;
    if (path === "/memories"   && current === "/memory")    return true;
    if (path === "/timeline"   && current === "/photo")     return true;
    return false;
  }

  function shortRoot(p: string | null): string {
    if (!p) return "";
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? p;
  }

  const collapsed = $derived(settingsStore.sidebarCollapsed);
  const theme     = $derived(settingsStore.theme);

  function cycleTheme() {
    const order: Array<"dark" | "light" | "system"> = ["dark", "light", "system"];
    const next = order[(order.indexOf(theme) + 1) % order.length];
    settingsStore.update({ theme: next });
  }

  const themeIcon = $derived(
    theme === "light" ? "sun" : theme === "dark" ? "moon" : "system"
  );
  const themeLabel = $derived(
    theme === "light" ? "Light" : theme === "dark" ? "Dark" : "System"
  );
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
    {#if !collapsed}
      <span class="brand">PhotoVault</span>
    {/if}
  </header>

  <nav>
    {#each items as item (item.path)}
      <a
        href="#{item.path}"
        class:active={isActive(item.path)}
        title={collapsed ? item.label : ""}
      >
        <span class="dot" aria-hidden="true"></span>
        <span class="label">{item.label}</span>
      </a>
    {/each}
  </nav>

  <footer>
    {#if !collapsed && libraryStore.driveRoot}
      <div class="lib" title={libraryStore.driveRoot}>
        <span class="lib-name">{shortRoot(libraryStore.driveRoot)}</span>
        <span class="lib-count mono">
          {libraryStore.photoCount.toLocaleString()}
        </span>
      </div>
    {/if}
    <div class="footer-actions">
      <button
        class="icon-btn"
        onclick={cycleTheme}
        aria-label="Cycle theme — currently {themeLabel}"
        title="Theme: {themeLabel}"
      >
        {#if themeIcon === "sun"}
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <circle cx="7" cy="7" r="2.6" stroke="currentColor" stroke-width="1.4"/>
            <path d="M7 1.2v1.4M7 11.4v1.4M1.2 7h1.4M11.4 7h1.4M2.9 2.9l1 1M10.1 10.1l1 1M2.9 11.1l1-1M10.1 3.9l1-1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
          </svg>
        {:else if themeIcon === "moon"}
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M11.5 8.4A4.6 4.6 0 0 1 5.6 2.5a.4.4 0 0 0-.5-.5 5.5 5.5 0 1 0 6.9 6.9.4.4 0 0 0-.5-.5z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/>
          </svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <rect x="1.5" y="2.4" width="11" height="7.2" rx="0.8" stroke="currentColor" stroke-width="1.3"/>
            <path d="M5 12h4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
          </svg>
        {/if}
      </button>
      {#if !collapsed}
        <button class="ghost switch" onclick={() => libraryStore.close()}>
          Switch library
        </button>
      {/if}
    </div>
  </footer>
</aside>

<style>
  .sidebar {
    width: 220px;
    flex-shrink: 0;
    background: var(--bg-paper);
    border-right: 1px solid var(--line-soft);
    padding: var(--s-4) var(--s-2) var(--s-2);
    display: flex;
    flex-direction: column;
    height: 100vh;
    position: relative;
    transition: width var(--t-base-d) var(--ease);
  }
  .sidebar.collapsed {
    width: 56px;
    padding: var(--s-4) 6px var(--s-2);
  }

  /* ===== header — wordmark only, no strap line ============ */
  header {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-3) var(--s-4);
    margin-bottom: var(--s-2);
    border-bottom: 1px solid var(--line-soft);
    min-height: 28px;
  }
  .sidebar.collapsed header {
    padding: 0 0 var(--s-3);
    justify-content: center;
  }

  .collapse {
    width: 26px;
    height: 26px;
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
  .collapse:hover {
    color: var(--ink);
    background: var(--bg-card);
    border-color: transparent;
  }

  .brand {
    font-family: var(--font-display);
    font-variation-settings: "opsz" 24, "wdth" 100;
    font-weight: 600;
    font-size: var(--t-lg);
    letter-spacing: -0.025em;
    color: var(--ink);
    line-height: 1;
  }

  /* ===== nav — text-led, ochre indicator on active ======= */
  nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    overflow-y: auto;
    padding: var(--s-2) 0;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 8px var(--s-3);
    margin-left: 2px;
    border-radius: var(--r-sm);
    color: var(--ink-muted);
    font-size: var(--t-sm);
    font-weight: 500;
    position: relative;
    transition: background var(--t-fast) var(--ease),
                color      var(--t-fast) var(--ease);
  }
  .sidebar.collapsed nav a {
    justify-content: center;
    padding: 9px 0;
    margin-left: 0;
  }
  nav a:hover {
    background: var(--bg-card);
    color: var(--ink);
  }
  nav a.active {
    color: var(--ink);
    font-weight: 600;
  }
  /* The single signature: 2px ochre indicator on the left edge.
     Static — no draw-in animation. */
  nav a.active::before {
    content: "";
    position: absolute;
    left: -2px;
    top: 6px;
    bottom: 6px;
    width: 2px;
    background: var(--accent);
    border-radius: 1px;
  }
  .sidebar.collapsed nav a.active::before {
    left: -6px;
  }

  /* In collapsed mode, show a small ochre dot for the active item only */
  nav a .dot {
    display: none;
  }
  .sidebar.collapsed nav a .dot {
    display: block;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: currentColor;
    opacity: 0.45;
  }
  .sidebar.collapsed nav a.active .dot {
    background: var(--accent);
    opacity: 1;
    width: 5px;
    height: 5px;
  }
  .sidebar.collapsed nav a .label {
    display: none;
  }

  /* ===== footer — drive name + theme toggle + switch ===== */
  footer {
    padding-top: var(--s-2);
    border-top: 1px solid var(--line-soft);
    margin-top: var(--s-2);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  .sidebar.collapsed footer { align-items: center; }

  .lib {
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
    padding: 0 var(--s-3);
    overflow: hidden;
  }
  .lib-name {
    font-size: var(--t-sm);
    color: var(--ink-soft);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .lib-count {
    font-size: var(--t-xs);
    color: var(--ink-faint);
    flex-shrink: 0;
  }

  .footer-actions {
    display: flex;
    gap: var(--s-2);
    align-items: center;
    padding: 0 var(--s-2);
  }
  .sidebar.collapsed .footer-actions {
    flex-direction: column;
    padding: 0;
  }

  .icon-btn {
    width: 30px;
    height: 30px;
    padding: 0;
    background: transparent;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    color: var(--ink-soft);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: color var(--t-fast) var(--ease),
                border-color var(--t-fast) var(--ease),
                background var(--t-fast) var(--ease);
  }
  .icon-btn:hover {
    color: var(--ink);
    border-color: var(--ink-faint);
    background: var(--bg-card);
  }

  .switch {
    flex: 1;
    font-size: var(--t-sm);
    padding: 6px var(--s-3);
  }
</style>
