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
  /// Face-pipeline only: number of streaming-writer flushes so far. The
  /// People page watches this and reloads the cluster grid whenever it
  /// bumps so newly-detected faces appear mid-run.
  chunks_flushed?: number;
  /// Face-pipeline only: cumulative count of faces detected so far.
  faces_found?: number;
  /// Face-pipeline only: "bridge" means embeddings are routed to the
  /// configured cloud GPU bridge for this run; "local" means on-device.
  /// Drives the small status chip in JobsIndicator so the user can
  /// confirm at a glance which path is being used.
  embedder_route?: "local" | "bridge";
}

export type JobKind =
  | "scan"
  | "metadata"
  | "faces"
  | "duplicates"
  | "bursts"
  | "documents"
  | "thumbnails"
  | "assets"
  | "semantic"
  | "geocoding"
  | "albumSuggestions"
  | "albumExport";

const KIND_TITLE: Record<JobKind, string> = {
  scan:             "Indexing files",
  metadata:         "Reading metadata",
  faces:            "Finding faces",
  duplicates:       "Detecting duplicates",
  bursts:           "Detecting bursts",
  documents:        "Classifying documents",
  thumbnails:       "Generating thumbnails",
  assets:           "Installing assets",
  semantic:         "Indexing visual search",
  geocoding:        "Resolving places",
  albumSuggestions: "Looking for trips",
  albumExport:      "Exporting album",
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
      // faces-specific
      chunks_flushed?: number;
      faces_found?: number;
      embedder_route?: string;
      // metadata-extraction-specific
      done?: number;
      // legacy fallbacks: older binaries used these names. Reading them
      // here means the new frontend works against an unrebuilt Rust
      // shell — the field rename in `FacesProgressDto` was breaking the
      // progress bar silently.
      photos_processed?: number;
      total_photos?: number | null;
      faces_detected?: number;
      // bursts/duplicates complete payload
      groups_found?: number;
    };
    const handle = (kind: JobKind, complete: boolean) => (e: { payload: Wire }) => {
      const p = e.payload;
      // Coalesce different progress shapes into one Job.
      const processed =
        p.files_processed ?? p.processed ?? p.photos_processed ?? p.done ?? 0;
      const total =
        kind === "scan" && !complete
          ? null
          : (p.files_found ?? p.total ?? p.total_photos ?? null);
      const facesFound = p.faces_found ?? p.faces_detected ?? 0;
      const message =
        p.message ??
        p.current_file ??
        (complete && p.groups_found != null
          ? `${p.groups_found} group${p.groups_found === 1 ? "" : "s"}`
          : null);
      const id = p.job_id;
      const next = new Map(this.jobs);
      const prev = next.get(id);
      const route =
        p.embedder_route === "bridge" || p.embedder_route === "local"
          ? p.embedder_route
          : prev?.embedder_route;
      next.set(id, {
        id,
        kind,
        title: prev?.title ?? KIND_TITLE[kind],
        message,
        processed,
        total: total != null ? total : null,
        elapsed_ms: p.elapsed_ms ?? prev?.elapsed_ms ?? 0,
        status: complete ? "complete" : "running",
        chunks_flushed: p.chunks_flushed ?? prev?.chunks_flushed ?? 0,
        faces_found: facesFound > 0 ? facesFound : (prev?.faces_found ?? 0),
        embedder_route: route,
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
      ["metadata:progress",   "metadata",   false],
      ["metadata:complete",   "metadata",   true ],
      ["faces:progress",      "faces",      false],
      ["faces:complete",      "faces",      true ],
      ["duplicates:progress", "duplicates", false],
      ["duplicates:complete", "duplicates", true ],
      ["bursts:progress",     "bursts",     false],
      ["bursts:complete",     "bursts",     true ],
      ["documents:progress",  "documents",  false],
      ["documents:complete",  "documents",  true ],
      ["thumbnails:progress", "thumbnails", false],
      ["thumbnails:complete", "thumbnails", true ],
      ["assets:progress",     "assets",     false],
      ["assets:complete",     "assets",     true ],
      ["semantic:progress",   "semantic",   false],
      ["semantic:complete",   "semantic",   true ],
      ["geocoding:progress",  "geocoding",  false],
      ["geocoding:complete",  "geocoding",  true ],
      ["album_suggestions:progress", "albumSuggestions", false],
      ["album_suggestions:complete", "albumSuggestions", true ],
      ["album_export:progress", "albumExport", false],
      ["album_export:complete", "albumExport", true ],
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

  /// Return the most-recent job of this kind (running or just-completed
  /// while it lingers), so a route component can read live counts and
  /// progress without subscribing to events itself.
  byKind(kind: JobKind): Job | null {
    let best: Job | null = null;
    for (const j of this.jobs.values()) {
      if (j.kind !== kind) continue;
      if (best == null || j.elapsed_ms > best.elapsed_ms) best = j;
    }
    return best;
  }

  dismiss(id: string) {
    if (!this.jobs.has(id)) return;
    const next = new Map(this.jobs);
    next.delete(id);
    this.jobs = next;
  }

  markCancelling(id: string) {
    const job = this.jobs.get(id);
    if (!job) return;
    const next = new Map(this.jobs);
    next.set(id, { ...job, message: "Cancelling..." });
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
