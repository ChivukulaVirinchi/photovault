export type ThumbnailLoader = (id: number, mediaType: "photo" | "video") => Promise<string | null>;
export type ThumbnailReadyHandler = (thumbnailPath: string) => void;

interface QueueEntry {
  id: number;
  mediaType: "photo" | "video";
  priority: number;
  seq: number;
  handlers: Set<ThumbnailReadyHandler>;
}

export function createThumbnailQueue(load: ThumbnailLoader, maxActive = 6) {
  let active = 0;
  let seq = 0;
  const queued = new Map<number, QueueEntry>();
  const activeHandlers = new Map<number, Set<ThumbnailReadyHandler>>();

  function nextEntry(): QueueEntry | null {
    let best: QueueEntry | null = null;
    for (const entry of queued.values()) {
      if (
        best == null ||
        entry.priority > best.priority ||
        (entry.priority === best.priority && entry.seq > best.seq)
      ) {
        best = entry;
      }
    }
    if (best) queued.delete(best.id);
    return best;
  }

  function pump() {
    while (active < maxActive) {
      const entry = nextEntry();
      if (!entry) return;
      active += 1;
      activeHandlers.set(entry.id, new Set(entry.handlers));
      void load(entry.id, entry.mediaType)
        .then((thumbnailPath) => {
          const handlers = activeHandlers.get(entry.id);
          if (!thumbnailPath || !handlers) return;
          for (const handler of handlers) handler(thumbnailPath);
        })
        .catch(() => {
          // Callers retry when the cell becomes visible again.
        })
        .finally(() => {
          active -= 1;
          activeHandlers.delete(entry.id);
          pump();
        });
    }
  }

  function enqueue(
    id: number,
    onReady: ThumbnailReadyHandler,
    priority = Date.now(),
    mediaType: "photo" | "video" = "photo",
  ) {
    const running = activeHandlers.get(id);
    if (running) {
      running.add(onReady);
      return () => running.delete(onReady);
    }

    const existing = queued.get(id);
    if (existing) {
      existing.handlers.add(onReady);
      existing.priority = Math.max(existing.priority, priority);
      existing.seq = ++seq;
    } else {
      queued.set(id, {
        id,
        mediaType,
        priority,
        seq: ++seq,
        handlers: new Set([onReady]),
      });
    }
    pump();

    return () => {
      const entry = queued.get(id);
      if (!entry) return;
      entry.handlers.delete(onReady);
      if (entry.handlers.size === 0) queued.delete(id);
    };
  }

  return {
    enqueue,
    pendingCount: () => queued.size,
    activeCount: () => active,
  };
}
