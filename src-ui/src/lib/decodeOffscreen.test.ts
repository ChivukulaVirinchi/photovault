import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { decodeOffscreen } from "./decodeOffscreen";

/**
 * A scriptable fake Image. We control whether `onload`, `onerror`, or
 * neither fires after `src` is assigned — that's the exact shape of
 * the Tauri / WebView2 hang the production code defends against.
 */
class FakeImage {
  static behaviour: "load" | "error" | "silent" = "load";
  static delay = 0;
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private _src = "";
  get src(): string {
    return this._src;
  }
  set src(value: string) {
    this._src = value;
    const behaviour = FakeImage.behaviour;
    setTimeout(() => {
      if (behaviour === "load") this.onload?.();
      else if (behaviour === "error") this.onerror?.();
      // "silent" → neither callback ever fires. The timeout in
      // decodeOffscreen is what saves the caller.
    }, FakeImage.delay);
  }
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeImage.behaviour = "load";
  FakeImage.delay = 0;
});
afterEach(() => {
  vi.useRealTimers();
});

describe("decodeOffscreen", () => {
  it("resolves when the fake Image fires onload", async () => {
    FakeImage.behaviour = "load";
    const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image);
    await vi.runAllTimersAsync();
    await expect(p).resolves.toBeUndefined();
  });

  it("rejects when the fake Image fires onerror", async () => {
    FakeImage.behaviour = "error";
    const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image);
    // Pre-attach the rejection assertion BEFORE running timers so the
    // rejection has a handler the moment it's emitted — otherwise
    // node logs an unhandled-rejection warning even though we'd be
    // awaiting it on the next line.
    const expectation = expect(p).rejects.toThrow("decode failed");
    await vi.runAllTimersAsync();
    await expectation;
  });

  it(
    "does NOT hang when the Image stays silent — resolves after the timeout " +
      "elapses (regression for the Tauri/WebView2 decode hang)",
    async () => {
      // The exact failure mode the production code defends against:
      // some WebView2 builds drop ICC-profiled JPEGs and never fire
      // onload OR onerror. Without the timeout, decodeOffscreen
      // would await forever and the slideshow would dead-lock.
      FakeImage.behaviour = "silent";
      const timeoutMs = 8000;
      const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image, timeoutMs);

      // Advance time up to but not including the timeout — the
      // promise must still be pending.
      let resolved = false;
      p.then(() => (resolved = true));
      await vi.advanceTimersByTimeAsync(timeoutMs - 1);
      expect(resolved).toBe(false);

      // Crossing the timeout boundary must resolve, not reject —
      // the slideshow can recover by trusting the visible <img>'s
      // own decoding="async" rather than blocking.
      await vi.advanceTimersByTimeAsync(2);
      await expect(p).resolves.toBeUndefined();
    },
  );

  it("clears the timeout once onload fires so subsequent calls don't leak", async () => {
    // Spy on clearTimeout to make sure the success path tears down
    // its safety net. A leak here would log noise in long sessions
    // but is otherwise harmless; the test pins the intent.
    const spy = vi.spyOn(globalThis, "clearTimeout");
    FakeImage.behaviour = "load";
    const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image);
    await vi.runAllTimersAsync();
    await p;
    expect(spy).toHaveBeenCalled();
    spy.mockRestore();
  });
});
