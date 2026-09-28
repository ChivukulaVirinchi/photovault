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
  it("keeps the deadline active after load while decode is hung", async () => {
    class HungDecodeImage extends FakeImage {
      decode() { return new Promise<void>(() => {}); }
    }
    const p = decodeOffscreen("blob:hung", HungDecodeImage as unknown as typeof Image, { timeoutMs: 50 });
    const expectation = expect(p).rejects.toThrow("readiness timed out");
    await vi.advanceTimersByTimeAsync(51);
    await expectation;
  });

  it("catches a synchronous decode failure", async () => {
    class BrokenDecodeImage extends FakeImage {
      decode(): Promise<void> { throw new Error("broken decoder"); }
    }
    const p = decodeOffscreen("blob:broken", BrokenDecodeImage as unknown as typeof Image);
    const expectation = expect(p).rejects.toThrow("broken decoder");
    await vi.runAllTimersAsync();
    await expectation;
  });
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
    "does NOT hang when the Image stays silent — rejects after the timeout " +
      "elapses (regression for the Tauri/WebView2 decode hang)",
    async () => {
      // The exact failure mode the production code defends against:
      // some WebView2 builds drop ICC-profiled JPEGs and never fire
      // onload OR onerror. Without the timeout, decodeOffscreen
      // would await forever and the slideshow would dead-lock.
      FakeImage.behaviour = "silent";
      const timeoutMs = 8000;
      const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image, { timeoutMs });
      const expectation = expect(p).rejects.toThrow("readiness timed out");

      // Advance time up to but not including the timeout — the
      // promise must still be pending.
      let resolved = false;
      p.then(() => (resolved = true), () => undefined);
      await vi.advanceTimersByTimeAsync(timeoutMs - 1);
      expect(resolved).toBe(false);

      // An unready image must reject, never be promoted as ready.
      await vi.advanceTimersByTimeAsync(2);
      await expectation;
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

  // ---------------------------------------------------------------------
  // non-strict mode: the slide being displayed, not a preload.
  //
  // The image HAS loaded; only decode() failed to settle. Rejecting here
  // showed "Couldn't load this photo" for a bitmap the browser was perfectly
  // willing to paint — strictly worse than showing the photo.
  // ---------------------------------------------------------------------

  it("non-strict: resolves when load succeeded but decode stays hung", async () => {
    class HungDecodeImage extends FakeImage {
      decode() { return new Promise<void>(() => {}); }
    }
    const p = decodeOffscreen("blob:hung", HungDecodeImage as unknown as typeof Image, {
      timeoutMs: 50,
      strict: false,
    });
    await vi.advanceTimersByTimeAsync(51);
    await expect(p).resolves.toBeUndefined();
  });

  it("non-strict: still rejects when the image never loads at all", async () => {
    // There is no bitmap to fall back to, so this must remain an error.
    FakeImage.behaviour = "error";
    const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image, { strict: false });
    const expectation = expect(p).rejects.toThrow("decode failed");
    await vi.runAllTimersAsync();
    await expectation;
  });

  it("non-strict: still rejects when the Image stays silent forever", async () => {
    FakeImage.behaviour = "silent";
    const p = decodeOffscreen("blob:fake", FakeImage as unknown as typeof Image, {
      timeoutMs: 50,
      strict: false,
    });
    const expectation = expect(p).rejects.toThrow("readiness timed out");
    await vi.advanceTimersByTimeAsync(51);
    await expectation;
  });
});
