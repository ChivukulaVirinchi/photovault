/**
 * Bounded asset loads with explicit presentation state. The browser's resource
 * cache remains authoritative; no second unbounded decoded-image cache.
 */
const MAX_CONCURRENT = 5;
const REQUEST_TIMEOUT_MS = 12_000;
interface Request {
  img: HTMLImageElement;
  url: string;
  priority: number;
  state: "queued" | "active" | "settled";
  cancel: () => void;
  /// See `PooledImageParams.ownOpacity`.
  ownOpacity: boolean;
}
const queue: Request[] = [];
const current = new WeakMap<HTMLImageElement, Request>();
let active = 0;
let pauseCount = 0;

function present(img: HTMLImageElement, state: "loading" | "ready" | "error") {
  img.dataset.imageState = state;
  img.style.opacity = state === "ready" ? "1" : "0";
}

function pump() {
  if (pauseCount > 0) return;
  while (active < MAX_CONCURRENT && queue.length) {
    let best = 0;
    for (let i = 1; i < queue.length; i++) if (queue[i].priority > queue[best].priority) best = i;
    const req = queue.splice(best, 1)[0];
    if (current.get(req.img) !== req) continue;
    const img = req.img;
    req.state = "active";
    active++;
    let decoding = false;
    const owns = () => current.get(img) === req && req.state === "active";
    const finish = () => {
      if (!owns()) return;
      req.state = "settled";
      clearTimeout(timer);
      img.removeEventListener("load", loaded);
      img.removeEventListener("error", failed);
      current.delete(img);
      active--;
      pump();
    };
    /// Report the presentation state. `ownOpacity` callers get the
    /// `data-image-state` hook but keep control of `style.opacity`.
    const settle = (state: "ready" | "error") => {
      if (req.ownOpacity) img.dataset.imageState = state;
      else present(img, state);
    };
    const failed = () => {
      if (!owns()) return;
      settle("error");
      // Abort the browser load before admitting another request.
      img.removeAttribute("src");
      finish();
    };
    const ready = () => {
      if (!owns()) return;
      settle("ready");
      finish();
    };
    const loaded = () => {
      if (!owns() || decoding) return;
      decoding = true;
      if (typeof img.decode === "function") {
        void img.decode().then(ready, failed);
      } else ready();
    };
    const timer = setTimeout(failed, REQUEST_TIMEOUT_MS);
    req.cancel = () => {
      if (!owns()) return;
      img.removeAttribute("src");
      finish();
    };
    img.addEventListener("load", loaded);
    img.addEventListener("error", failed);
    img.src = req.url;
    if (img.complete && img.naturalWidth > 0) loaded();
  }
}

/**
 * Stop admitting new browser image loads while interaction needs the WebView's
 * main thread. Requests already decoding are allowed to finish; queued work
 * resumes when the returned release function is called.
 */
export function pausePooledImages(): () => void {
  pauseCount += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    pauseCount = Math.max(0, pauseCount - 1);
    if (pauseCount === 0) pump();
  };
}

function dequeue(img: HTMLImageElement) {
  const req = current.get(img);
  if (!req) return;
  if (req.state === "queued") {
    const index = queue.indexOf(req);
    if (index >= 0) queue.splice(index, 1);
    current.delete(img);
    req.state = "settled";
  } else req.cancel();
}

export interface PooledImageParams {
  src: string | null;
  priority?: number;
  /// Let the caller own this element's opacity.
  ///
  /// By default the pool drives opacity itself — 0 until the bitmap is
  /// decoded and presented, then 1 — which is what makes a grid tile fade in
  /// instead of showing a broken frame. That inline style also wins over any
  /// stylesheet rule, so an element that coordinates its own visibility (the
  /// slideshow's crossfading backdrops, which use `.visible { opacity }`) must
  /// opt out or the class stops having any effect.
  ///
  /// With `ownOpacity: true` the pool still queues, loads and reports
  /// `data-image-state`, but never touches `style.opacity`.
  ownOpacity?: boolean;
}

export function pooledImage(node: HTMLImageElement, params: PooledImageParams) {
  let wanted: string | null | undefined;
  const originalOpacity = node.style.opacity;
  function apply(params: PooledImageParams) {
    const pending = current.get(node);
    if (wanted === params.src) {
      if (pending) pending.priority = params.priority ?? 0;
      return;
    }
    wanted = params.src;
    dequeue(node);
    if (params.ownOpacity) {
      // Report state for CSS hooks, but leave opacity alone.
      node.dataset.imageState = "loading";
    } else {
      present(node, "loading");
    }
    node.removeAttribute("src");
    if (!params.src) {
      node.removeAttribute("data-pool-src");
      return;
    }
    node.setAttribute("data-pool-src", params.src);
    const req: Request = {
      img: node, url: params.src, priority: params.priority ?? 0,
      state: "queued", cancel: () => {}, ownOpacity: params.ownOpacity === true,
    };
    current.set(node, req);
    queue.push(req);
    pump();
  }
  apply(params);
  return {
    update: apply,
    destroy() {
      dequeue(node);
      node.removeAttribute("data-pool-src");
      node.removeAttribute("data-image-state");
      // Only undo an opacity the pool itself set. For `ownOpacity` callers the
      // inline value was never ours, so restoring the captured original would
      // clobber whatever the caller's own CSS had applied.
      if (!params.ownOpacity) node.style.opacity = originalOpacity;
    },
  };
}
