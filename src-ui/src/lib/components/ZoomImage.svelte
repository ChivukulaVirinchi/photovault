<script lang="ts">
  /// Production photo zoom & pan.
  ///
  /// - Wheel zoom is **anchored to the cursor**: the pixel under the
  ///   cursor stays under the cursor as scale changes.
  /// - Drag-to-pan when zoomed past fit. Cursor changes to grab/grabbing.
  /// - Double-click toggles 1:1 ↔ fit.
  /// - Keyboard `+` / `-` zoom about image centre, `0` = fit, `1` = 1:1.
  /// - Preserves the current fit/actual intent across photo changes.
  /// - Honours an external `rotate` prop (degrees: 0 / 90 / 180 / 270);
  ///   the parent owns the rotation state so toolbar buttons can mutate
  ///   it without reaching inside.

  import { onMount } from "svelte";
  import type { ZoomApi } from "../zoomApi";

  interface Props {
    src: string;
    alt?: string;
    /// Rotation in degrees. 0 / 90 / 180 / 270.
    rotate?: number;
    /// Preferred mode applied when a new image finishes decoding.
    preferredMode?: "fit" | "actual";
    /// External handle so parent toolbars can drive zoom from buttons.
    api?: ZoomApi;
  }
  let { src, alt = "", rotate = 0, preferredMode = "fit", api = $bindable() }: Props = $props();

  // Transform state — internal, driven by gestures + keyboard + toolbar API.
  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);

  let containerEl = $state<HTMLDivElement | undefined>(undefined);
  let imgEl       = $state<HTMLImageElement | undefined>(undefined);
  let naturalW    = $state(0);
  let naturalH    = $state(0);
  let containerW  = $state(0);
  let containerH  = $state(0);

  let dragging = $state(false);
  let dragStart = { x: 0, y: 0, tx: 0, ty: 0 };

  /// Scale at which the image fits within the container with rotation applied.
  const fitScale = $derived.by(() => {
    if (!naturalW || !naturalH || !containerW || !containerH) return 1;
    // Treat 90/270 rotations as effectively swapping dimensions.
    const rotated = rotate % 180 !== 0;
    const w = rotated ? naturalH : naturalW;
    const h = rotated ? naturalW : naturalH;
    return Math.min(containerW / w, containerH / h, 1);
  });

  const MIN_SCALE_FACTOR = 1;     // can't go below fit
  const MAX_SCALE = 16;           // 16x is the 1:1-equivalent ceiling

  function clampScale(s: number): number {
    const min = fitScale * MIN_SCALE_FACTOR;
    return Math.min(Math.max(s, min), Math.max(MAX_SCALE, fitScale));
  }

  function reset() {
    scale = fitScale;
    tx = 0;
    ty = 0;
  }

  function actual() {
    scale = clampScale(1);
    tx = 0;
    ty = 0;
    clampPan();
  }

  function applyPreferredMode() {
    if (preferredMode === "actual") actual();
    else reset();
  }

  /// Zoom around a viewport point (cx, cy) — keeps the image pixel under
  /// that point fixed.
  function zoomAt(cx: number, cy: number, delta: number) {
    const oldScale = scale;
    const newScale = clampScale(oldScale * delta);
    if (newScale === oldScale) return;
    // Coordinates relative to container centre.
    const rect = containerEl?.getBoundingClientRect();
    if (!rect) return;
    const dx = cx - rect.left - rect.width / 2;
    const dy = cy - rect.top - rect.height / 2;
    // Solve: (dx - tx) * (newScale / oldScale) + tx' = dx
    const ratio = newScale / oldScale;
    tx = dx - (dx - tx) * ratio;
    ty = dy - (dy - ty) * ratio;
    scale = newScale;
    clampPan();
  }

  /// Constrain pan so the image can't be dragged completely off-screen.
  function clampPan() {
    if (!naturalW || !naturalH || !containerW || !containerH) return;
    const rotated = rotate % 180 !== 0;
    const w = (rotated ? naturalH : naturalW) * scale;
    const h = (rotated ? naturalW : naturalH) * scale;
    // Allow pan up to half the *overflow*. When image is smaller than
    // container, force-centre.
    const overflowX = Math.max(0, w - containerW) / 2;
    const overflowY = Math.max(0, h - containerH) / 2;
    tx = Math.min(Math.max(tx, -overflowX), overflowX);
    ty = Math.min(Math.max(ty, -overflowY), overflowY);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    // Trackpad pinch arrives as ctrl+wheel in Chromium; treat both same way.
    const delta = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    zoomAt(e.clientX, e.clientY, delta);
  }

  function onMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    if (scale <= fitScale + 0.001) return; // nothing to pan when at-fit
    dragging = true;
    dragStart = { x: e.clientX, y: e.clientY, tx, ty };
    e.preventDefault();
  }
  function onMouseMove(e: MouseEvent) {
    if (!dragging) return;
    tx = dragStart.tx + (e.clientX - dragStart.x);
    ty = dragStart.ty + (e.clientY - dragStart.y);
    clampPan();
  }
  function onMouseUp() {
    dragging = false;
  }

  function onDoubleClick(e: MouseEvent) {
    // Toggle between fit and 1:1 — anchor the toggle on the cursor so
    // double-click on a face lands centred on that face.
    const at1to1 = Math.abs(scale - 1) < 0.01;
    if (at1to1) {
      reset();
    } else {
      const delta = 1 / scale;
      zoomAt(e.clientX, e.clientY, delta);
    }
  }

  function measure() {
    if (!containerEl) return;
    const r = containerEl.getBoundingClientRect();
    containerW = r.width;
    containerH = r.height;
  }

  function onImgLoad() {
    if (!imgEl) return;
    naturalW = imgEl.naturalWidth;
    naturalH = imgEl.naturalHeight;
    applyPreferredMode();
  }

  // Reset whenever src or rotation changes.
  $effect(() => {
    void src;
    void rotate;
    void preferredMode;
    // Defer to next tick so naturalW/H reflect the new image after load.
    setTimeout(applyPreferredMode, 0);
  });

  onMount(() => {
    measure();
    const ro = new ResizeObserver(() => {
      measure();
      applyPreferredMode();
    });
    if (containerEl) ro.observe(containerEl);
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      ro.disconnect();
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  });

  // Public API for parent toolbars / keyboard handlers.
  api = {
    zoomIn: () => {
      if (!containerEl) return;
      const r = containerEl.getBoundingClientRect();
      zoomAt(r.left + r.width / 2, r.top + r.height / 2, 1.25);
    },
    zoomOut: () => {
      if (!containerEl) return;
      const r = containerEl.getBoundingClientRect();
      zoomAt(r.left + r.width / 2, r.top + r.height / 2, 1 / 1.25);
    },
    fit: reset,
    actual,
  };

  const cursor = $derived(
    dragging ? "grabbing" : scale > fitScale + 0.001 ? "grab" : "default"
  );
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="zoom-host"
  bind:this={containerEl}
  onwheel={onWheel}
  onmousedown={onMouseDown}
  ondblclick={onDoubleClick}
  style:cursor
  role="img"
  aria-label={alt}
  tabindex="-1"
>
  <img
    bind:this={imgEl}
    {src}
    {alt}
    onload={onImgLoad}
    draggable="false"
    style:transform="translate({tx}px, {ty}px) scale({scale}) rotate({rotate}deg)"
    style:will-change={dragging ? "transform" : "auto"}
  />
</div>

<style>
  .zoom-host {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
    touch-action: none;
  }
  img {
    max-width: none;
    max-height: none;
    transform-origin: center center;
    transition: transform 50ms linear;
    pointer-events: none;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.45),
                0 4px 16px rgba(0, 0, 0, 0.30);
  }
</style>
