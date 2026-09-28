import { afterEach, expect, it, vi } from "vitest";
import { finishSlotTransition } from "./slideTransition";
afterEach(() => vi.useRealTimers());

it("does not permit slot reuse before the outgoing animation finishes", async () => {
  let finish!: () => void;
  const cancel = vi.fn();
  const node = { getAnimations: () => [{ finished: new Promise<void>((r) => { finish = r; }), cancel }] };
  let reusable = false;
  const pending = finishSlotTransition(node as unknown as Element).then(() => { reusable = true; });
  await Promise.resolve();
  expect(reusable).toBe(false);
  finish();
  await pending;
  expect(reusable).toBe(true);
  expect(cancel).not.toHaveBeenCalled();
});

it("cancels stuck animations before releasing the slot; supports reduced motion", async () => {
  vi.useFakeTimers();
  const cancel = vi.fn();
  const node = { getAnimations: () => [{ finished: new Promise(() => {}), cancel }] };
  const pending = finishSlotTransition(node as unknown as Element, 50);
  await vi.advanceTimersByTimeAsync(50);
  await pending;
  expect(cancel).toHaveBeenCalledOnce();
  await finishSlotTransition(null);
});
