<script lang="ts">
  import { burstConfetti } from "../confetti";
  import { milestones } from "../stores/milestones.svelte";
  import { shareCard } from "../stores/shareCard.svelte";
  import { libraryStore } from "../stores/library.svelte";
  import { jobs, type JobKind } from "../stores/jobs.svelte";

  /// Jobs whose completion can change one of the milestone counters.
  const GROWTH_JOBS = new Set<JobKind>(["scan", "metadata", "faces", "takeout"]);

  const reducedMotion =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  let shown = $state(0);
  let scrim = $state<HTMLDivElement | undefined>();
  let primaryBtn = $state<HTMLButtonElement | undefined>();
  let lastFocused: HTMLElement | null = null;
  /// Opening a library is the first chance to notice a crossed threshold.
  $effect(() => {
    const root = libraryStore.driveRoot;
    if (!root) return;
    void milestones.evaluate(root);
  });

  /// Indexing finishes — the usual way a library crosses a threshold.
  let wasIndexing = false;
  $effect(() => {
    const indexing = Array.from(jobs.jobs.values()).some(
      (job) => GROWTH_JOBS.has(job.kind) && job.status === "running",
    );
    if (indexing) {
      wasIndexing = true;
      return;
    }
    if (!wasIndexing) return;
    wasIndexing = false;
    const root = libraryStore.driveRoot;
    if (root) void milestones.evaluate(root);
  });

  /// Count the number up rather than dropping it on screen — this is the part
  /// that makes it feel earned.
  $effect(() => {
    const milestone = milestones.current;
    if (!milestone) return;
    if (reducedMotion) {
      shown = milestone.value;
      return;
    }
    let raf = 0;
    const started = performance.now();
    const duration = 950;
    const tick = (now: number) => {
      const progress = Math.min(1, (now - started) / duration);
      const eased = 1 - Math.pow(1 - progress, 3);
      shown = Math.round(milestone.value * eased);
      if (progress < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    /// Frames are throttled while a window is occluded, which would leave the
    /// number stuck near zero. Always land on the real value.
    const settle = setTimeout(() => { shown = milestone.value; }, duration + 120);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(settle);
    };
  });

  /// Confetti lands once the card has settled, so the celebration reads as a
  /// consequence of the number arriving rather than noise over the animation.
  $effect(() => {
    const milestone = milestones.current;
    const target = scrim;
    if (!milestone || !target) return;
    const timer = setTimeout(
      () =>
        burstConfetti(target, {
          count: 180,
          speed: 330,
          field: { width: 0.82, aspectRatio: 19 / 16 },
          origin: { x: 0.5, y: 0.5 },
        }),
      430,
    );
    return () => clearTimeout(timer);
  });

  function makeCard() {
    shareCard.show(milestones.snapshot);
    milestones.dismiss();
  }

  /// Keyboard support for the modal: Escape closes, Tab stays inside the
  /// dialog, focus enters on open and returns to where it was on close.
  /// (App.svelte's global Escape handler deliberately skips open modals.)
  $effect(() => {
    if (!milestones.current) {
      if (lastFocused) {
        lastFocused.focus?.();
        lastFocused = null;
      }
      return;
    }
    lastFocused = document.activeElement as HTMLElement | null;
    primaryBtn?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") milestones.dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function trapTab(e: KeyboardEvent) {
    if (e.key !== "Tab" || !scrim) return;
    const focusables = Array.from(scrim.querySelectorAll<HTMLElement>("button"));
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      last.focus();
      e.preventDefault();
    } else if (!e.shiftKey && document.activeElement === last) {
      first.focus();
      e.preventDefault();
    }
  }
</script>

{#if milestones.current}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="scrim"
    class:reduced={reducedMotion}
    bind:this={scrim}
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) milestones.dismiss();
    }}
  >
    <div
      class="moment"
      role="dialog"
      aria-modal="true"
      aria-labelledby="milestone-headline"
      tabindex="-1"
      onkeydown={trapTab}
    >
      <div class="glow" aria-hidden="true"></div>

      <p class="lead">Your library just passed</p>

      <p class="count display" id="milestone-headline">
        {shown.toLocaleString()}
      </p>
      <p class="unit mono">{milestones.current.label}</p>

      <div class="rule" aria-hidden="true"></div>

      <p class="line">{milestones.current.line}</p>

      <div class="actions">
        <button class="primary" bind:this={primaryBtn} onclick={makeCard}>Make a card</button>
        <button class="ghost" onclick={() => milestones.dismiss()}>Not now</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 52%);
    backdrop-filter: blur(7px);
    animation: scrim-in var(--t-base-d) var(--ease);
  }

  .moment {
    position: relative;
    width: min(400px, calc(100vw - 40px));
    padding: var(--s-7) var(--s-6) var(--s-5);
    text-align: center;
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-xl);
    box-shadow: 0 30px 70px rgb(0 0 0 / 45%);
    overflow: hidden;
    animation: moment-in var(--t-slow) var(--ease-out);
  }

  /* One soft pulse of the accent behind the number, then it rests. */
  .glow {
    position: absolute;
    top: 42px;
    left: 50%;
    width: 320px;
    height: 320px;
    margin-left: -160px;
    pointer-events: none;
    background: radial-gradient(circle, var(--accent-soft) 0%, transparent 68%);
    opacity: 0;
    animation: glow-pulse 1500ms var(--ease-out) 180ms;
  }

  .lead {
    margin: 0;
    font-size: var(--t-xs);
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ink-muted);
    animation: rise var(--t-slow) var(--ease-out) 60ms backwards;
  }

  .count {
    position: relative;
    margin: var(--s-3) 0 0;
    font-size: var(--t-display);
    font-weight: 500;
    line-height: 1;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
    font-variation-settings: "opsz" 48;
  }

  .unit {
    margin: var(--s-2) 0 0;
    font-size: var(--t-xs);
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  /* Hairline that draws itself across, left to right. */
  .rule {
    height: 1px;
    margin: var(--s-5) auto var(--s-4);
    background: var(--line);
    transform-origin: left center;
    animation: rule-sweep 700ms var(--ease-out) 520ms backwards;
  }

  .line {
    margin: 0 0 var(--s-5);
    font-size: var(--t-sm);
    line-height: 1.65;
    color: var(--ink-soft);
    animation: rise var(--t-slow) var(--ease-out) 200ms backwards;
  }

  .actions {
    display: flex;
    gap: var(--s-2);
    justify-content: center;
    animation: rise var(--t-slow) var(--ease-out) 320ms backwards;
  }

  @keyframes scrim-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
  @keyframes moment-in {
    from { opacity: 0; transform: translateY(16px) scale(0.975); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes glow-pulse {
    0%   { opacity: 0;    transform: scale(0.72); }
    38%  { opacity: 0.85; transform: scale(1); }
    100% { opacity: 0;    transform: scale(1.22); }
  }
  @keyframes rule-sweep {
    from { transform: scaleX(0); }
    to   { transform: scaleX(1); }
  }

  @media (prefers-reduced-motion: reduce) {
    .scrim, .moment, .glow, .lead, .line, .actions, .rule {
      animation: none;
    }
    .glow { display: none; }
  }
</style>
