import { listen } from "@tauri-apps/api/event";
import { commandErrorMessage } from "../api";
import { systemEx, type UpdateStatus } from "../api/all";
import { toasts } from "./toast.svelte";

/** Wait after launch before the automatic check, so it never competes
 *  with opening the library. */
const AUTO_CHECK_DELAY_MS = 20_000;

/// Update check + install, shared by the quiet launch check and Settings.
class UpdatesStore {
  status = $state<UpdateStatus | null>(null);
  checking = $state(false);
  installing = $state(false);
  /** 0..1 while the new version downloads. */
  progress = $state<number | null>(null);
  restartReady = $state(false);
  error = $state<string | null>(null);
  private autoScheduled = false;

  async check(): Promise<UpdateStatus | null> {
    if (this.checking) return this.status;
    this.checking = true;
    this.error = null;
    try {
      this.status = await systemEx.updatesCheck();
      return this.status;
    } catch (e) {
      this.error = commandErrorMessage(e);
      return null;
    } finally {
      this.checking = false;
    }
  }

  /// Once per launch, only when the user turned automatic checks on.
  /// Failures stay silent: being offline is the normal case.
  scheduleAutoCheck() {
    if (this.autoScheduled) return;
    this.autoScheduled = true;
    setTimeout(async () => {
      try {
        const status = await systemEx.updatesCheck();
        this.status = status;
        if (status.newer_available && status.latest) {
          toasts.show({
            message: `Smriti ${status.latest} is available. Update from Settings.`,
            durationMs: 8000,
          });
        }
      } catch {
        // Offline or rate-limited; the next launch tries again.
      }
    }, AUTO_CHECK_DELAY_MS);
  }

  async install() {
    if (this.installing) return;
    this.installing = true;
    this.error = null;
    this.progress = 0;
    const stop = await listen<{ downloaded: number; total: number }>("update:progress", (e) => {
      const { downloaded, total } = e.payload;
      this.progress = total > 0 ? Math.min(1, downloaded / total) : null;
    }).catch(() => undefined);
    try {
      const { outcome } = await systemEx.updateInstall();
      if (outcome === "restart_required") this.restartReady = true;
      else if (outcome === "installer_launched") {
        toasts.info("Installing the update. Smriti will close for a moment.");
      }
    } catch (e) {
      this.error = commandErrorMessage(e);
    } finally {
      stop?.();
      this.installing = false;
      this.progress = null;
    }
  }

  restart() {
    void systemEx.restart();
  }
}

export const updates = new UpdatesStore();
