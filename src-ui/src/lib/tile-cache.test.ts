import { beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("maplibre-gl", () => ({ addProtocol: vi.fn(), default: { addProtocol: vi.fn() } }));
vi.mock("./api/all", () => ({ settings: { get: vi.fn() } }));
import { ensureTileCache, loadTile } from "./tile-cache";
import { settings } from "./api/all";
import { clearTileCache, setTileCacheLimit, storeTile, tileCacheGeneration, tileCacheStats } from "./tile-cache-storage";

const data = new Map<string, Response>();
const key = (request: Request | string) => typeof request === "string" ? request : request.url;
const cache = {
  match: async (request: Request | string) => data.get(key(request))?.clone(),
  put: async (request: Request | string, response: Response) => { data.set(key(request), response.clone()); },
  delete: async (request: Request | string) => data.delete(key(request)),
  keys: async () => [...data.keys()].map(url => new Request(url)),
};
const url = "https://a.tile.openstreetmap.org/1/0/0.png";

beforeEach(async () => {
  vi.stubGlobal("caches", { open: async () => cache, delete: async () => data.clear() });
  vi.stubGlobal("fetch", vi.fn());
  await clearTileCache();
  await setTileCacheLimit(50);
});

describe("map tile cache", () => {
  it("does not hold map construction behind cache-budget maintenance", async () => {
    vi.mocked(settings.get).mockReturnValue(new Promise(() => {}));
    await ensureTileCache();
    const maplibre = await import("maplibre-gl");
    expect(maplibre.addProtocol).toHaveBeenCalledWith("cached", expect.any(Function));
  });

  it("uses cached tiles without a request regardless of age", async () => {
    data.set(url, new Response("cached", { headers: { "x-pv-cached-at": String(Date.now()) } }));
    expect(new TextDecoder().decode(await loadTile(url, new AbortController().signal))).toBe("cached");
    expect(fetch).not.toHaveBeenCalled();
    data.set(url, new Response("offline", { headers: { "x-pv-cached-at": "1" } }));
    expect(new TextDecoder().decode(await loadTile(url, new AbortController().signal))).toBe("offline");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("honours cancellation before reading a cached tile", async () => {
    data.set(url, new Response("old"));
    const controller = new AbortController();
    controller.abort();
    await expect(loadTile(url, controller.signal)).rejects.toMatchObject({ name: "AbortError" });
  });

  it("fetches normally when persistent storage is unavailable", async () => {
    vi.stubGlobal("caches", { open: async () => { throw new Error("unavailable"); } });
    vi.mocked(fetch).mockResolvedValue(new Response("network"));
    expect(new TextDecoder().decode(await loadTile(url, new AbortController().signal))).toBe("network");
    await tileCacheStats();
  });

  it("enforces the byte limit and reports the same cache", async () => {
    const size = 30 * 1024 * 1024;
    await storeTile(url, new Response("a"), size, tileCacheGeneration());
    await storeTile(`${url}?second`, new Response("b"), size, tileCacheGeneration());
    expect(data.has(url)).toBe(false);
    expect(await tileCacheStats()).toEqual({ size_bytes: size, file_count: 1, limit_bytes: 50 * 1024 * 1024 });
  });

  it("does not repopulate a cleared cache from an older request", async () => {
    const generation = tileCacheGeneration();
    await clearTileCache();
    await storeTile(url, new Response("late"), 4, generation);
    expect((await tileCacheStats()).file_count).toBe(0);
  });
});
