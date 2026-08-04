<script lang="ts">
  import {
    Clock, Users, FolderOpen, Sparkles, Search, Map as MapIcon, Bot,
    Copy, Layers, BarChart2, Trash2, Settings,
    ChevronLeft, ChevronRight, Sun, Moon, Monitor, type Icon as IconType,
  } from "lucide-svelte";
  import { libraryStore } from "../stores/library.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { assistantStore } from "../stores/assistant.svelte";

  interface Props { current: string }
  let { current }: Props = $props();

  type Item = { path: string; label: string; icon: typeof IconType };
  const items: Item[] = [
    { path: "/timeline",   label: "Timeline",   icon: Clock      },
    { path: "/people",     label: "People",     icon: Users      },
    { path: "/albums",     label: "Albums",     icon: FolderOpen },
    { path: "/memories",   label: "Memories",   icon: Sparkles   },
    { path: "/search",     label: "Search",     icon: Search     },
    { path: "/assistant",  label: "Assistant",  icon: Bot        },
    { path: "/map",        label: "Map",        icon: MapIcon    },
    { path: "/duplicates", label: "Duplicates", icon: Copy       },
    { path: "/bursts",     label: "Bursts",     icon: Layers     },
    { path: "/insights",   label: "Insights",   icon: BarChart2  },
    { path: "/trash",      label: "Trash",      icon: Trash2     },
    { path: "/settings",   label: "Settings",   icon: Settings   },
  ];

  function isActive(path: string) {
    if (path === "/assistant") return assistantStore.open;
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
  const visibleItems = $derived(
    items.filter(
      (item) =>
        item.path !== "/assistant" ||
        (settingsStore.data?.ai_features_enabled === true &&
          settingsStore.data?.assistant_enabled !== false),
    ),
  );

  function cycleTheme() {
    const order: Array<"dark" | "light" | "system"> = ["dark", "light", "system"];
    const next = order[(order.indexOf(theme) + 1) % order.length];
    settingsStore.update({ theme: next }).catch(() => {});
  }

  const ThemeIcon = $derived(theme === "light" ? Sun : theme === "dark" ? Moon : Monitor);
  const themeLabel = $derived(theme === "light" ? "Light" : theme === "dark" ? "Dark" : "System");

  /// Arrow-key roving focus for nav items. ↑/↓ wrap; Home/End jump to ends.
  /// Enter on a focused link is handled by the browser (anchor activation).
  function onNavKey(e: KeyboardEvent) {
    const nav = e.currentTarget as HTMLElement;
    const links = Array.from(nav.querySelectorAll<HTMLElement>("a, button.nav-action"));
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
    {#if collapsed}
      <!-- Square logomark: lowercase italic "s" + ochre tittle. Uses
           currentColor so the text follows the sidebar's --ink — fixes
           the dark-mode invisibility the external <img> SVG had. -->
      <svg
        class="brand-logomark"
        viewBox="0 0 100 100"
        role="img"
        aria-label="Smriti"
      >
        <text
          x="20"
          y="78"
          font-family="'Cormorant Garamond', Cormorant, 'Iowan Old Style', Garamond, 'Times New Roman', serif"
          font-style="italic"
          font-weight="500"
          font-size="84"
          fill="currentColor"
        >s</text>
        <circle cx="78" cy="32" r="7" fill="#c89968" />
      </svg>
    {:else}
      <div class="brand">
        <svg
          class="brand-wordmark"
          viewBox="0 0 320 100"
          role="img"
          aria-label="Smriti"
        >
          <text
            x="20"
            y="68"
            font-family="'Cormorant Garamond', Cormorant, 'Iowan Old Style', Garamond, 'Times New Roman', serif"
            font-style="italic"
            font-weight="500"
            font-size="64"
            fill="currentColor"
            letter-spacing="0.5"
          >smriti</text>
          <circle cx="195" cy="22" r="6" fill="#c89968" />
        </svg>
        <span class="brand-tagline">Photo library</span>
      </div>
    {/if}
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav onkeydown={onNavKey}>
    {#each visibleItems as item (item.path)}
      {@const Icon = item.icon}
      {#if item.path === "/assistant"}
        <button
          class="nav-action"
          class:active={isActive(item.path)}
          title={collapsed ? item.label : "Assistant (Ctrl+Shift+A)"}
          onclick={() => assistantStore.show()}
        >
          <Icon class="nav-icon" size={16} strokeWidth={1.75} />
          <span class="label">{item.label}</span>
        </button>
      {:else}
        <a
          href="#{item.path}"
          class:active={isActive(item.path)}
          title={collapsed ? item.label : ""}
        >
          <Icon class="nav-icon" size={16} strokeWidth={1.75} />
          <span class="label">{item.label}</span>
        </a>
      {/if}
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

  /* Brand stack — wordmark + small tagline below it.
     Inline <svg> (not <img>) so the text fill picks up `--ink` via
     currentColor and stays readable in dark mode without a separate
     dark-mode asset. */
  .brand {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    line-height: 1;
    color: var(--ink);
  }
  .brand-wordmark {
    height: 28px;
    width: auto;
    display: block;
  }
  .brand-tagline {
    font-size: 9px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ink-muted);
    font-weight: 500;
    margin-left: 2px;
  }
  /* Square logomark for the collapsed sidebar — fits inside the same
     header without the text wordmark's width. */
  .brand-logomark {
    width: 26px;
    height: 26px;
    display: block;
    color: var(--ink);
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    overflow-y: auto;
    padding: var(--s-2) 0;
  }
  nav a,
  nav .nav-action {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 8px var(--s-3);
    margin-left: 2px;
    width: calc(100% - 2px);
    border-radius: var(--r-sm);
    border: 0;
    background: transparent;
    color: var(--ink-muted);
    font-size: var(--t-sm);
    font-weight: 500;
    position: relative;
    text-decoration: none;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease),
                color      var(--t-fast) var(--ease);
  }
  .sidebar.collapsed nav a,
  .sidebar.collapsed nav .nav-action {
    justify-content: center;
    padding: 9px 0;
    margin-left: 0;
    width: 100%;
  }
  nav a :global(.nav-icon),
  nav .nav-action :global(.nav-icon) {
    flex-shrink: 0;
    color: currentColor;
  }
  nav a:hover,
  nav .nav-action:hover {
    background: var(--bg-card);
    color: var(--ink);
  }
  nav a.active,
  nav .nav-action.active {
    color: var(--ink);
    font-weight: 600;
  }
  nav a.active::before,
  nav .nav-action.active::before {
    content: "";
    position: absolute;
    left: -2px;
    top: 6px;
    bottom: 6px;
    width: 2px;
    background: var(--accent);
    border-radius: 1px;
  }
  .sidebar.collapsed nav a.active::before,
  .sidebar.collapsed nav .nav-action.active::before {
    left: -6px;
  }
  .sidebar.collapsed nav a .label,
  .sidebar.collapsed nav .nav-action .label {
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
