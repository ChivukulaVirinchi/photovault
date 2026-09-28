import { describe, expect, it, vi } from "vitest";
import { drainSearchPages } from "./searchPaging";
import { gridWindow } from "./gridWindow";

describe("automatic search paging", () => {
  it("stops immediately for a cancelled input", async () => {
    const next = vi.fn(async () => true);
    await drainSearchPages(() => false, next);
    expect(next).not.toHaveBeenCalled();
  });
  it("does not spin on a failed or non-progressing page", async () => {
    const next = vi.fn(async () => false);
    await drainSearchPages(() => true, next);
    expect(next).toHaveBeenCalledTimes(1);
  });
  it("yields a real task and notices input cancellation before the next page", async () => {
    let current = true;
    const next = vi.fn(async () => true);
    setTimeout(() => { current = false; }, 0);
    await drainSearchPages(() => current, next);
    expect(next).toHaveBeenCalledTimes(1);
  });
  it("leaves rejected loaders rejected instead of retrying forever", async () => {
    const next = vi.fn(async () => { throw Error("offline"); });
    await expect(drainSearchPages(() => true, next)).rejects.toThrow("offline");
    expect(next).toHaveBeenCalledTimes(1);
  });
});

describe("continuous search grid", () => {
  it("reaches the last of 20,000 photos with bounded mounted cells", () => {
    const first = gridWindow(20_000, 1000, 0, 800);
    expect(first.start).toBe(0);
    expect(first.end).toBeLessThan(100);
    const last = gridWindow(20_000, 1000, 3_333 * (1004 / 6), 800);
    expect(last.end).toBe(20_000);
    expect(last.end - last.start).toBeLessThan(100);
    expect(last.paddingTop).toBeGreaterThan(0);
  });
  it("handles empty, narrow and above-the-fold grids", () => {
    expect(gridWindow(0, 1000, 0, 800).end).toBe(0);
    expect(gridWindow(20, 50, 0, 100).columns).toBe(1);
    expect(gridWindow(20, 1000, -1000, 100).start).toBe(0);
  });
});
