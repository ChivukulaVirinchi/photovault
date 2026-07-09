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
});
