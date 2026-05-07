<script lang="ts">
  import {
    Clock, Users, FolderOpen, Sparkles, Search, Map as MapIcon,
    Copy, Layers, FileText, BarChart2, Trash2, Settings,
    ChevronLeft, ChevronRight, Sun, Moon, Monitor, type Icon as IconType,
  } from "lucide-svelte";
  import { libraryStore } from "../stores/library.svelte";
  import { settingsStore } from "../stores/settings.svelte";

  interface Props { current: string }
  let { current }: Props = $props();

  type Item = { path: string; label: string; icon: typeof IconType };
  const items: Item[] = [
    { path: "/timeline",   label: "Timeline",   icon: Clock      },
    { path: "/people",     label: "People",     icon: Users      },
    { path: "/albums",     label: "Albums",     icon: FolderOpen },
    { path: "/memories",   label: "Memories",   icon: Sparkles   },
    { path: "/search",     label: "Search",     icon: Search     },
    { path: "/map",        label: "Map",        icon: MapIcon    },
    { path: "/duplicates", label: "Duplicates", icon: Copy       },
    { path: "/bursts",     label: "Bursts",     icon: Layers     },
    { path: "/documents",  label: "Documents",  icon: FileText   },
    { path: "/insights",   label: "Insights",   icon: BarChart2  },
    { path: "/trash",      label: "Trash",      icon: Trash2     },
    { path: "/settings",   label: "Settings",   icon: Settings   },
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

  const ThemeIcon = $derived(theme === "light" ? Sun : theme === "dark" ? Moon : Monitor);
  const themeLabel = $derived(theme === "light" ? "Light" : theme === "dark" ? "Dark" : "System");

  /// Arrow-key roving focus for nav items. ↑/↓ wrap; Home/End jump to ends.
  /// Enter on a focused link is handled by the browser (anchor activation).
  function onNavKey(e: KeyboardEvent) {
    const nav = e.currentTarget as HTMLElement;
    const links = Array.from(nav.querySelectorAll<HTMLAnchorElement>("a"));
    if (links.length === 0) return;
    const here = links.findIndex((a) => a === document.activeElement);
    let next = -1;
    if (e.key === "ArrowDown") next = (here + 1 + links.length) % links.length;
    else if (e.key === "ArrowUp") next = (here - 1 + links.length) % links.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = links.length - 1;
    else return;
    e.preventDefault();
    links[next]?.focus();
  }
</script>

<aside class="sidebar" class:collapsed>
  <header>
    <button
      class="collapse"
      onclick={() => settingsStore.toggleSidebar()}
      aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      title={collapsed ? "Expand" : "Collapse"}
    >
      {#if collapsed}
        <ChevronRight size={14} strokeWidth={1.75} />
      {:else}
        <ChevronLeft size={14} strokeWidth={1.75} />
      {/if}
    </button>
    {#if !collapsed}
      <span class="brand">PhotoVault</span>
    {/if}
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav onkeydown={onNavKey}>
    {#each items as item (item.path)}
      {@const Icon = item.icon}
      <a
        href="#{item.path}"
        class:active={isActive(item.path)}
        title={collapsed ? item.label : ""}
      >
        <Icon class="nav-icon" size={16} strokeWidth={1.75} />
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
        <ThemeIcon size={14} strokeWidth={1.75} />
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
    text-decoration: none;
    transition: background var(--t-fast) var(--ease),
                color      var(--t-fast) var(--ease);
  }
  .sidebar.collapsed nav a {
    justify-content: center;
    padding: 9px 0;
    margin-left: 0;
  }
  nav a :global(.nav-icon) {
    flex-shrink: 0;
    color: currentColor;
  }
  nav a:hover {
    background: var(--bg-card);
    color: var(--ink);
  }
  nav a.active {
    color: var(--ink);
    font-weight: 600;
  }
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
  .sidebar.collapsed nav a .label {
    display: none;
  }

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
