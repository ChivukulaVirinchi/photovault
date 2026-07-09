import { assistant, type AssistantRun } from "../api/all";

export interface AssistantActivityEvent {
  run_id: string;
  library_root: string;
  label: string;
}

class AssistantStore {
  open = $state(false);
  run = $state<AssistantRun | null>(null);
  activity = $state<Array<{ label: string }>>([]);
  busy = $state(false);
  error = $state<string | null>(null);
  private seq = 0;

  show() {
    this.open = true;
    setTimeout(() => {
      document.dispatchEvent(new CustomEvent("smriti:assistant-focus"));
    }, 0);
  }

  hide() {
    this.open = false;
  }

  async start(message: string) {
    const trimmed = message.trim();
    if (!trimmed) return;
    const seq = ++this.seq;
    const previousRun = this.run;
    const previousActivity = this.activity;
    this.busy = true;
    this.error = null;
    this.activity = [];
    let createdOptimisticRun = false;
    try {
      if (
        this.run &&
        !["stopped", "failed"].includes(this.run.status)
      ) {
        const nextRun = await assistant.continueRun(this.run.run_id, trimmed);
        if (seq !== this.seq) return;
        this.run = nextRun;
      } else {
        const runId = `assistant-${Date.now()}-${Math.random().toString(36).slice(2)}`;
        createdOptimisticRun = true;
        this.run = {
          run_id: runId,
          library_root: "",
          status: "running",
          message: trimmed,
          response: null,
          clarification_options: [],
          activity: [],
          preview: null,
          album_id: null,
        };
        const nextRun = await assistant.start(trimmed, runId);
        if (seq !== this.seq) return;
        this.run = nextRun;
      }
      if (this.run) this.activity = mergeActivity(this.activity, this.run.activity);
    } catch (e) {
      if (seq === this.seq) {
        if (createdOptimisticRun) {
          this.run = previousRun;
          this.activity = previousActivity;
        }
        this.error = formatError(e);
      }
    } finally {
      if (seq === this.seq) this.busy = false;
    }
  }

  async stop() {
    if (!this.run) return;
    const seq = ++this.seq;
    this.busy = true;
    try {
      const nextRun = await assistant.stop(this.run.run_id);
      if (seq !== this.seq) return;
      this.run = nextRun;
      this.activity = mergeActivity(this.activity, this.run.activity);
    } catch (e) {
      if (seq === this.seq) this.error = formatError(e);
    } finally {
      if (seq === this.seq) this.busy = false;
    }
  }

  async choose(option: string) {
    await this.start(option);
  }

  async approve() {
    const preview = this.run?.preview;
    if (!this.run || !preview) return;
    const seq = ++this.seq;
    this.busy = true;
    this.error = null;
    try {
      const nextRun = await assistant.approve(this.run.run_id, preview.approval_id);
      if (seq !== this.seq) return;
      this.run = nextRun;
      this.activity = mergeActivity(this.activity, this.run.activity);
      if (this.run.album_id) window.location.hash = `/album?id=${this.run.album_id}`;
    } catch (e) {
      if (seq === this.seq) this.error = formatError(e);
    } finally {
      if (seq === this.seq) this.busy = false;
    }
  }

  async reject() {
    const preview = this.run?.preview;
    if (!this.run || !preview) return;
    const seq = ++this.seq;
    this.busy = true;
    try {
      const nextRun = await assistant.reject(this.run.run_id, preview.approval_id);
      if (seq !== this.seq) return;
      this.run = nextRun;
      this.activity = mergeActivity(this.activity, this.run.activity);
    } catch (e) {
      if (seq === this.seq) this.error = formatError(e);
    } finally {
      if (seq === this.seq) this.busy = false;
    }
  }

  async clear() {
    this.seq += 1;
    this.run = null;
    this.activity = [];
    this.error = null;
    try {
      await assistant.clear();
    } catch {
      // Local clear is enough for the user-visible state.
    }
  }

  appendActivity(event: AssistantActivityEvent) {
    if (!this.run) return;
    if (this.run.run_id !== event.run_id) return;
    if (this.run.library_root && this.run.library_root !== event.library_root) return;
    const item = { label: event.label };
    this.activity = mergeActivity(this.activity, [item]);
  }

  resetForLibrary() {
    this.seq += 1;
    this.open = false;
    this.run = null;
    this.activity = [];
    this.error = null;
    this.busy = false;
    assistant.clear().catch(() => {});
  }
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const any = e as Record<string, unknown>;
    if (typeof any.reason === "string") return any.reason;
    if (typeof any.message === "string") return any.message;
    if (typeof any.kind === "string") return any.kind;
  }
  return "Assistant request failed.";
}

function mergeActivity(
  current: Array<{ label: string }>,
  next: Array<{ label: string }>,
): Array<{ label: string }> {
  const out = current.slice();
  for (const item of next) {
    if (out[out.length - 1]?.label !== item.label && !out.some((x) => x.label === item.label)) {
      out.push(item);
    }
  }
  return out;
}

export const assistantStore = new AssistantStore();
