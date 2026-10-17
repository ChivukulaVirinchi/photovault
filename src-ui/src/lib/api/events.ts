import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/// Already-indexed photos reconciled with the disk at the start of a scan.
export interface ScanReconciled {
  job_id: string;
  moved: number;
  removed: number;
  modified: number;
  held_back_removals: number;
  unreadable: number;
}

export const events = {
  onScanReconciled: (cb: (p: ScanReconciled) => void) =>
    listen<ScanReconciled>("scan:reconciled", (e) => cb(e.payload)) as Promise<UnlistenFn>,
};
