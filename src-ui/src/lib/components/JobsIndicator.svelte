<script lang="ts">
  import { jobs, etaMs, type Job } from "../stores/jobs.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Loader2, X } from "lucide-svelte";

  let expanded = $state(false);
  // Auto-collapse when no jobs are running.
  $effect(() => {
    if (jobs.count === 0) expanded = false;
  });

  function pct(j: Job): number | null {
    if (j.total == null || j.total <= 0) return null;
    return Math.min(100, Math.round((j.processed / j.total) * 100));
  }
  function fmtEta(j: Job): string | null {
    const ms = etaMs(j);
    if (ms == null) return null;
    const s = Math.round(ms / 1000);
    if (s < 60) return `~${s}s left`;
    const m = Math.round(s / 60);
    if (m < 60) return `~${m}m left`;
    const h = Math.floor(m / 60);
    const mr = m % 60;
    return `~${h}h ${mr}m left`;
  }
  function canCancel(j: Job): boolean {
    return !["assets", "albumSuggestions"].includes(j.kind);
  }
  async function cancelJob(j: Job) {
    // Optimistic local-state flip so the row reads "Cancelling..."
    // immediately. The real status flip arrives via the complete event
    // when the engine actually stops.
    if (j.id.startsWith("pending-")) {
      // Optimistic placeholder — just dismiss locally.
      jobs.dismiss(j.id);
      return;
    }
    try {
      await invoke("jobs_cancel", { args: { job_id: j.id } });
    } catch (e) {
      // Best-effort — if the IPC fails (e.g., job already finished),
      // leave the row alone and let the natural complete event clean up.
      // eslint-disable-next-line no-console
      console.warn("jobs_cancel failed:", e);
    }
  }
</script>

{#if jobs.count > 0}
  <div class="indicator" class:expanded role="status" aria-live="polite">
    {#if expanded}
      <header>
        <span class="title">
          {jobs.count} {jobs.count === 1 ? "task" : "tasks"} running
        </span>
        <button class="icon-btn" onclick={() => (expanded = false)} aria-label="Collapse">
          <X size={13} strokeWidth={1.75} />
        </button>
      </header>
      <ul class="list">
        {#each jobs.active as j (j.id)}
          {@const p = pct(j)}
          {@const eta = fmtEta(j)}
          <li>
            <div class="row">
              <Loader2 class="spin" size={13} strokeWidth={2} />
              <span class="t">{j.title}</span>
              {#if p != null}<span class="pct mono">{p}%</span>{/if}
              {#if canCancel(j)}
                <button
                  class="icon-btn cancel"
                  onclick={() => cancelJob(j)}
                  aria-label={j.kind === "faces" ? "Pause" : "Cancel"}
                  title={j.kind === "faces"
                    ? "Pause ? resume later, even after moving the drive to another machine"
                    : "Cancel"}
                >
                  <X size={12} strokeWidth={2} />
                </button>
              {/if}
            </div>
            {#if j.total != null && j.total > 0}
              <div class="track" aria-hidden="true">
                <div class="fill" style="width: {p ?? 0}%"></div>
              </div>
            {/if}
            <div class="sub">
              {#if eta}<span class="eta mono">{eta}</span>{/if}
              {#if j.kind === "faces" && j.embedder_route}
                <!-- Confirms at a glance whether embeddings are
                     routing to the cloud bridge or running locally
                     for this run. Reflects the configured intent at
                     job start, not per-batch runtime health. -->
                <span class="chip" data-route={j.embedder_route}
                  title={j.embedder_route === "bridge"
                    ? "Embeddings routed to the configured Cloud GPU bridge. Detection still runs locally on every photo."
                    : "Embeddings running on the local ONNX session (CPU / local GPU). Enable a healthy bridge in Settings to offload them."}>
                  {j.embedder_route === "bridge" ? "Cloud GPU" : "Local CPU"}
                </span>
              {/if}
              {#if j.message}<span class="msg">{j.message}</span>{/if}
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <button class="pill" onclick={() => (expanded = true)} aria-label="Show running tasks">
        <Loader2 class="spin" size={14} strokeWidth={2} />
        <span class="count mono">{jobs.count}</span>
        <span class="label">{jobs.count === 1 ? "task" : "tasks"}</span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .indicator {
    position: fixed;
    right: var(--s-4);
    bottom: var(--s-4);
    z-index: 60;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px 6px 10px;
    background: var(--bg-paper);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: var(--r-pill, 999px);
    box-shadow: 0 8px 22px rgba(0,0,0,0.40);
    cursor: pointer;
    font-size: var(--t-sm);
    transition: background var(--t-fast) var(--ease);
  }
  .pill:hover { background: var(--bg-card); }
  .pill .count { font-weight: 600; }
  .pill .label { color: var(--ink-muted); font-size: var(--t-xs); }

  .indicator.expanded {
    width: min(340px, 88vw);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: 0 18px 40px rgba(0,0,0,0.45);
    overflow: hidden;
    animation: slide-up 200ms var(--ease);
  }
  @keyframes slide-up {
    from { opacity: 0; transform: translateY(8px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  header {
    display: flex; align-items: center; gap: var(--s-2);
    padding: var(--s-3) var(--s-3) var(--s-2);
    border-bottom: 1px solid var(--line-soft);
  }
  header .title { flex: 1; font-size: var(--t-sm); font-weight: 600; color: var(--ink); }
  .icon-btn {
    width: 22px; height: 22px;
    background: transparent; border: none;
    color: var(--ink-muted);
    border-radius: var(--r-sm);
    display: inline-flex; align-items: center; justify-content: center;
    cursor: pointer;
  }
  .icon-btn:hover { background: var(--bg-card); color: var(--ink); }
  .icon-btn.cancel {
    width: 22px; height: 22px;
    color: var(--hot, #d05a4a);
    background: color-mix(in oklab, var(--hot, #d05a4a) 12%, transparent);
    border: 1px solid color-mix(in oklab, var(--hot, #d05a4a) 36%, transparent);
  }
  .icon-btn.cancel:hover {
    color: var(--bg);
    background: var(--hot, #d05a4a);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: var(--s-2);
    display: flex; flex-direction: column;
    gap: var(--s-2);
    max-height: 60vh;
    overflow-y: auto;
  }
  .list li {
    background: var(--bg);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-sm);
    padding: 6px 10px 8px;
    display: flex; flex-direction: column; gap: 4px;
  }
  .row {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--t-sm);
  }
  .row .t { flex: 1; color: var(--ink); font-weight: 500; }
  .row .pct { color: var(--ink-muted); font-size: var(--t-xs); }
  .track {
    height: 3px;
    background: var(--bg-card);
    border-radius: 2px;
    overflow: hidden;
  }
  .track .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms var(--ease);
  }
  .sub {
    display: flex; align-items: center; gap: var(--s-2);
    font-size: var(--t-xs);
    color: var(--ink-muted);
    overflow: hidden;
  }
  .eta { color: var(--accent); font-weight: 500; }
  .chip {
    display: inline-flex;
    align-items: center;
    padding: 1px 7px;
    border-radius: 999px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    line-height: 1.5;
    letter-spacing: 0.02em;
    cursor: help;
    flex-shrink: 0;
  }
  .chip[data-route="bridge"] {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 16%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
  }
  .chip[data-route="local"] {
    color: var(--ink-muted);
    background: var(--bg-card);
    border: 1px solid var(--line);
  }
  .msg {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  :global(.spin) {
    color: var(--accent);
    animation: spin 1.2s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
