import { photos } from "./api/photos";
import { createThumbnailQueue, type ThumbnailReadyHandler } from "./thumbnailQueue";
import { probeVideoPoster } from "./videoProbe";

export interface ThumbnailRequestOptions {
  id: number;
  thumbnailPath: string | null;
  mediaType?: "photo" | "video";
  root?: Element | Document | null;
  rootMargin?: string;
  debounceMs?: number;
  priority?: number;
  onReady: ThumbnailReadyHandler;
}

const queue = createThumbnailQueue(
  async (id, mediaType) =>
    mediaType === "video"
      ? probeVideoPoster(id)
      : (await photos.requestThumbnail(id)).thumbnail_path,
  4,
  async (ids, mediaType) => {
    if (mediaType === "video") {
      const pairs = await Promise.all(ids.map(async (id) => [id, await probeVideoPoster(id)] as const));
      return new Map(pairs);
    }
    const result = await photos.requestThumbnails(ids);
    return new Map(result.items.map((item) => [item.id, item.thumbnail_path]));
  },
  2,
);

export function enqueueThumbnail(
  id: number,
  onReady: ThumbnailReadyHandler,
  priority = Date.now(),
  mediaType: "photo" | "video" = "photo",
  batchable = true,
) {
  return queue.enqueue(id, onReady, priority, mediaType, batchable);
}

export function resetThumbnailRequests() {
  queue.reset();
}

export function thumbnailOnVisible(node: HTMLElement, initial: ThumbnailRequestOptions) {
  let options = initial;
  let prefetchObserver: IntersectionObserver | null = null;
  let viewportObserver: IntersectionObserver | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let unsubscribe: (() => void) | null = null;
  let failedPath: string | null = null;
  function retryMissingImage(event: Event) {
    if (!(event.target instanceof HTMLImageElement) || !options.thumbnailPath || failedPath === options.thumbnailPath) return;
    failedPath = options.thumbnailPath;
    options = { ...options, thumbnailPath: null };
    schedule(true);
  }
  node.addEventListener("error", retryMissingImage, true);

  function cleanupPending() {
    if (timer != null) {
      clearTimeout(timer);
      timer = null;
    }
    if (unsubscribe) {
      unsubscribe();
      unsubscribe = null;
    }
  }

  function cleanupObserver() {
    if (prefetchObserver) {
      prefetchObserver.disconnect();
      prefetchObserver = null;
    }
    if (viewportObserver) {
      viewportObserver.disconnect();
      viewportObserver = null;
    }
    cleanupPending();
  }

  function rootViewportRect(): DOMRect | null {
    const root = options.root;
    if (root && root instanceof Element) return root.getBoundingClientRect();
    if (typeof window === "undefined") return null;
    return new DOMRect(0, 0, window.innerWidth, window.innerHeight);
  }

  function isActuallyVisible(entry: IntersectionObserverEntry): boolean {
    const rootRect = rootViewportRect();
    if (!rootRect) return false;
    const rect = entry.boundingClientRect;
    return (
      rect.bottom > rootRect.top &&
      rect.top < rootRect.bottom &&
      rect.right > rootRect.left &&
      rect.left < rootRect.right
    );
  }

  function schedule(visibleNow = false) {
    cleanupPending();
    if (options.thumbnailPath || options.id <= 0) return;
    const debounceMs = visibleNow ? 0 : (options.debounceMs ?? 70);
    timer = setTimeout(() => {
      timer = null;
      if (options.thumbnailPath) return;
      const priority = options.priority ?? (Date.now() + (visibleNow ? 1_000_000_000 : 0));
      unsubscribe = enqueueThumbnail(
        options.id,
        options.onReady,
        priority,
        options.mediaType ?? "photo",
        !visibleNow,
      );
    }, debounceMs);
  }

  function attach() {
    cleanupObserver();
    if (options.thumbnailPath || options.id <= 0) return;
    prefetchObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            schedule(false);
          } else {
            cleanupPending();
          }
        }
      },
      {
        root: options.root ?? null,
        rootMargin: options.rootMargin ?? "900px",
      },
    );
    viewportObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting && isActuallyVisible(entry)) {
            schedule(true);
          }
        }
      },
      {
        root: options.root ?? null,
        rootMargin: "0px",
      },
    );
    prefetchObserver.observe(node);
    viewportObserver.observe(node);
  }

  attach();

  return {
    update(next: ThumbnailRequestOptions) {
      const rootChanged = next.root !== options.root;
      const idChanged = next.id !== options.id;
      const pathChanged = next.thumbnailPath !== options.thumbnailPath;
      options = next;
      if (options.thumbnailPath || options.id <= 0) {
        cleanupObserver();
        return;
      }
      if (!prefetchObserver || !viewportObserver || rootChanged || idChanged || pathChanged) attach();
    },
    destroy() {
      node.removeEventListener("error", retryMissingImage, true);
      cleanupObserver();
    },
  };
}
