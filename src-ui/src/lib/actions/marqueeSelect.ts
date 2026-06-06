import { selection } from "../stores/selection.svelte";

type Point = { x: number; y: number };
type Rect = { x: number; y: number; w: number; h: number };

interface MarqueeSelectOptions {
  cellSelector?: string;
  interactiveSelector?: string;
  getAllIds: () => number[];
}

const DEFAULT_CELL_SELECTOR = "[data-photo-id]";
const DEFAULT_INTERACTIVE_SELECTOR = [
  "[data-no-marquee]",
  "a",
  "button",
  "input",
  "textarea",
  "select",
].join(", ");

export function marqueeSelect(node: HTMLElement, options: MarqueeSelectOptions) {
  let opts = options;
  let start: Point | null = null;
  let current: Point | null = null;
  let pointer: Point | null = null;
  let base = new Set<number>();
  let raf = 0;
  let autoScrollRaf = 0;
  let scrollVelocity = 0;
  const overlay = document.createElement("div");

  overlay.className = "pv-marquee";
  overlay.setAttribute("aria-hidden", "true");
  overlay.style.display = "none";
  document.body.appendChild(overlay);

  function pointFromClient(clientX: number, clientY: number): Point {
    const host = node.getBoundingClientRect();
    return {
      x: clientX - host.left + node.scrollLeft,
      y: clientY - host.top + node.scrollTop,
    };
  }

  function rectFromPoints(a: Point, b: Point): Rect {
    const x = Math.min(a.x, b.x);
    const y = Math.min(a.y, b.y);
    return {
      x,
      y,
      w: Math.abs(a.x - b.x),
      h: Math.abs(a.y - b.y),
    };
  }

  function renderOverlay() {
    if (!start || !current) {
      overlay.style.display = "none";
      return;
    }
    const host = node.getBoundingClientRect();
    const rect = rectFromPoints(start, current);
    overlay.style.display = "block";
    overlay.style.left = `${host.left + rect.x - node.scrollLeft}px`;
    overlay.style.top = `${host.top + rect.y - node.scrollTop}px`;
    overlay.style.width = `${rect.w}px`;
    overlay.style.height = `${rect.h}px`;
  }

  function queueUpdate() {
    if (raf !== 0) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      updateSelection();
    });
  }

  function updateAutoScroll() {
    if (!start || !pointer) {
      scrollVelocity = 0;
      return;
    }
    const host = node.getBoundingClientRect();
    const edge = 56;
    const maxVelocity = 28;
    const topDelta = pointer.y - host.top;
    const bottomDelta = host.bottom - pointer.y;
    if (topDelta < edge) {
      scrollVelocity = -Math.ceil(Math.min(1, (edge - topDelta) / edge) * maxVelocity);
    } else if (bottomDelta < edge) {
      scrollVelocity = Math.ceil(Math.min(1, (edge - bottomDelta) / edge) * maxVelocity);
    } else {
      scrollVelocity = 0;
    }
    if (scrollVelocity !== 0 && autoScrollRaf === 0) {
      autoScrollRaf = requestAnimationFrame(stepAutoScroll);
    }
  }

  function stepAutoScroll() {
    autoScrollRaf = 0;
    if (!start || !pointer || scrollVelocity === 0) return;
    const maxScroll = Math.max(0, node.scrollHeight - node.clientHeight);
    const before = node.scrollTop;
    node.scrollTop = Math.max(0, Math.min(maxScroll, before + scrollVelocity));
    if (node.scrollTop === before) {
      scrollVelocity = 0;
      return;
    }
    current = pointFromClient(pointer.x, pointer.y);
    renderOverlay();
    queueUpdate();
    updateAutoScroll();
  }

  function updateSelection() {
    if (!start || !current) return;
    const contentRect = rectFromPoints(start, current);
    const next = new Set(base);
    const allowed = new Set(opts.getAllIds());
    const cells = node.querySelectorAll<HTMLElement>(opts.cellSelector ?? DEFAULT_CELL_SELECTOR);
    cells.forEach((cell) => {
      const rawId = cell.dataset.photoId;
      if (!rawId) return;
      const id = Number(rawId);
      if (!Number.isFinite(id) || !allowed.has(id)) return;
      const cr = cell.getBoundingClientRect();
      const host = node.getBoundingClientRect();
      const cellRect = {
        x: cr.left - host.left + node.scrollLeft,
        y: cr.top - host.top + node.scrollTop,
        w: cr.width,
        h: cr.height,
      };
      const intersects =
        cellRect.x < contentRect.x + contentRect.w &&
        cellRect.x + cellRect.w > contentRect.x &&
        cellRect.y < contentRect.y + contentRect.h &&
        cellRect.y + cellRect.h > contentRect.y;
      if (intersects) next.add(id);
    });
    selection.ids = next;
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest(opts.interactiveSelector ?? DEFAULT_INTERACTIVE_SELECTOR)) return;
    base = e.shiftKey ? new Set(selection.ids) : new Set();
    if (!e.shiftKey) selection.clear();
    pointer = { x: e.clientX, y: e.clientY };
    start = pointFromClient(e.clientX, e.clientY);
    current = start;
    node.setPointerCapture(e.pointerId);
    renderOverlay();
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (!start) return;
    pointer = { x: e.clientX, y: e.clientY };
    current = pointFromClient(e.clientX, e.clientY);
    renderOverlay();
    updateAutoScroll();
    queueUpdate();
  }

  function finish(e: PointerEvent) {
    if (!start) return;
    try { node.releasePointerCapture(e.pointerId); } catch {}
    if (raf !== 0) cancelAnimationFrame(raf);
    if (autoScrollRaf !== 0) cancelAnimationFrame(autoScrollRaf);
    raf = 0;
    autoScrollRaf = 0;
    scrollVelocity = 0;
    start = null;
    current = null;
    pointer = null;
    renderOverlay();
  }

  function onScroll() {
    if (!start || !pointer) return;
    current = pointFromClient(pointer.x, pointer.y);
    renderOverlay();
    queueUpdate();
  }

  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointermove", onPointerMove);
  node.addEventListener("pointerup", finish);
  node.addEventListener("pointercancel", finish);
  node.addEventListener("scroll", onScroll, { passive: true });

  return {
    update(next: MarqueeSelectOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", finish);
      node.removeEventListener("pointercancel", finish);
      node.removeEventListener("scroll", onScroll);
      if (raf !== 0) cancelAnimationFrame(raf);
      if (autoScrollRaf !== 0) cancelAnimationFrame(autoScrollRaf);
      overlay.remove();
    },
  };
}
