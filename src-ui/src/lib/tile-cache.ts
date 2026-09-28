import { settings } from "./api/all";
import { openTileCache, setTileCacheLimit, storeTile, tileCacheGeneration } from "./tile-cache-storage";

/**
 * MapLibre is loaded on demand, never at module scope.
 *
 * This file used to `import * as maplibregl from "maplibre-gl"` at the top
 * level, and both Map.svelte and PhotoDetail.svelte reach it from App.svelte,
 * which is reached from the entry point. That one static import pulled the
 * entire ~900 KB MapLibre library into the initial bundle, so every cold
 * start paid to parse and evaluate the map renderer before it could paint
 * anything — including users who never open the Map.
 *
 * Keeping the import dynamic here means MapLibre lands in the route chunks
 * that actually create a map.
 */

type MapLibre = typeof import("maplibre-gl");

let mapLibrePromise: Promise<MapLibre> | null = null;

/** Load MapLibre exactly once; every caller awaits the same promise. */
export function loadMapLibre(): Promise<MapLibre> {
  mapLibrePromise ??= import("maplibre-gl");
  return mapLibrePromise;
}

let installPromise: Promise<void> | null = null;

/**
 * Register the `cached://` protocol and apply the configured tile-cache
 * limit. Idempotent and memoized, so it is safe to call from every route
 * that builds a map.
 *
 * Callers MUST await this before constructing a `maplibregl.Map`, because
 * the map starts requesting tiles immediately: an unregistered `cached`
 * scheme fails every one of those requests, which shows up as a permanently
 * blank map rather than a slow one.
 */
export function ensureTileCache(): Promise<void> {
  installPromise ??= (async () => {
    const maplibregl = await loadMapLibre();
    maplibregl.addProtocol("cached", async (params, controller) => ({
      data: await loadTile(params.url.replace(/^cached:\/\//, ""), controller.signal),
    }));
    // Cache sizing is maintenance, not a prerequisite for the first map
    // frame. Register the protocol synchronously, then configure the budget
    // without holding map construction behind settings IPC + IndexedDB.
    void settings.get()
      .then((config) => setTileCacheLimit(config.map_cache_limit_mb))
      .catch(() => {});
  })().catch((error) => {
    // Allow a later attempt to retry rather than caching a rejection forever.
    installPromise = null;
    throw error;
  });
  return installPromise;
}

export async function loadTile(url: string, signal: AbortSignal): Promise<ArrayBuffer> {
  signal.throwIfAborted();
  const generation = tileCacheGeneration();
  const cache = await openTileCache();
  const hit = await cache?.match(url).catch(() => undefined);
  // A map tile does not become unusable with age. Keep the bounded local
  // cache authoritative so reopening Map paints from disk and never blanks
  // while an unnecessary refresh races the renderer. The configured cache
  // budget still evicts least-recently-used tiles when space is needed.
  if (hit) return hit.arrayBuffer();
  const controller = new AbortController();
  const abort = () => controller.abort(signal.reason);
  signal.addEventListener("abort", abort, { once: true });
  if (signal.aborted) abort();
  const deadline = setTimeout(() => controller.abort(new Error("Tile request timed out")), 8_000);
  try {
    const response = await fetch(url, { signal: controller.signal, cache: "default" });
    if (!response.ok) throw new Error(`Tile request returned ${response.status}`);
    const data = await response.arrayBuffer();
    signal.throwIfAborted();
    const headers = new Headers(response.headers);
    headers.delete("content-encoding");
    headers.set("x-pv-cached-at", String(Date.now()));
    headers.set("x-smriti-bytes", String(data.byteLength));
    void storeTile(url, new Response(data.slice(0), { headers }), data.byteLength, generation).catch(() => {});
    return data;
  } catch (error) {
    signal.throwIfAborted();
    throw error;
  } finally {
    clearTimeout(deadline);
    signal.removeEventListener("abort", abort);
  }
}
