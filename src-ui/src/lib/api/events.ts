import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface ScanProgress {
  job_id: string;
  files_found: number;
  files_processed: number;
  bytes_processed: number;
  current_file: string;
  elapsed_ms: number;
  is_complete: boolean;
  error_count: number;
  error_details: string[];
}

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
  onScanProgress: (cb: (p: ScanProgress) => void) =>
    listen<ScanProgress>("scan:progress", (e) => cb(e.payload)) as Promise<UnlistenFn>,
  onScanComplete: (cb: (p: ScanProgress) => void) =>
    listen<ScanProgress>("scan:complete", (e) => cb(e.payload)) as Promise<UnlistenFn>,
  onScanReconciled: (cb: (p: ScanReconciled) => void) =>
    listen<ScanReconciled>("scan:reconciled", (e) => cb(e.payload)) as Promise<UnlistenFn>,
};
