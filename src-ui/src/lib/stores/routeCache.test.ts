import { beforeEach, describe, expect, it } from "vitest";

import { libraryStore } from "./library.svelte";
import { routeCache } from "./routeCache.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  libraryStore.driveRoot = "/drv";
  libraryStore.session = 1;
  routeCache.clear();
});

describe("routeCache deferred-load races", () => {
  it("caches a resolved answer for later callers", async () => {
    let loads = 0;
    const a = await routeCache.get("trash:page", async () => {
      loads += 1;
      return "v1";
    });
    const b = await routeCache.get("trash:page", async () => {
      loads += 1;
      return "v2";
    });
    expect(a).toBe("v1");
    expect(b).toBe("v1");
    expect(loads).toBe(1);
    expect(routeCache.peek("trash:page")).toBe("v1");
  });

  it("evicts the oldest route entries at the fixed memory bound", () => {
    for (let i = 0; i < 64; i += 1) routeCache.put(`bounded:${i}`, i);
    expect(routeCache.peek("bounded:0")).toBe(0);
    routeCache.put("bounded:64", 64);
    expect(routeCache.peek("bounded:0")).toBeNull();
    expect(routeCache.peek("bounded:64")).toBe(64);
  });

  it("shares one in-flight promise between concurrent callers", async () => {
    const d = deferred<string>();
    const p1 = routeCache.get("k", () => d.promise);
    const p2 = routeCache.get("k", () => d.promise);
    expect(p2).toBe(p1);
    d.resolve("shared");
    await expect(p2).resolves.toBe("shared");
  });

  it("a deferred answer cannot refill the cache after prefix invalidation", async () => {
    const d1 = deferred<string>();
    const p1 = routeCache.get("trash:page", () => d1.promise);
    routeCache.invalidate("trash");

    // The invalidated promise must not be shared with new callers.
    let loads = 0;
    const d2 = deferred<string>();
    const p2 = routeCache.get("trash:page", () => {
      loads += 1;
      return d2.promise;
    });
    expect(p2).not.toBe(p1);
    expect(loads).toBe(1);

    // The old (pre-invalidation) request finishes late: its caller still
    // receives the answer, but the cache must not adopt it.
    d1.resolve("old");
    await expect(p1).resolves.toBe("old");
    expect(routeCache.peek("trash:page")).toBeNull();

    d2.resolve("new");
    await expect(p2).resolves.toBe("new");
    expect(routeCache.peek("trash:page")).toBe("new");
  });

  it("a deferred answer cannot refill the cache after invalidateAll", async () => {
    const d1 = deferred<string>();
    const p1 = routeCache.get("people:clusters", () => d1.promise);
    routeCache.invalidateAll();

    const d2 = deferred<string>();
    const p2 = routeCache.get("people:clusters", () => d2.promise);
    expect(p2).not.toBe(p1);

    d1.resolve("old");
    await expect(p1).resolves.toBe("old");
    expect(routeCache.peek("people:clusters")).toBeNull();

    d2.resolve("new");
    await p2;
    expect(routeCache.peek("people:clusters")).toBe("new");
  });

  it("a deferred answer cannot survive a library switch", async () => {
    const d1 = deferred<string>();
    const p1 = routeCache.get("duplicates:groups", () => d1.promise);

    // Session changes (close + reopen, or switch to another library).
    libraryStore.session = 2;

    const d2 = deferred<string>();
    const p2 = routeCache.get("duplicates:groups", () => d2.promise);
    expect(p2).not.toBe(p1);

    d1.resolve("old-session");
    await expect(p1).resolves.toBe("old-session");
    expect(routeCache.peek("duplicates:groups")).toBeNull();

    d2.resolve("new-session");
    await p2;
    expect(routeCache.peek("duplicates:groups")).toBe("new-session");
  });

  it("a rejected loader is not cached and a late rejection cannot evict a newer request", async () => {
    // Without invalidation: rejection is not negative-cached.
    const d1 = deferred<string>();
    const p1 = routeCache.get("k", () => d1.promise);
    d1.reject(new Error("boom"));
    await expect(p1).rejects.toThrow("boom");
    const p1b = routeCache.get("k", async () => "retry");
    await expect(p1b).resolves.toBe("retry");

    // With invalidation: a late rejection of the orphaned promise must
    // not delete the newer pending request's slot.
    const dOrphan = deferred<string>();
    const pOrphan = routeCache.get("k2", () => dOrphan.promise);
    routeCache.invalidate("k2");
    const d2 = deferred<string>();
    const p2 = routeCache.get("k3", () => d2.promise);
    expect(p2).not.toBe(pOrphan);
    dOrphan.reject(new Error("late boom"));
    await expect(pOrphan).rejects.toThrow("late boom");
    // The orphan's cleanup must not have removed the newer pending slot.
    const pShared = routeCache.get("k3", () => d2.promise);
    expect(pShared).toBe(p2);
    d2.resolve("fresh");
    await expect(p2).resolves.toBe("fresh");
    expect(routeCache.peek("k3")).toBe("fresh");
  });

  it("an orphaned promise finishing later cannot evict the newer pending slot", async () => {
    const d1 = deferred<string>();
    const p1 = routeCache.get("map:pins", () => d1.promise);
    routeCache.invalidate("map");

    const d2 = deferred<string>();
    const p2 = routeCache.get("map:pins", () => d2.promise);

    // Orphan finishes after the new request is already pending.
    d1.resolve("old");
    await p1;
    // New request still pending — the orphan's cleanup must not have
    // removed its inflight entry, so callers still share it.
    const p3 = routeCache.get("map:pins", () => d2.promise);
    expect(p3).toBe(p2);

    d2.resolve("new");
    await p3;
    expect(routeCache.peek("map:pins")).toBe("new");
  });
});
