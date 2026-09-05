export type ThumbnailLoader = (id: number, mediaType: "photo" | "video") => Promise<string | null>;
export type ThumbnailBatchLoader = (
  ids: number[],
  mediaType: "photo" | "video",
) => Promise<Map<number, string | null>>;
export type ThumbnailReadyHandler = (thumbnailPath: string) => void;

interface QueueEntry {
  id: number;
  mediaType: "photo" | "video";
  priority: number;
  batchable: boolean;
  seq: number;
  handlers: Set<ThumbnailReadyHandler>;
}

export function createThumbnailQueue(
  load: ThumbnailLoader,
  maxActive = 6,
  loadBatch?: ThumbnailBatchLoader,
  maxBatchSize = 24,
) {
  const batchPriorityWindow = 1_000;
  const maxUrgentOverflow = maxActive;
  let active = 0;
  let activeUrgent = 0;
  let seq = 0;
  let pumpScheduled = false;
  const queued = new Map<number, QueueEntry>();
  const activeHandlers = new Map<number, Set<ThumbnailReadyHandler>>();
  let generation = 0;

  function bestEntry(match?: (entry: QueueEntry) => boolean, remove = true): QueueEntry | null {
    let best: QueueEntry | null = null;
    for (const entry of queued.values()) {
      if (match && !match(entry)) continue;
      if (
        best == null ||
        entry.priority > best.priority ||
        (entry.priority === best.priority && entry.seq > best.seq)
      ) {
        best = entry;
      }
    }
    if (best && remove) queued.delete(best.id);
    return best;
  }

  function nextBatch(): QueueEntry[] {
    const first = bestEntry();
    if (!first) return [];
    const batch = [first];
    if (!loadBatch || first.mediaType !== "photo" || maxBatchSize <= 1 || !first.batchable) {
      return batch;
    }

    while (batch.length < maxBatchSize) {
      const next = bestEntry(
        (entry) =>
          entry.batchable &&
          entry.mediaType === first.mediaType &&
          entry.priority >= first.priority - batchPriorityWindow,
      );
      if (!next) break;
      batch.push(next);
    }
    return batch;
  }

  function pump() {
    while (true) {
      const next = bestEntry(undefined, false);
      if (!next) return;
      const urgent = !next.batchable;
      if (active >= maxActive && !(urgent && activeUrgent < maxUrgentOverflow)) return;

      const entries = nextBatch();
      if (entries.length === 0) return;
      const runGeneration = generation;
      active += 1;
      if (urgent) activeUrgent += 1;
      for (const entry of entries) {
        activeHandlers.set(entry.id, new Set(entry.handlers));
      }
      const run =
        entries.length > 1 && loadBatch
          ? loadBatch(
              entries.map((entry) => entry.id),
              entries[0].mediaType,
            )
          : load(entries[0].id, entries[0].mediaType).then(
              (thumbnailPath) => new Map([[entries[0].id, thumbnailPath]]),
            );
      void run
        .then((thumbnailPaths) => {
          if (runGeneration !== generation) return;
          for (const entry of entries) {
            const handlers = activeHandlers.get(entry.id);
            const thumbnailPath = thumbnailPaths.get(entry.id);
            if (!thumbnailPath || !handlers) continue;
            for (const handler of handlers) handler(thumbnailPath);
          }
        })
        .catch(() => {
          // Callers retry when the cell becomes visible again.
        })
        .finally(() => {
          active -= 1;
          if (urgent) activeUrgent -= 1;
          if (runGeneration === generation) {
            for (const entry of entries) activeHandlers.delete(entry.id);
          }
          requestPump();
        });
    }
  }

  function requestPump() {
    if (!loadBatch) {
      pump();
      return;
    }
    if (pumpScheduled) return;
    pumpScheduled = true;
    queueMicrotask(() => {
      pumpScheduled = false;
      pump();
    });
  }

  function enqueue(
    id: number,
    onReady: ThumbnailReadyHandler,
    priority = Date.now(),
    mediaType: "photo" | "video" = "photo",
    batchable = true,
  ) {
    const enqueueGeneration = generation;
    const running = activeHandlers.get(id);
    if (running) {
      running.add(onReady);
      return () => running.delete(onReady);
    }

    const existing = queued.get(id);
    if (existing) {
      existing.handlers.add(onReady);
      existing.priority = Math.max(existing.priority, priority);
      existing.batchable &&= batchable;
      existing.seq = ++seq;
    } else {
      queued.set(id, {
        id,
        mediaType,
        priority,
        batchable,
        seq: ++seq,
        handlers: new Set([onReady]),
      });
    }
    requestPump();

    return () => {
      if (enqueueGeneration !== generation) return;
      activeHandlers.get(id)?.delete(onReady);
      const entry = queued.get(id);
      if (!entry) return;
      entry.handlers.delete(onReady);
      if (entry.handlers.size === 0) queued.delete(id);
    };
  }

  return {
    enqueue,
    reset: () => {
      generation += 1;
      queued.clear();
      activeHandlers.clear();
    },
    pendingCount: () => queued.size,
    activeCount: () => active,
  };
}
