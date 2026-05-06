<script lang="ts">
  import { libraryStore } from "../stores/library.svelte";

  interface Props {
    current: string;
  }
  let { current }: Props = $props();

  const items = [
    { path: "/timeline", label: "Timeline" },
    { path: "/people", label: "People" },
    { path: "/albums", label: "Albums" },
    { path: "/memories", label: "Memories" },
    { path: "/search", label: "Search" },
    { path: "/map", label: "Map" },
    { path: "/duplicates", label: "Duplicates" },
    { path: "/bursts", label: "Bursts" },
    { path: "/documents", label: "Documents" },
    { path: "/insights", label: "Insights" },
    { path: "/health", label: "Library health" },
    { path: "/trash", label: "Trash" },
    { path: "/settings", label: "Settings" },
  ];
</script>

<aside class="sidebar">
  <header>
    <strong>PhotoVault</strong>
    <span class="muted">{libraryStore.photoCount.toLocaleString()} photos</span>
  </header>
  <nav>
    {#each items as item}
      <a
        href="#{item.path}"
        class:active={current === item.path}
      >{item.label}</a>
    {/each}
  </nav>
  <footer>
    <button onclick={() => libraryStore.close()} class="ghost">Switch library</button>
  </footer>
</aside>

<style>
  .sidebar {
    width: 220px;
    background: #0f0f12;
    border-right: 1px solid #1f1f22;
    padding: 20px 14px;
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 8px 16px;
    border-bottom: 1px solid #1f1f22;
    margin-bottom: 12px;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }
  nav a {
    padding: 8px 10px;
    border-radius: 6px;
    color: #c8c8cc;
  }
  nav a:hover {
    background: #16161a;
    text-decoration: none;
  }
  nav a.active {
    background: #1d1d22;
    color: #fff;
  }
  footer {
    padding-top: 12px;
    border-top: 1px solid #1f1f22;
  }
  .ghost {
    width: 100%;
    background: transparent;
    border: 1px solid #2a2a2d;
  }
</style>
