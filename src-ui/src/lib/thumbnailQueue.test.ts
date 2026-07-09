import { describe, expect, it } from "vitest";

import { createThumbnailQueue } from "./thumbnailQueue";

/// Helper for tests that need a manually-resolvable promise.
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

/// Drain microtasks so queued continuations run. Used after we
/// resolve a deferred promise — vitest doesn't auto-await microtasks
/// between assertions.
async function flush() {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("thumbnailQueue priority handling", () => {
  it("runs the highest-priority queued item next after the in-flight one resolves", async () => {
    const first = deferred<string | null>();
    const second = deferred<string | null>();
    const started: number[] = [];

    const queue = createThumbnailQueue(async (id) => {
      started.push(id);
      if (id === 1) return first.promise;
      if (id === 3) return second.promise;
      return `thumb-${id}`;
    }, 1);

    const ready: string[] = [];
    queue.enqueue(1, (path) => ready.push(path), 1);
    queue.enqueue(2, (path) => ready.push(path), 2);
    queue.enqueue(3, (path) => ready.push(path), 9); // highest priority

    expect(started).toEqual([1]);

    first.resolve("thumb-1");
    await flush();
    expect(started).toEqual([1, 3]);

    second.resolve("thumb-3");
    await flush();
    expect(started).toEqual([1, 3, 2]);
    expect(ready).toEqual(["thumb-1", "thumb-3", "thumb-2"]);
  });

  it("reprioritizes an already-queued item when it becomes visible", async () => {
    const first = deferred<string | null>();
    const started: number[] = [];

    const queue = createThumbnailQueue(async (id) => {
      started.push(id);
      if (id === 1) return first.promise;
      return `thumb-${id}`;
    }, 1);

    const ready: string[] = [];
    queue.enqueue(1, (path) => ready.push(path), 1);
    queue.enqueue(2, (path) => ready.push(path), 2);
    queue.enqueue(3, (path) => ready.push(path), 3);
    queue.enqueue(2, (path) => ready.push(`again:${path}`), 99);

    first.resolve("thumb-1");
    await flush();

    expect(started).toEqual([1, 2, 3]);
    expect(ready).toEqual(["thumb-1", "thumb-2", "again:thumb-2", "thumb-3"]);
  });
});

describe("thumbnailQueue deduplication", () => {
  it("coalesces duplicate requests for the same photo id", async () => {
    const gate = deferred<string | null>();
    let calls = 0;
    const queue = createThumbnailQueue(async () => {
      calls += 1;
      return gate.promise;
    }, 2);

    const ready: string[] = [];
    queue.enqueue(42, (path) => ready.push(`a:${path}`), 1);
    queue.enqueue(42, (path) => ready.push(`b:${path}`), 5);

    gate.resolve("thumb-42");
    await flush();

    expect(calls).toBe(1);
    expect(ready).toEqual(["a:thumb-42", "b:thumb-42"]);
  });
});

describe("thumbnailQueue batching", () => {
  it("batches queued photo requests by priority", async () => {
    const singleStarted: number[] = [];
    const batches: number[][] = [];

    const queue = createThumbnailQueue(
      async (id) => {
        singleStarted.push(id);
        return `thumb-${id}`;
      },
      1,
      async (ids) => {
        batches.push(ids);
        return new Map(ids.map((id) => [id, `thumb-${id}`]));
      },
      2,
    );

    const ready: string[] = [];
    queue.enqueue(1, (path) => ready.push(path), 1);
    queue.enqueue(2, (path) => ready.push(path), 2);
    queue.enqueue(3, (path) => ready.push(path), 9);
    queue.enqueue(4, (path) => ready.push(path), 5);

    await flush();

    expect(singleStarted).toEqual([]);
    expect(batches).toEqual([[3, 4], [2, 1]]);
    expect(ready).toEqual(["thumb-3", "thumb-4", "thumb-2", "thumb-1"]);
  });

  it("does not mix distant low-priority prefetches into a visible batch", async () => {
    const singleStarted: number[] = [];
    const batches: number[][] = [];
    const queue = createThumbnailQueue(
      async (id) => {
        singleStarted.push(id);
        return `thumb-${id}`;
      },
      1,
      async (ids) => {
        batches.push(ids);
        return new Map(ids.map((id) => [id, `thumb-${id}`]));
      },
      4,
    );

    queue.enqueue(1, () => {}, 1_000_000);
    queue.enqueue(2, () => {}, 10);
    queue.enqueue(3, () => {}, 9);

    await flush();

    expect(singleStarted).toEqual([1]);
    expect(batches).toEqual([[2, 3]]);
  });

  it("does not batch visible requests behind a slower visible neighbor", async () => {
    const singleStarted: number[] = [];
    const batches: number[][] = [];
    const queue = createThumbnailQueue(
      async (id) => {
        singleStarted.push(id);
        return `thumb-${id}`;
      },
      1,
      async (ids) => {
        batches.push(ids);
        return new Map(ids.map((id) => [id, `thumb-${id}`]));
      },
      4,
    );

    queue.enqueue(1, () => {}, 1_000, "photo", false);
    queue.enqueue(2, () => {}, 999, "photo", false);
    queue.enqueue(3, () => {}, 10, "photo", true);
    queue.enqueue(4, () => {}, 9, "photo", true);

    await flush();

    expect(singleStarted).toEqual([1, 2]);
    expect(batches).toEqual([[3, 4]]);
  });

  it("starts visible requests even when prefetch batches fill normal slots", async () => {
    const batchGate = deferred<Map<number, string | null>>();
    const singleStarted: number[] = [];
    const batches: number[][] = [];
    const queue = createThumbnailQueue(
      async (id) => {
        singleStarted.push(id);
        return `thumb-${id}`;
      },
      1,
      async (ids) => {
        batches.push(ids);
        return batchGate.promise;
      },
      4,
    );

    queue.enqueue(1, () => {}, 10, "photo", true);
    queue.enqueue(2, () => {}, 9, "photo", true);
    await flush();
    expect(batches).toEqual([[1, 2]]);

    queue.enqueue(3, () => {}, 1_000_000, "photo", false);
    await flush();

    expect(singleStarted).toEqual([3]);
    batchGate.resolve(new Map([[1, "thumb-1"], [2, "thumb-2"]]));
  });

  it("lets a viewport jump start up to one full urgent window", async () => {
    const batchGate = deferred<Map<number, string | null>>();
    const urgentGate = deferred<string | null>();
    const singleStarted: number[] = [];
    const batches: number[][] = [];
    const queue = createThumbnailQueue(
      async (id) => {
        singleStarted.push(id);
        return urgentGate.promise;
      },
      4,
      async (ids) => {
        batches.push(ids);
        return batchGate.promise;
      },
      2,
    );

    for (const id of [1, 2, 3, 4, 5, 6, 7, 8]) queue.enqueue(id, () => {}, id, "photo", true);
    await flush();
    expect(batches).toEqual([
      [8, 7],
      [6, 5],
      [4, 3],
      [2, 1],
    ]);

    for (const id of [101, 102, 103, 104]) queue.enqueue(id, () => {}, 1_000_000 + id, "photo", false);
    queue.enqueue(105, () => {}, 1_000_105, "photo", false);
    await flush();

    expect(singleStarted).toEqual([105, 104, 103, 102]);
    urgentGate.resolve("thumb");
    batchGate.resolve(new Map());
  });
});

describe("thumbnailQueue cancellation", () => {
  it("does not start or report a canceled queued item", async () => {
    const gate = deferred<string | null>();
    const started: number[] = [];

    const queue = createThumbnailQueue(async (id) => {
      started.push(id);
      if (id === 1) return gate.promise;
      return `thumb-${id}`;
    }, 1);

    const ready: string[] = [];
    queue.enqueue(1, (path) => ready.push(path), 1);
    const cancel = queue.enqueue(2, (path) => ready.push(path), 2);
    cancel();

    gate.resolve("thumb-1");
    await flush();

    expect(started).toEqual([1]);
    expect(ready).toEqual(["thumb-1"]);
    expect(queue.pendingCount()).toBe(0);
  });
});
