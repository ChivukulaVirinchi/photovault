import maplibregl from "maplibre-gl";
import { settings } from "./api/all";
import { openTileCache, setTileCacheLimit, storeTile, tileCacheGeneration } from "./tile-cache-storage";

const MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;
let installed = false;

export async function loadTile(url: string, signal: AbortSignal): Promise<ArrayBuffer> {
  signal.throwIfAborted();
  const generation = tileCacheGeneration();
  const cache = await openTileCache();
  const hit = await cache?.match(url).catch(() => undefined);
  const savedAt = Number(hit?.headers.get("x-pv-cached-at"));
  if (hit && savedAt > 0 && Date.now() - savedAt < MAX_AGE_MS) return hit.arrayBuffer();
  try {
    const response = await fetch(url, { signal, cache: "default" });
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
    // Previously visited areas remain usable offline, even after their TTL.
    if (hit) return hit.arrayBuffer();
    throw error;
  }
}

export function installTileCache() {
  if (installed) return;
  installed = true;
  void settings.get().then(config => setTileCacheLimit(config.map_cache_limit_mb)).catch(() => {});
  maplibregl.addProtocol("cached", async (params, controller) => ({
    data: await loadTile(params.url.replace(/^cached:\/\//, ""), controller.signal),
  }));
}
