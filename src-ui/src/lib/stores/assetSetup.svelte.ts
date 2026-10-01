import { listen } from "@tauri-apps/api/event";
import { systemEx, people, geocoding } from "../api/all";
import { commandErrorMessage } from "../api";
import { libraryStore } from "./library.svelte";
import { jobs } from "./jobs.svelte";

export type AssetFeature = "all" | "faces" | "visual" | "places";
const INTRO_KEY = "smriti.smart-setup.seen";

class AssetSetupStore {
  ready = $state<{ faces: boolean; visual: boolean; places: boolean } | null>(null);
  errors = $state<Partial<Record<AssetFeature, string>>>({});
  requesting = $state(false);
  introSeen = $state(localStorage.getItem(INTRO_KEY) === "1");
  private pending: { feature: AssetFeature; root: string | null; session: number; installed: boolean } | null = null;
  private jobId: string | null = null;

  async refresh() {
    this.ready = await systemEx.assetSetupStatus();
    if (Object.values(this.ready).every(Boolean)) this.dismissIntro();
  }

  async install() {
    const unlisten = await Promise.all([
      listen<{ job_id: string }>("assets:complete", async ({ payload }) => {
        await this.refresh();
        if (this.pending && payload.job_id === this.jobId) this.pending.installed = true;
        this.resume();
      }),
      listen<{ job_id: string; stage: string; message?: string }>("assets:progress", ({ payload }) => {
        if (payload.stage === "error" && payload.job_id === this.jobId && this.pending) {
          this.errors = { ...this.errors, [this.pending.feature]: payload.message ?? "Setup failed. Please retry." };
          this.pending = null;
        }
      }),
    ]);
    try { await this.refresh(); }
    catch (error) { unlisten.forEach((stop) => stop()); throw error; }
    return () => unlisten.forEach((stop) => stop());
  }

  dismissIntro() {
    this.introSeen = true;
    localStorage.setItem(INTRO_KEY, "1");
  }

  async enable(feature: AssetFeature) {
    if (this.requesting || jobs.isRunning("assets")) return;
    this.dismissIntro();
    this.requesting = true;
    this.errors = { ...this.errors, all: undefined, [feature]: undefined };
    this.pending = { feature, root: libraryStore.driveRoot, session: libraryStore.session, installed: false };
    try {
      await jobs.install();
      const { job_id } = await systemEx.installAssets(feature);
      this.jobId = job_id;
      jobs.register(job_id, "assets");
      // A no-op setup can finish before its command response arrives.
      const job = jobs.jobs.get(job_id);
      if (job?.status === "complete") {
        await this.refresh();
        if (this.pending) this.pending.installed = true;
        this.resume();
      } else if (job?.status === "error") {
        throw job.message ?? "Setup failed. Please retry.";
      }
    } catch (error) {
      this.errors = { ...this.errors, [feature]: commandErrorMessage(error) };
      this.pending = null;
    } finally { this.requesting = false; }
  }

  resume() {
    const pending = this.pending;
    if (!pending?.installed) return;
    if (pending.root !== libraryStore.driveRoot || pending.session !== libraryStore.session || !libraryStore.isOpen) {
      this.pending = null;
      return;
    }
    if (jobs.isRunning("scan") || jobs.isRunning("metadata") || jobs.isRunning("takeout")) return;
    this.pending = null;
    const start = async () => {
      for (const feature of ["faces", "places"] as const) {
        if (pending.session !== libraryStore.session || pending.root !== libraryStore.driveRoot) return;
        const kind = feature === "faces" ? "faces" : "geocoding";
        if ((pending.feature !== "all" && pending.feature !== feature) || jobs.isRunning(kind)) continue;
        try {
          const result = feature === "faces"
            ? await people.startProcessing(pending.session)
            : await geocoding.backfill(false, pending.session);
          if (pending.session === libraryStore.session) jobs.register(result.job_id, kind);
        } catch (error) {
          this.errors = { ...this.errors, [feature]: commandErrorMessage(error) };
        }
      }
    };
    void start();
  }
}

export const assetSetup = new AssetSetupStore();
