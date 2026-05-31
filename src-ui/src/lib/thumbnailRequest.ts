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
  6,
);

export function enqueueThumbnail(
  id: number,
  onReady: ThumbnailReadyHandler,
  priority = Date.now(),
  mediaType: "photo" | "video" = "photo",
) {
  return queue.enqueue(id, onReady, priority, mediaType);
}

export function thumbnailOnVisible(node: HTMLElement, initial: ThumbnailRequestOptions) {
  let options = initial;
  let observer: IntersectionObserver | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let unsubscribe: (() => void) | null = null;

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
    if (observer) {
      observer.disconnect();
      observer = null;
    }
    cleanupPending();
  }

  function schedule() {
    cleanupPending();
    if (options.thumbnailPath || options.id <= 0) return;
    const debounceMs = options.debounceMs ?? 70;
    timer = setTimeout(() => {
      timer = null;
      if (options.thumbnailPath) return;
      unsubscribe = enqueueThumbnail(
        options.id,
        options.onReady,
        options.priority ?? Date.now(),
        options.mediaType ?? "photo",
      );
    }, debounceMs);
  }

  function attach() {
    cleanupObserver();
    if (options.thumbnailPath || options.id <= 0) return;
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            schedule();
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
    observer.observe(node);
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
      if (!observer || rootChanged || idChanged || pathChanged) attach();
    },
    destroy() {
      cleanupObserver();
    },
  };
}
