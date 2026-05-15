import { createThumbnailQueue } from "./thumbnailQueue.js";

function equal<T>(actual: T, expected: T, label: string) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

function deepEqual<T>(actual: T, expected: T, label: string) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

async function runsHighestPriorityNext() {
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
  queue.enqueue(3, (path) => ready.push(path), 9);
  deepEqual(started, [1], "starts first item immediately");

  first.resolve("thumb-1");
  await flush();
  deepEqual(started, [1, 3], "starts highest priority queued item next");

  second.resolve("thumb-3");
  await flush();
  deepEqual(started, [1, 3, 2], "falls back to lower priority after urgent item");
  deepEqual(ready, ["thumb-1", "thumb-3", "thumb-2"], "reports ready paths in completion order");
}

async function coalescesDuplicateRequests() {
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

  equal(calls, 1, "coalesces duplicate loader calls");
  deepEqual(ready, ["a:thumb-42", "b:thumb-42"], "fans out coalesced result");
}

async function cancellationDropsQueuedHandlers() {
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
  deepEqual(started, [1], "does not start canceled queued item");
  deepEqual(ready, ["thumb-1"], "does not call canceled handler");
  equal(queue.pendingCount(), 0, "clears canceled queued entry");
}

await runsHighestPriorityNext();
await coalescesDuplicateRequests();
await cancellationDropsQueuedHandlers();
console.log("thumbnailQueue tests passed");
