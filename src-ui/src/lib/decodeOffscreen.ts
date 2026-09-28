/**
 * Off-screen image decode helper for the slideshow.
 *
 * One deadline covers both loading and decoding, because some WebViews load
 * an image but never settle `decode()` on certain ICC-tagged JPEGs.
 *
 * What a timeout should mean depends on the caller, so it is explicit:
 *
 *   * `strict: true` (preloads) — a timeout is a failure. Nothing is on
 *     screen yet, so the caller can simply try again later.
 *   * `strict: false` (the slide being shown) — a timeout resolves. The image
 *     has *loaded*; only `decode()` failed to settle. Rejecting here turned a
 *     perfectly displayable photo into a permanent "Couldn't load this photo"
 *     card, which is strictly worse than showing a bitmap the browser is
 *     willing to paint. Only a genuine load error still rejects.
 *
 * Every completion path removes handlers and ignores late callbacks.
 */
export interface DecodeOffscreenOptions {
  timeoutMs?: number;
  /// See above. Defaults to true (a timeout is an error).
  strict?: boolean;
}

export function decodeOffscreen(
  url: string,
  // Allow tests to inject a fake Image constructor. Defaults to the
  // window-global one in production.
  ImageCtor: { new (): HTMLImageElement } = Image,
  options: DecodeOffscreenOptions = {},
): Promise<void> {
  const { timeoutMs = 8000, strict = true } = options;
  return new Promise<void>((resolve, reject) => {
    const img = new ImageCtor();
    let settled = false;
    /// `loaded` distinguishes "the bitmap is available" from "we gave up".
    let loaded = false;
    const finish = (error?: unknown) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      img.onload = null;
      img.onerror = null;
      if (error) reject(error);
      else resolve();
    };
    const timer = setTimeout(() => {
      if (!strict && loaded) {
        // Decode is taking longer than we are willing to wait, but the image
        // is loaded. Let the caller show it.
        finish();
        return;
      }
      finish(new Error("image readiness timed out"));
    }, timeoutMs);
    img.onload = () => {
      loaded = true;
      const decode = (img as HTMLImageElement & { decode?: () => Promise<void> }).decode;
      if (typeof decode === "function") {
        void Promise.resolve().then(() => decode.call(img)).then(() => finish(), (error) => {
          // A decode rejection for an image the browser already loaded is the
          // documented WebView2 ICC case; non-strict callers should still show
          // it rather than surfacing an error.
          if (!strict && loaded) finish();
          else finish(error);
        });
      } else if (typeof img.naturalWidth !== "number" || (img.naturalWidth > 0 && img.naturalHeight > 0)) {
        const paint = typeof requestAnimationFrame === "function"
          ? requestAnimationFrame
          : (cb: FrameRequestCallback) => setTimeout(cb, 0) as unknown as number;
        paint(() => finish());
      } else {
        finish(new Error("image loaded without dimensions"));
      }
    };
    img.onerror = () => {
      // A real load failure always fails, strict or not — there is no bitmap.
      finish(new Error("decode failed"));
    };
    img.src = url;
  });
}
