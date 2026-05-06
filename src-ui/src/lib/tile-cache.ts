/// Persistent tile cache for MapLibre, backed by the browser Cache API.
/// Survives app restarts. Uses a custom MapLibre protocol so we can do
/// async cache lookups synchronously from MapLibre's perspective.
///
/// Usage: call `installTileCache()` once before any MapLibre map is created.
/// Then in your map config, transform OSM URLs through the `cached:` scheme:
///   tiles: ["cached://https://a.tile.openstreetmap.org/{z}/{x}/{y}.png", ...]

import maplibregl from "maplibre-gl";

const CACHE_NAME = "photovault-tiles-v1";
const SCHEME = "cached://";

/// Stale-after duration: 30 days. OSM is allowed to be stale for a while;
/// we just want to avoid hammering their servers.
const MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

let installed = false;

export function installTileCache() {
  if (installed) return;
  installed = true;

  maplibregl.addProtocol("cached", async (params, _abortController) => {
    // Strip the scheme prefix that MapLibre re-prepends.
    const realUrl = params.url.replace(/^cached:\/\//, "");

    try {
      const cache = await caches.open(CACHE_NAME);

      // Cache hit?
      const hit = await cache.match(realUrl);
      if (hit) {
        const dateHeader = hit.headers.get("x-pv-cached-at");
        const fresh =
          dateHeader && Date.now() - Number(dateHeader) < MAX_AGE_MS;
        if (fresh) {
          const buf = await hit.arrayBuffer();
          return { data: buf };
        }
      }

      // Miss or stale. Fetch fresh.
      const res = await fetch(realUrl, {
        // Tile servers respond with public, cacheable headers — letting
        // the browser HTTP cache also help is fine.
        cache: "default",
      });
      if (!res.ok) {
        throw new Error(`tile ${realUrl} returned ${res.status}`);
      }

      const buf = await res.arrayBuffer();

      // Store in our persistent cache. Wrap in a Response so we can
      // attach our own freshness header.
      const headers = new Headers(res.headers);
      headers.set("x-pv-cached-at", String(Date.now()));
      // Cache.put requires a re-instantiated body.
      const stored = new Response(buf.slice(0), {
        status: 200,
        headers,
      });
      // Fire-and-forget; tile rendering doesn't wait on this.
      cache.put(realUrl, stored).catch(() => {
        // Quota errors etc. — non-fatal.
      });

      return { data: buf };
    } catch (e) {
      // Bubble up; MapLibre will display a missing-tile placeholder.
      throw e;
    }
  });
}
