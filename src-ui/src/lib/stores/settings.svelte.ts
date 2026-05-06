import { settings } from "../api/all";
import type { Settings } from "../api/all";

class SettingsStore {
  data = $state<Settings | null>(null);
  loading = $state(false);

  async load() {
    this.loading = true;
    try {
      this.data = await settings.get();
      this.applyTheme();
    } catch {
      // Best-effort. Without a library opened, settings still works,
      // but if Tauri IPC isn't available we silently fall back.
    } finally {
      this.loading = false;
    }
  }

  /// Apply the persisted `theme` to the document root.
  /// Uses `[data-theme="light"]` as the light-mode hook in CSS.
  private applyTheme() {
    if (!this.data) return;
    const t = this.data.theme;
    let resolved: "dark" | "light" = "dark";
    if (t === "light") resolved = "light";
    else if (t === "system") {
      resolved = window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark";
    }
    document.documentElement.setAttribute("data-theme", resolved);
  }

  async update(patch: Partial<Settings>) {
    // Optimistic local update so UI is instant; backend reconciles.
    if (this.data) this.data = { ...this.data, ...patch };
    this.applyTheme();
    try {
      const next = await settings.update(patch);
      this.data = next;
      this.applyTheme();
    } catch {
      // Revert? For now we rely on next load() to reconcile if the user
      // opens settings. Mutations here are low-stakes (theme, sidebar).
    }
  }

  get sidebarCollapsed(): boolean {
    return this.data?.sidebar_collapsed ?? false;
  }

  get theme(): "dark" | "light" | "system" {
    return (this.data?.theme as "dark" | "light" | "system") ?? "dark";
  }

  toggleSidebar() {
    return this.update({ sidebar_collapsed: !this.sidebarCollapsed });
  }
}

export const settingsStore = new SettingsStore();
