import { settings } from "../api/all";
import type { Settings } from "../api/all";

class SettingsStore {
  data = $state<Settings | null>(null);
  loading = $state(false);
  private seq = 0;
  private systemThemeQuery: MediaQueryList | null = null;
  private systemThemeListener: ((event: MediaQueryListEvent) => void) | null = null;

  async load() {
    const seq = ++this.seq;
    this.loading = true;
    try {
      const next = await settings.get();
      if (seq !== this.seq) return;
      this.data = next;
      this.applyTheme();
    } catch {
      // Best-effort. Without a library opened, settings still works,
      // but if Tauri IPC isn't available we silently fall back.
    } finally {
      if (seq === this.seq) this.loading = false;
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
    this.installSystemThemeListener(t === "system");
  }

  private installSystemThemeListener(enabled: boolean) {
    if (typeof window === "undefined" || !window.matchMedia) return;
    if (!enabled) {
      if (this.systemThemeQuery && this.systemThemeListener) {
        this.systemThemeQuery.removeEventListener("change", this.systemThemeListener);
      }
      this.systemThemeQuery = null;
      this.systemThemeListener = null;
      return;
    }
    if (this.systemThemeQuery && this.systemThemeListener) return;
    const query = window.matchMedia("(prefers-color-scheme: light)");
    const listener = () => {
      if (this.data?.theme === "system") {
        document.documentElement.setAttribute("data-theme", query.matches ? "light" : "dark");
      }
    };
    query.addEventListener("change", listener);
    this.systemThemeQuery = query;
    this.systemThemeListener = listener;
  }

  async update(patch: Partial<Settings>) {
    const seq = ++this.seq;
    // Optimistic local update so UI is instant; backend reconciles.
    const previous = this.data;
    if (this.data) this.data = { ...this.data, ...patch };
    this.applyTheme();
    try {
      const next = await settings.update(patch);
      if (seq !== this.seq) return;
      this.data = next;
      this.applyTheme();
    } catch (e) {
      if (seq === this.seq) {
        this.data = previous;
        this.applyTheme();
      }
      throw e;
    }
  }

  get sidebarCollapsed(): boolean {
    return this.data?.sidebar_collapsed ?? false;
  }

  get theme(): "dark" | "light" | "system" {
    return (this.data?.theme as "dark" | "light" | "system") ?? "dark";
  }

  toggleSidebar() {
    void this.update({ sidebar_collapsed: !this.sidebarCollapsed }).catch(() => {});
  }
}

export const settingsStore = new SettingsStore();
