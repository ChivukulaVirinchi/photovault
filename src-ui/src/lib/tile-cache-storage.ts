// Map tiles belong to the WebView cache, not a second Rust cache directory.
export const TILE_CACHE_NAME = "smriti-tiles-v1";
let limitBytes = 500 * 1024 * 1024;
let generation = 0;
let mutations: Promise<unknown> = Promise.resolve();
let inventory: Map<string, number> | null = null;
let totalBytes = 0;

function mutate<T>(operation: () => Promise<T>): Promise<T> {
  const result = mutations.then(operation);
  mutations = result.catch(() => {});
  return result;
}

export async function openTileCache(): Promise<Cache | null> {
  try { return await caches.open(TILE_CACHE_NAME); } catch { return null; }
}

async function entries(cache: Cache): Promise<Map<string, number>> {
  if (inventory) return inventory;
  const found = new Map<string, number>();
  for (const request of await cache.keys()) {
    const response = await cache.match(request);
    if (!response) continue;
    const recorded = Number(response.headers.get("x-smriti-bytes"));
    found.set(request.url, recorded > 0 ? recorded : (await response.arrayBuffer()).byteLength);
  }
  inventory = found;
  totalBytes = [...found.values()].reduce((sum, size) => sum + size, 0);
  return found;
}

async function trim(cache: Cache, sizes: Map<string, number>) {
  for (const [url, size] of sizes) {
    if (totalBytes <= limitBytes) break;
    await cache.delete(url);
    sizes.delete(url);
    totalBytes -= size;
  }
}

export const tileCacheGeneration = () => generation;

export function storeTile(url: string, response: Response, bytes: number, expectedGeneration: number) {
  return mutate(async () => {
    if (expectedGeneration !== generation || bytes > limitBytes) return;
    const cache = await openTileCache();
    if (!cache) return;
    const sizes = await entries(cache);
    await cache.put(url, response);
    totalBytes += bytes - (sizes.get(url) ?? 0);
    sizes.delete(url);
    sizes.set(url, bytes);
    await trim(cache, sizes);
  });
}

export function setTileCacheLimit(limitMb: number) {
  limitBytes = Math.max(50, Math.min(10_000, Number.isFinite(limitMb) ? limitMb : 500)) * 1024 * 1024;
  return mutate(async () => {
    const cache = await openTileCache();
    if (cache) await trim(cache, await entries(cache));
  });
}

export function tileCacheStats() {
  return mutate(async () => {
    const cache = await openTileCache();
    const sizes = cache ? await entries(cache) : new Map<string, number>();
    return { size_bytes: cache ? totalBytes : 0, file_count: sizes.size, limit_bytes: limitBytes };
  });
}

export function clearTileCache() {
  generation += 1;
  return mutate(async () => {
    await caches.delete(TILE_CACHE_NAME);
    inventory = null;
    totalBytes = 0;
  });
}
