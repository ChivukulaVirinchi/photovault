import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/// One running long-task. The same struct is rebuilt as progress
/// events arrive; the store keeps a Map keyed by `id` so navigating
/// away and back never loses the running state.
export interface Job {
  id: string;
  kind: JobKind;
  /// Human label for the indicator: "Scanning library", "Detecting bursts", etc.
  title: string;
  /// Optional sub-text from the most recent progress event.
  message: string | null;
  /// Latest processed / total counts where available.
  processed: number;
  total: number | null;
  /// Wall-clock ms since the job started (set from the event's `elapsed_ms`).
  elapsed_ms: number;
  status: "running" | "complete" | "error";
}

export type JobKind =
  | "scan"
  | "faces"
  | "duplicates"
  | "bursts"
  | "documents"
  | "thumbnails"
  | "geocoding";

const KIND_TITLE: Record<JobKind, string> = {
  scan:       "Scanning library",
  faces:      "Finding faces",
  duplicates: "Detecting duplicates",
  bursts:     "Detecting bursts",
  documents:  "Classifying documents",
  thumbnails: "Generating thumbnails",
  geocoding:  "Resolving places",
};

class JobsStore {
  jobs = $state<Map<string, Job>>(new Map());
  private installed = false;
  private unlisten: UnlistenFn[] = [];

  /// Active (still-running) jobs in stable insertion order. Completed
  /// entries linger for ~3s so the user sees the success flash.
  active = $derived.by(() => {
    return Array.from(this.jobs.values()).filter((j) => j.status === "running");
  });

  count = $derived(this.active.length);

  /// Subscribe once at app mount. Idempotent.
  async install() {
    if (this.installed) return;
    this.installed = true;
    type Wire = {
      job_id: string;
      stage?: string;
      processed?: number;
      total?: number | null;
      elapsed_ms?: number;
      message?: string | null;
      // scan-specific
      files_processed?: number;
      files_found?: number;
      current_file?: string;
      is_complete?: boolean;
    };
    const handle = (kind: JobKind, complete: boolean) => (e: { payload: Wire }) => {
      const p = e.payload;
      // Coalesce different progress shapes into one Job.
      const processed = p.files_processed ?? p.processed ?? 0;
      const total =
        p.files_found != null ? p.files_found : (p.total ?? null);
      const message = p.message ?? p.current_file ?? null;
      const id = p.job_id;
      const next = new Map(this.jobs);
      const prev = next.get(id);
      next.set(id, {
        id,
        kind,
        title: prev?.title ?? KIND_TITLE[kind],
        message,
        processed,
        total: total != null ? total : null,
        elapsed_ms: p.elapsed_ms ?? prev?.elapsed_ms ?? 0,
        status: complete ? "complete" : "running",
      });
      this.jobs = next;
      if (complete) {
        // Linger briefly so the user sees the run finish, then evict.
        setTimeout(() => this.dismiss(id), 2500);
      }
    };
    const subs: Array<[string, JobKind, boolean]> = [
      ["scan:progress",       "scan",       false],
      ["scan:complete",       "scan",       true ],
      ["faces:progress",      "faces",      false],
      ["faces:complete",      "faces",      true ],
      ["duplicates:progress", "duplicates", false],
      ["duplicates:complete", "duplicates", true ],
      ["bursts:progress",     "bursts",     false],
      ["bursts:complete",     "bursts",     true ],
      ["documents:progress",  "documents",  false],
      ["documents:complete",  "documents",  true ],
      ["thumbnails:progress", "thumbnails", false],
      ["geocoding:progress",  "geocoding",  false],
    ];
    const unlistens = await Promise.all(
      subs.map(([ev, kind, done]) => listen<Wire>(ev, handle(kind, done))),
    );
    this.unlisten = unlistens;
  }

  /// Manually register a job whose start was triggered by a button
  /// click — gives the indicator something to show before the first
  /// progress event arrives.
  register(id: string, kind: JobKind) {
    if (!id) return;
    if (this.jobs.has(id)) return;
    const next = new Map(this.jobs);
    next.set(id, {
      id,
      kind,
      title: KIND_TITLE[kind],
      message: "starting…",
      processed: 0,
      total: null,
      elapsed_ms: 0,
      status: "running",
    });
    this.jobs = next;
  }

  /// True if a job of this kind is currently running. Lets a tab show
  /// "Scanning…" disabled state without tracking the id locally.
  isRunning(kind: JobKind): boolean {
    for (const j of this.jobs.values()) {
      if (j.kind === kind && j.status === "running") return true;
    }
    return false;
  }

  dismiss(id: string) {
    if (!this.jobs.has(id)) return;
    const next = new Map(this.jobs);
    next.delete(id);
    this.jobs = next;
  }

}

/// Compute a rough ETA in ms based on processed/total + elapsed_ms.
/// Returns null if total is unknown or processed is 0.
export function etaMs(j: Job): number | null {
  if (j.total == null || j.total <= 0 || j.processed <= 0) return null;
  const remaining = Math.max(0, j.total - j.processed);
  return Math.round((j.elapsed_ms / j.processed) * remaining);
}

export const jobs = new JobsStore();
