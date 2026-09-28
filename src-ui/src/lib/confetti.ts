/**
 * A small, dependency-free confetti burst.
 *
 * Deliberately restrained: the particles are drawn from the app's own accent
 * and ink tokens rather than a rainbow, so a celebration still looks like
 * Smriti. The canvas is created inside the caller's element and removes itself
 * when the last particle dies, so nothing has to be cleaned up by callers.
 */

export interface ConfettiOptions {
  /** Where the burst starts, as a fraction of the target's box. */
  origin?: { x: number; y: number };
  /** How many pieces. Keep it modest — this is a nudge, not a parade. */
  count?: number;
  /** Initial speed in pixels per second. */
  speed?: number;
  /** Spread angle in radians, centred on straight up. */
  spread?: number;
  /**
   * Width of the launch band as a fraction of the target. `0` fires from a
   * single point; `0.9` fills the screen with a rising fan, which is what a
   * milestone wants — a point burst in the middle reads as a small puff.
   */
  spreadWidth?: number;
  /**
   * Seed particles throughout a centred field instead of launching them from
   * a line. `width` is a fraction of the target; the field is reduced as
   * needed to preserve `aspectRatio` inside short windows.
   */
  field?: { width: number; aspectRatio: number };
}

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  size: number;
  spin: number;
  angle: number;
  colour: string;
  /** Seconds of life already used. */
  age: number;
  life: number;
}

const GRAVITY = 900;
const DRAG = 0.86;

/** Read accent + ink straight off the document so the burst matches the theme. */
function palette(): string[] {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  const accent = read("--accent", "#9eb3d4");
  const paper = read("--bg-paper", "#161821");
  return [
    accent,
    read("--ink", "#e7eaf0"),
    `color-mix(in oklab, ${accent} 62%, ${paper})`,
    `color-mix(in oklab, ${accent} 34%, ${paper})`,
    read("--keep", "#7ea291"),
  ].map((value) => {
    /// Resolve `color-mix()` through the browser before handing it to canvas.
    if (!value.startsWith("color-mix")) return value;
    const probe = document.createElement("div");
    probe.style.color = value;
    document.body.appendChild(probe);
    const resolved = getComputedStyle(probe).color;
    probe.remove();
    return resolved || accent;
  });
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Fire a burst inside `target`, which must be a positioned element (the canvas
 * is absolutely positioned to fill it).
 */
export function burstConfetti(target: HTMLElement, options: ConfettiOptions = {}): void {
  if (prefersReducedMotion()) return;
  const width = target.clientWidth;
  const height = target.clientHeight;
  if (width === 0 || height === 0) return;

  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  Object.assign(canvas.style, {
    position: "absolute",
    inset: "0",
    width: "100%",
    height: "100%",
    pointerEvents: "none",
    zIndex: "9",
  });
  target.appendChild(canvas);

  const ctx = canvas.getContext("2d");
  if (!ctx) {
    canvas.remove();
    return;
  }
  ctx.scale(dpr, dpr);

  const colours = palette();
  const count = options.count ?? 44;
  const speed = options.speed ?? 430;
  const spread = options.spread ?? Math.PI * 0.92;
  const originX = (options.origin?.x ?? 0.5) * width;
  const originY = (options.origin?.y ?? 0.42) * height;
  const band = (options.spreadWidth ?? 0) * width;
  let fieldWidth = Math.max(0, Math.min(1, options.field?.width ?? 0)) * width;
  let fieldHeight = options.field ? fieldWidth / Math.max(0.1, options.field.aspectRatio) : 0;
  const maxFieldHeight = height * 0.82;
  if (fieldHeight > maxFieldHeight && options.field) {
    fieldHeight = maxFieldHeight;
    fieldWidth = fieldHeight * options.field.aspectRatio;
  }
  const fieldMode = fieldWidth > 0 && fieldHeight > 0;

  const particles: Particle[] = Array.from({ length: count }, (_, index) => {
    /// Fan the pieces evenly across the spread with a little jitter, so the
    /// burst reads as one gesture rather than a random blob.
    const ratio = count === 1 ? 0.5 : index / (count - 1);
    const fieldX = (index * 0.61803398875) % 1;
    const fieldY = (index * 0.75487766625) % 1;
    const spawnX = fieldMode
      ? originX + (fieldX - 0.5) * fieldWidth
      : originX + (ratio - 0.5) * band;
    const spawnY = fieldMode
      ? originY + (fieldY - 0.5) * fieldHeight
      : originY;
    /// Pieces launched further from the centre lean outward, which is what
    /// turns a point puff into a screen-wide fan.
    const lean = !fieldMode && band > 0 ? ((spawnX - originX) / (band / 2)) * 0.55 : 0;
    const angle = fieldMode
      ? -Math.PI / 2 + (Math.random() - 0.5) * Math.PI * 1.15
      : -Math.PI / 2 + (ratio - 0.5) * spread * (band > 0 ? 0.25 : 1) + lean + (Math.random() - 0.5) * 0.22;
    const velocity = fieldMode
      ? speed * (0.18 + Math.random() * 0.34)
      : speed * (0.55 + Math.random() * 0.65);
    return {
      x: spawnX + (Math.random() - 0.5) * 10,
      y: spawnY + (Math.random() - 0.5) * 14,
      vx: Math.cos(angle) * velocity,
      vy: Math.sin(angle) * velocity,
      size: 6 + Math.random() * 6,
      spin: (Math.random() - 0.5) * 14,
      angle: Math.random() * Math.PI,
      colour: colours[index % colours.length],
      age: 0,
      life: 1.9 + Math.random() * 1.4,
    };
  });

  let previous = performance.now();
  let frame = 0;
  let finished = false;

  function finish() {
    if (finished) return;
    finished = true;
    cancelAnimationFrame(frame);
    clearTimeout(guard);
    canvas.remove();
  }

  /// Frames stop entirely while a window is occluded, which would leave a
  /// frozen frame painted over the UI. Remove the canvas on a timer too.
  const guard = setTimeout(finish, 6000);

  frame = requestAnimationFrame(function step(now) {
    const dt = Math.min((now - previous) / 1000, 0.05);
    previous = now;
    ctx.clearRect(0, 0, width, height);

    let alive = 0;
    for (const particle of particles) {
      particle.age += dt;
      if (particle.age >= particle.life) continue;
      alive += 1;

      const damping = Math.pow(DRAG, dt * 60);
      particle.vx *= damping;
      particle.vy = particle.vy * damping + GRAVITY * dt;
      particle.x += particle.vx * dt;
      particle.y += particle.vy * dt;
      particle.angle += particle.spin * dt;

      /// Hold full opacity for most of the life, then fade out quickly so the
      /// pieces disappear rather than lingering at 5%.
      const remaining = 1 - particle.age / particle.life;
      ctx.globalAlpha = remaining > 0.35 ? 1 : remaining / 0.35;
      ctx.fillStyle = particle.colour;
      ctx.save();
      ctx.translate(particle.x, particle.y);
      ctx.rotate(particle.angle);
      ctx.fillRect(-particle.size / 2, -particle.size / 4, particle.size, particle.size / 2);
      ctx.restore();
    }
    ctx.globalAlpha = 1;

    if (alive > 0) frame = requestAnimationFrame(step);
    else finish();
  });
}
