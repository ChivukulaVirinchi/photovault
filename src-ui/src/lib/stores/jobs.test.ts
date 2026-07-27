import { beforeEach, describe, expect, it } from "vitest";

import { jobs, type Job } from "./jobs.svelte";

function job(overrides: Partial<Job>): Job {
  return {
    id: "job",
    kind: "scan",
    title: "Job",
    message: null,
    processed: 0,
    total: null,
    elapsed_ms: 0,
    status: "running",
    ...overrides,
  };
}

beforeEach(() => {
  jobs.jobs = new Map();
  (jobs as unknown as { suppressedLibraryJobIds: Set<string> }).suppressedLibraryJobIds =
    new Set();
});

describe("jobs store", () => {
  it("byKind prefers a running job over a lingering completed job", () => {
    jobs.jobs = new Map([
      ["old", job({ id: "old", status: "complete", elapsed_ms: 90_000 })],
      ["new", job({ id: "new", status: "running", elapsed_ms: 1 })],
    ]);

    expect(jobs.byKind("scan")?.id).toBe("new");
  });

  it("byKind still chooses the farthest-running job when several are active", () => {
    jobs.jobs = new Map([
      ["a", job({ id: "a", elapsed_ms: 10 })],
      ["b", job({ id: "b", elapsed_ms: 20 })],
    ]);

    expect(jobs.byKind("scan")?.id).toBe("b");
  });

  it("clears library-scoped jobs without dropping install jobs", () => {
    jobs.jobs = new Map([
      ["scan", job({ id: "scan", kind: "scan" })],
      ["thumbs", job({ id: "thumbs", kind: "thumbnails" })],
      ["takeout", job({ id: "takeout", kind: "takeout" })],
      ["assets", job({ id: "assets", kind: "assets" })],
      ["semantic-install", job({ id: "semantic-install", kind: "semantic", stage: "download" })],
      ["semantic-index", job({ id: "semantic-index", kind: "semantic", stage: "index" })],
      ["pending-semantic-index-1", job({ id: "pending-semantic-index-1", kind: "semantic" })],
    ]);

    jobs.clearLibraryScoped();

    expect(Array.from(jobs.jobs.keys())).toEqual(["assets", "semantic-install"]);
  });

  it("ignores late progress from a cleared library-scoped job", () => {
    jobs.jobs = new Map([
      ["scan", job({ id: "scan", kind: "scan" })],
    ]);

    jobs.clearLibraryScoped();
    (jobs as unknown as {
      applyWire: (kind: string, complete: boolean, payload: unknown) => void;
    }).applyWire("scan", false, {
      job_id: "scan",
      processed: 10,
      total: 100,
      message: "old library",
    });

    expect(jobs.jobs.has("scan")).toBe(false);
  });

  it("still accepts non-library progress after library-scoped jobs are cleared", () => {
    jobs.jobs = new Map([
      ["scan", job({ id: "scan", kind: "scan" })],
      ["assets", job({ id: "assets", kind: "assets" })],
    ]);

    jobs.clearLibraryScoped();
    (jobs as unknown as {
      applyWire: (kind: string, complete: boolean, payload: unknown) => void;
    }).applyWire("assets", false, {
      job_id: "assets",
      processed: 1,
      total: 3,
      message: "installing",
    });

    expect(jobs.jobs.get("assets")?.message).toBe("installing");
  });

  it("does not let a late semantic-index event recreate a cleared library job", () => {
    jobs.jobs = new Map([
      ["semantic-index", job({ id: "semantic-index", kind: "semantic", stage: "index" })],
    ]);

    jobs.clearLibraryScoped();
    (jobs as unknown as {
      applyWire: (kind: string, complete: boolean, payload: unknown) => void;
    }).applyWire("semantic", false, {
      job_id: "semantic-index",
      stage: "index",
      processed: 16,
      message: "old library",
    });

    expect(jobs.jobs.has("semantic-index")).toBe(false);
  });
});
