/**
 * Off-screen image decode helper for the slideshow.
 *
 * We deliberately do NOT call `img.decode()` here — in some Tauri /
 * WebView2 builds it hangs indefinitely when the source JPEG carries
 * certain ICC color profile blocks (Unsplash exports trip it). The
 * `onload` / `onerror` callbacks are reliable across every webview we
 * support, and the visible `<img decoding="async">` handles the
 * paint-time decode anyway.
 *
 * The timeout is a fail-safe: if neither callback fires within
 * `timeoutMs`, we resolve rather than hang. The slideshow can briefly
 * flash on the next slide; that's strictly better than a stuck
 * crossfade where every button-driven re-trigger gets shadowed by
 * the same hung await.
 */
export function decodeOffscreen(
  url: string,
  // Allow tests to inject a fake Image constructor. Defaults to the
  // window-global one in production.
  ImageCtor: { new (): HTMLImageElement } = Image,
  timeoutMs = 8000,
): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const img = new ImageCtor();
    const timer = setTimeout(() => {
      resolve();
    }, timeoutMs);
    img.onload = () => {
      clearTimeout(timer);
      resolve();
    };
    img.onerror = () => {
      clearTimeout(timer);
      reject(new Error("decode failed"));
    };
    img.src = url;
  });
}
