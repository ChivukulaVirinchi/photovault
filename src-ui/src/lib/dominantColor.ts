/// Extract a dominant color from an image URL.
///
/// Strategy: downsample the image to 16×16 onto an offscreen canvas, read
/// the pixels, and bucket them into an HSL histogram (8 hue × 4 sat ×
/// 4 val bins = 128 buckets). Pick the most populated non-extreme bucket
/// and return the average RGB of pixels that fell into it.
///
/// Naive averaging would mute saturation toward gray; the histogram
/// preserves the photo's recognisable mood. ~2 ms per call. Zero deps.
///
/// Used by the photo-detail "gallery wall" tint signature.

export type RGB = [number, number, number];

const CACHE = new Map<string, Promise<RGB>>();

export function extractDominantColor(url: string): Promise<RGB> {
  const cached = CACHE.get(url);
  if (cached) return cached;
  const p = doExtract(url).catch(() => [128, 128, 128] as RGB);
  CACHE.set(url, p);
  return p;
}

async function doExtract(url: string): Promise<RGB> {
  const img = new Image();
  // Tauri's asset protocol responses don't taint the canvas (CSP disabled
  // in tauri.conf.json), but setting this defensively keeps the code valid
  // even if CSP is tightened later.
  img.crossOrigin = "anonymous";
  img.src = url;
  await img.decode();

  const SIZE = 16;
  const canvas = document.createElement("canvas");
  canvas.width = SIZE;
  canvas.height = SIZE;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("no 2d context");
  ctx.drawImage(img, 0, 0, SIZE, SIZE);
  const { data } = ctx.getImageData(0, 0, SIZE, SIZE);

  const HUE_BINS = 8;
  const SAT_BINS = 4;
  const VAL_BINS = 4;
  type Bucket = { count: number; r: number; g: number; b: number };
  const bins: Bucket[] = Array.from({ length: HUE_BINS * SAT_BINS * VAL_BINS },
    () => ({ count: 0, r: 0, g: 0, b: 0 }));

  let fallbackR = 0, fallbackG = 0, fallbackB = 0, fallbackN = 0;

  for (let i = 0; i < data.length; i += 4) {
    const r = data[i], g = data[i + 1], b = data[i + 2], a = data[i + 3];
    if (a < 200) continue;
    fallbackR += r; fallbackG += g; fallbackB += b; fallbackN++;

    const [h, s, v] = rgbToHsv(r, g, b);
    // Skip blowouts and crushed shadows — they tend to be edges/backgrounds
    if (v < 0.10 || v > 0.96) continue;
    // Skip very desaturated pixels — we don't want gray to win
    if (s < 0.16) continue;

    const hi = Math.min(HUE_BINS - 1, Math.floor(h * HUE_BINS));
    const si = Math.min(SAT_BINS - 1, Math.floor(s * SAT_BINS));
    const vi = Math.min(VAL_BINS - 1, Math.floor(v * VAL_BINS));
    const idx = hi * SAT_BINS * VAL_BINS + si * VAL_BINS + vi;
    const bin = bins[idx];
    bin.count++;
    bin.r += r; bin.g += g; bin.b += b;
  }

  let best: Bucket | null = null;
  for (const bin of bins) {
    if (bin.count > 0 && (!best || bin.count > best.count)) best = bin;
  }

  if (best && best.count > 2) {
    return [
      Math.round(best.r / best.count),
      Math.round(best.g / best.count),
      Math.round(best.b / best.count),
    ];
  }

  // No saturated pixels — fall back to plain average so we still return a
  // sensible color for monochrome / heavily desaturated photos.
  if (fallbackN === 0) return [128, 128, 128];
  return [
    Math.round(fallbackR / fallbackN),
    Math.round(fallbackG / fallbackN),
    Math.round(fallbackB / fallbackN),
  ];
}

function rgbToHsv(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn), min = Math.min(rn, gn, bn);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === rn)      h = ((gn - bn) / d + (gn < bn ? 6 : 0)) / 6;
    else if (max === gn) h = ((bn - rn) / d + 2) / 6;
    else                 h = ((rn - gn) / d + 4) / 6;
  }
  const s = max === 0 ? 0 : d / max;
  const v = max;
  return [h, s, v];
}
