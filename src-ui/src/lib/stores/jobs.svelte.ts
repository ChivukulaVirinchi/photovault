import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { routeCache } from "./routeCache.svelte";
import { dataRevision } from "./dataRevision.svelte";

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
  stage?: string;
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
  error_count?: number;
  error_details?: string[];
}

export type JobKind =
  | "scan"
  | "metadata"
  | "faces"
  | "duplicates"
  | "bursts"
  | "thumbnails"
  | "assets"
  | "semantic"
  | "geocoding"
  | "albumSuggestions"
  | "albumExport"
  | "takeout";

const LIBRARY_SCOPED_KINDS = new Set<JobKind>([
  "scan",
  "metadata",
  "faces",
  "duplicates",
  "bursts",
  "thumbnails",
  "geocoding",
  "albumSuggestions",
  "albumExport",
  "takeout",
]);

const KIND_TITLE: Record<JobKind, string> = {
  scan:             "Indexing files",
  metadata:         "Reading metadata",
  faces:            "Finding faces",
  duplicates:       "Detecting duplicates",
  bursts:           "Detecting bursts",
  thumbnails:       "Generating thumbnails",
  assets:           "Installing assets",
  semantic:         "Indexing visual search",
  geocoding:        "Resolving places",
  albumSuggestions: "Looking for trips",
  albumExport:      "Exporting album",
  takeout:          "Importing Google Photos",
};

type WireJobEvent = {
  job_id: string;
  stage?: string;
  processed?: number;
  total?: number | null;
  elapsed_ms?: number;
  message?: string | null;
  files_processed?: number;
  files_found?: number;
  current_file?: string;
  is_complete?: boolean;
  chunks_flushed?: number;
  faces_found?: number;
  embedder_route?: string;
  done?: number;
  photos_processed?: number;
  total_photos?: number | null;
  faces_detected?: number;
  groups_found?: number;
  error_count?: number;
  error_details?: string[];
};

class JobsStore {
  jobs = $state<Map<string, Job>>(new Map());
  private installed = false;
  private installPromise: Promise<void> | null = null;
  private unlisten: UnlistenFn[] = [];
  private suppressedLibraryJobIds = new Set<string>();
  /// Pending "linger then evict" timers, so clearing the library can cancel
  /// them instead of letting them fire against a reset registry.
  private lingerTimers = new Set<ReturnType<typeof setTimeout>>();

  /// Active (still-running) jobs in stable insertion order. Completed
  /// entries linger for ~3s so the user sees the success flash.
  active = $derived.by(() => {
    return Array.from(this.jobs.values()).filter((j) => j.status !== "complete");
  });

  count = $derived(this.active.length);

  /// Subscribe once at app mount. Idempotent.
  async install() {
    if (this.installed) return;
    if (this.installPromise) return this.installPromise;

    this.installPromise = this.installInner();
    try {
      await this.installPromise;
    } finally {
      this.installPromise = null;
    }
  }

  private applyWire(kind: JobKind, complete: boolean, p: WireJobEvent) {
    const isError = p.stage === "error";
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
      (p.error_count ? `${p.error_count} file${p.error_count === 1 ? "" : "s"} could not be indexed` : null) ??
      (complete && p.groups_found != null
        ? `${p.groups_found} group${p.groups_found === 1 ? "" : "s"}`
        : null);
    const id = p.job_id;
    if (!id) return;
    const stage = p.stage;
    if (this.suppressedLibraryJobIds.has(id) && isLibraryScopedWire(kind, id, stage)) {
      if (complete || isError) this.suppressedLibraryJobIds.delete(id);
      return;
    }
    const next = new Map(this.jobs);
    const prev = next.get(id);
    const route =
      p.embedder_route === "bridge" || p.embedder_route === "local"
        ? p.embedder_route
        : prev?.embedder_route;
    const job: Job = {
      id,
      kind,
      title: prev?.title ?? KIND_TITLE[kind],
      message,
      processed,
      total: total != null ? total : null,
      elapsed_ms: p.elapsed_ms ?? prev?.elapsed_ms ?? 0,
      status: isError ? "error" : (complete ? "complete" : "running"),
      stage: stage ?? prev?.stage,
      chunks_flushed: p.chunks_flushed ?? prev?.chunks_flushed ?? 0,
      faces_found: facesFound > 0 ? facesFound : (prev?.faces_found ?? 0),
      embedder_route: route,
      error_count: p.error_count ?? prev?.error_count ?? 0,
      error_details: p.error_details ?? prev?.error_details ?? [],
    };
    next.set(id, job);
    this.jobs = next;
    // A finished job is the main way route data changes without a user
    // mutation — hand the change to the route cache's invalidation matrix.
    if (complete && !isError) {
      routeCache.invalidateForJob(kind);
      // Jobs that can change library totals (insights) or search
      // results also move the shared data revision.
      if (
        kind === "scan" ||
        kind === "metadata" ||
        kind === "takeout" ||
        kind === "faces" ||
        kind === "geocoding"
      ) {
        dataRevision.bump();
      }
    }
    if (complete || isError) {
      // Linger briefly so the user sees the run finish, then evict. Tracked
      // so clearing the library can cancel it — an untracked timer could
      // otherwise fire against a job registry that has already been reset.
      const timer = setTimeout(() => {
        this.lingerTimers.delete(timer);
        if (this.jobs.get(id) === job) this.dismiss(id);
      }, isError ? 8000 : 2500);
      this.lingerTimers.add(timer);
    }
  }

  private async installInner() {
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
      error_count?: number;
      error_details?: string[];
    };
    const handle = (kind: JobKind, complete: boolean) => (e: { payload: Wire }) => {
      this.applyWire(kind, complete, e.payload);
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
      ["takeout:progress", "takeout", false],
      ["takeout:complete", "takeout", true],
    ];
    const results = await Promise.allSettled(
      subs.map(([ev, kind, done]) => listen<Wire>(ev, handle(kind, done))),
    );
    const unlistens = results.flatMap((result) =>
      result.status === "fulfilled" ? [result.value] : [],
    );
    const failed = results.find((result) => result.status === "rejected");
    if (failed) {
      for (const unlisten of unlistens) {
        try { unlisten(); } catch {}
      }
      this.installed = false;
      throw failed.reason;
    }
    this.unlisten = unlistens;
    this.installed = true;
  }

  /// Manually register a job whose start was triggered by a button
  /// click — gives the indicator something to show before the first
  /// progress event arrives.
  register(id: string, kind: JobKind) {
    if (!id) return;
    this.suppressedLibraryJobIds.delete(id);
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
      if (best == null || jobPriority(j) > jobPriority(best)) {
        best = j;
      } else if (jobPriority(j) === jobPriority(best) && j.elapsed_ms > best.elapsed_ms) {
        best = j;
      }
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

  clearLibraryScoped() {
    // Cancel pending eviction timers: they capture the job they were created
    // for and would otherwise fire against the registry we are about to
    // replace, dismissing an unrelated job that happens to reuse an id.
    for (const timer of this.lingerTimers) clearTimeout(timer);
    this.lingerTimers.clear();
    for (const job of this.jobs.values()) {
      if (isLibraryScopedJob(job)) this.suppressedLibraryJobIds.add(job.id);
    }
    const next = new Map(
      Array.from(this.jobs).filter(([, job]) => !isLibraryScopedJob(job)),
    );
    if (next.size !== this.jobs.size) this.jobs = next;
  }

}

function isLibraryScopedWire(kind: JobKind, id: string, stage?: string): boolean {
  if (LIBRARY_SCOPED_KINDS.has(kind)) return true;
  return kind === "semantic" && (
    stage === "index" ||
    stage === "index-complete" ||
    id.startsWith("pending-semantic-index-")
  );
}

function isLibraryScopedJob(job: Job): boolean {
  if (LIBRARY_SCOPED_KINDS.has(job.kind)) return true;
  return job.kind === "semantic" && (
    job.stage === "index" ||
    job.stage === "index-complete" ||
    job.id.startsWith("pending-semantic-index-")
  );
}

function jobPriority(j: Job): number {
  if (j.status === "running") return 2;
  if (j.status === "error") return 1;
  return 0;
}

/// Compute a rough ETA in ms based on processed/total + elapsed_ms.
/// Returns null if total is unknown or processed is 0.
export function etaMs(j: Job): number | null {
  if (j.total == null || j.total <= 0 || j.processed <= 0) return null;
  const remaining = Math.max(0, j.total - j.processed);
  return Math.round((j.elapsed_ms / j.processed) * remaining);
}

export const jobs = new JobsStore();
