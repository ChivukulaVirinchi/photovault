import { libraryStore } from "./library.svelte";
import { dataRevision } from "./dataRevision.svelte";
import type { JobKind } from "./jobs.svelte";

/**
 * Session-scoped route cache.
 *
 * Six of the nine routes used to re-issue their IPC on every switch, and
 * People / Duplicates / Bursts / Trash / Map had no cache at all — tab
 * switching felt like a reload every time. Entries are keyed by
 * `driveRoot:session:revision:key`, so a library switch or a data revision
 * bump can never serve another library's — or another revision's — data.
 * Nothing clears this store globally: keying is what makes stale reads
 * impossible, and `syncRevision` prunes entries from superseded revisions.
 * (An earlier version of this comment claimed App.svelte clears everything
 * on a session change. It does not, and it does not need to.)
 *
 * Invalidation is explicit, never time-based: cached entries only go
 * away when the data they represent can actually have changed —
 *  - the matching background job completed (see `invalidateForJob`)
 *  - a mutation happened that touches the route's data (trash/restore,
 *    resolve duplicates, merge people, run bursts…)
 *  - the library switched or closed.
 *
 * Pending loaders are generation-guarded: every invalidation bumps the
 * generation of the affected keys, and a loader's answer is only stored
 * if its captured generation (and session) still match. A request that
 * started before a mutation therefore cannot refill the cache with
 * obsolete data, and invalidated in-flight promises are not shared with
 * newer callers — a new `get` starts a fresh load instead of awaiting a
 * stale one.
 */
interface Entry {
  key: string;
  data: unknown;
  loadedAt: number;
}

interface Pending {
  key: string;
  gen: number;
  promise: Promise<unknown>;
}

const MAX_ENTRIES = 64;

class RouteCacheStore {
  private entries = new Map<string, Entry>();
  private inflight = new Map<string, Pending>();
  private generations = new Map<string, number>();
  private revision = dataRevision.version;

  private syncRevision() {
    const revision = dataRevision.version;
    if (revision === this.revision) return;
    // A mutation revision invalidates every route cache atomically. Clearing
    // the bounded maps also prevents old revision keys from accumulating.
    this.entries.clear();
    this.inflight.clear();
    this.generations.clear();
    this.revision = revision;
  }

  private fullKey(logicalKey: string): string {
    this.syncRevision();
    return `${libraryStore.driveRoot ?? "closed"}:${libraryStore.session}:${this.revision}:${logicalKey}`;
  }

  /** Bump the generation of every key whose logical part matches `match`. */
  private bump(match: (logicalKey: string) => boolean): Set<string> {
    const bumped = new Set<string>();
    for (const [full, pending] of this.inflight) {
      if (match(pending.key)) bumped.add(full);
    }
    for (const full of this.generations.keys()) {
      // full = `${driveRoot}:${session}:${logicalKey}`; recover the
      // logical part by stripping the (recorded) session prefix.
      const stripped = this.logicalOf(full);
      if (stripped !== null && match(stripped)) bumped.add(full);
    }
    for (const full of bumped) {
      this.generations.set(full, (this.generations.get(full) ?? 0) + 1);
    }
    return bumped;
  }

  /** Recover the logical key of a full key under the *current* session. */
  private logicalOf(full: string): string | null {
    const prefix = `${libraryStore.driveRoot ?? "closed"}:${libraryStore.session}:${this.revision}:`;
    return full.startsWith(prefix) ? full.slice(prefix.length) : null;
  }

  /**
   * Return the cached value for `key`, or run `loader` once (concurrent
   * callers share the same in-flight promise) and cache the result.
   * The answer is stored only if no invalidation happened while the
   * loader was running.
   */
  get<T>(key: string, loader: () => Promise<T>): Promise<T> {
    const full = this.fullKey(key);
    const cached = this.entries.get(full);
    if (cached) return Promise.resolve(cached.data as T);
    const pending = this.inflight.get(full);
    if (pending) return pending.promise as Promise<T>;
    const gen = this.generations.get(full) ?? 0;
    let p: Promise<unknown>;
    p = loader()
      .then((data) => {
        // Same key still owned by this exact request, same generation,
        // same session — only then may the answer enter the cache.
        if (
          this.inflight.get(full)?.promise === p &&
          (this.generations.get(full) ?? 0) === gen &&
          this.fullKey(key) === full
        ) {
          this.entries.set(full, { key, data, loadedAt: Date.now() });
          while (this.entries.size > MAX_ENTRIES) {
            const oldest = this.entries.keys().next().value;
            if (oldest === undefined) break;
            this.entries.delete(oldest);
          }
        }
        return data;
      })
      .finally(() => {
        // Only clear our own slot — a newer request may occupy it.
        if (this.inflight.get(full)?.promise === p) this.inflight.delete(full);
      });
    this.inflight.set(full, { key, gen, promise: p });
    return p as Promise<T>;
  }

  /** Synchronous read of a cached value, or null. */
  peek<T>(key: string): T | null {
    const cached = this.entries.get(this.fullKey(key));
    return cached ? (cached.data as T) : null;
  }

  /** Store a value computed outside `get` (stale-while-revalidate flows). */
  put<T>(key: string, data: T) {
    this.entries.set(this.fullKey(key), { key, data, loadedAt: Date.now() });
    while (this.entries.size > MAX_ENTRIES) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) break;
      this.entries.delete(oldest);
    }
  }

  /** Drop entries whose logical key is `prefix` or nested under it. */
  invalidate(prefix: string) {
    const matches = (k: string) => k === prefix || k.startsWith(`${prefix}:`);
    for (const [full, entry] of this.entries) {
      if (matches(entry.key)) this.entries.delete(full);
    }
    // Stop sharing invalidated promises: pending callers of `get` that
    // arrive after this point start a fresh load, and the orphaned
    // loader can no longer write its (obsolete) answer back.
    const bumped = this.bump(matches);
    for (const full of bumped) this.inflight.delete(full);
  }

  /** Drop everything for the current library (a full data refresh). */
  invalidateAll() {
    this.entries.clear();
    // Bump every generation so deferred loaders cannot refill the cache,
    // then stop sharing all pending promises.
    for (const full of new Set([...this.inflight.keys(), ...this.generations.keys()])) {
      this.generations.set(full, (this.generations.get(full) ?? 0) + 1);
    }
    this.inflight.clear();
  }

  /** Library switched or closed — nothing survives a session change. */
  clear() {
    // Session changed: deferred loaders from the old session must not
    // write entries (their `fullKey(key) === full` check fails too), and
    // they are no longer shared with anyone.
    for (const full of new Set([...this.inflight.keys(), ...this.generations.keys()])) {
      this.generations.set(full, (this.generations.get(full) ?? 0) + 1);
    }
    this.entries.clear();
    this.inflight.clear();
  }

  /**
   * Job-completion hooks. Background jobs are the main way route data
   * changes without a user mutation, so their completion drives the
   * invalidation matrix.
   */
  invalidateForJob(kind: JobKind) {
    switch (kind) {
      case "faces":
        this.invalidate("people");
        break;
      case "duplicates":
        this.invalidate("duplicates");
        break;
      case "bursts":
        this.invalidate("bursts");
        break;
      case "geocoding":
        this.invalidate("map");
        break;
      // These reshuffle the whole library — every route's data can have
      // changed (new photos, new dates, new albums, new groups).
      case "scan":
      case "metadata":
      case "takeout":
        this.invalidateAll();
        break;
      default:
        // thumbnails/assets/semantic only change pixels, not route data.
        break;
    }
  }
}

export const routeCache = new RouteCacheStore();
