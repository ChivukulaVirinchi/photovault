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
    this.busy = true;
    this.error = null;
    this.activity = [];
    try {
      if (
        this.run &&
        !["stopped", "failed"].includes(this.run.status)
      ) {
        this.run = await assistant.continueRun(this.run.run_id, trimmed);
      } else {
        const runId = `assistant-${Date.now()}-${Math.random().toString(36).slice(2)}`;
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
        this.run = await assistant.start(trimmed, runId);
      }
      if (this.run) this.activity = mergeActivity(this.activity, this.run.activity);
    } catch (e) {
      this.error = formatError(e);
    } finally {
      this.busy = false;
    }
  }

  async stop() {
    if (!this.run) return;
    this.busy = false;
    try {
      this.run = await assistant.stop(this.run.run_id);
      this.activity = mergeActivity(this.activity, this.run.activity);
    } catch (e) {
      this.error = formatError(e);
    } finally {
      this.busy = false;
    }
  }

  async choose(option: string) {
    await this.start(option);
  }

  async approve() {
    const preview = this.run?.preview;
    if (!this.run || !preview) return;
    this.busy = true;
    this.error = null;
    try {
      this.run = await assistant.approve(this.run.run_id, preview.approval_id);
      this.activity = mergeActivity(this.activity, this.run.activity);
      if (this.run.album_id) window.location.hash = `/album?id=${this.run.album_id}`;
    } catch (e) {
      this.error = formatError(e);
    } finally {
      this.busy = false;
    }
  }

  async reject() {
    const preview = this.run?.preview;
    if (!this.run || !preview) return;
    this.busy = true;
    try {
      this.run = await assistant.reject(this.run.run_id, preview.approval_id);
      this.activity = mergeActivity(this.activity, this.run.activity);
    } catch (e) {
      this.error = formatError(e);
    } finally {
      this.busy = false;
    }
  }

  async clear() {
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
    if (this.run && this.run.run_id !== event.run_id) return;
    if (this.run && this.run.library_root && this.run.library_root !== event.library_root) return;
    const item = { label: event.label };
    this.activity = mergeActivity(this.activity, [item]);
  }

  resetForLibrary() {
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
