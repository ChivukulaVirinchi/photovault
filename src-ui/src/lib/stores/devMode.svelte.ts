/// Tiny localStorage-backed flag that gates the "advanced" /
/// destructive bits of Settings (regenerate thumbnails, reset face
/// clusters, run faces from scratch, refresh all places, bridge URL,
/// library health counters). General users see a clean Settings
/// page; turning on Dev mode reveals the rest.
///
/// No backend involvement on purpose — this is a UX gate, not a
/// security boundary. The destructive commands themselves still
/// require an `OpenLibrary`, so a non-dev user who somehow toggled
/// the flag still can't do real damage outside of their own data.

const STORAGE_KEY = "smriti:devMode";

function load(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

class DevModeStore {
  enabled = $state(load());

  set(value: boolean) {
    this.enabled = value;
    try {
      if (value) localStorage.setItem(STORAGE_KEY, "1");
      else localStorage.removeItem(STORAGE_KEY);
    } catch {
      // localStorage can throw in privacy/incognito modes; tolerate.
    }
  }

  toggle() {
    this.set(!this.enabled);
  }
}

export const devMode = new DevModeStore();
