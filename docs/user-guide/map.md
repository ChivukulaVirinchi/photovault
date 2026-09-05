# Map View

The Map view plots every geotagged photo in your library on an
interactive world map. Pan, zoom, and click clusters to drill into
specific places.

## What you see

- **Clusters** — at zoomed-out levels, nearby photos are grouped into
  circular badges showing the photo count.
- **Pins** — individual photos at high zoom.
- **Filmstrip** — clicking a cluster or pin opens a side panel with
  the photos for that location and a scrollable filmstrip.

## Map tiles

Smriti uses [OpenStreetMap](https://www.openstreetmap.org/) tiles via
[MapLibre GL](https://maplibre.org/). Tiles are downloaded on demand
when you pan to a region for the first time, then cached locally on
in the app's WebView storage. Fresh cached tiles avoid network requests;
expired tiles are refreshed when online.

**Cache location:** the WebView Cache API, shared by Map and the photo-detail
minimap. Use Settings to manage it; it is not stored on the photo drive.

**Cache size cap:** configurable in **Settings → Map → Tile cache
size**. The default 500 MB is enough for the regions you regularly
visit; older inserted tiles are evicted first.

Both Map and photo location minimaps can request tiles while visible.

## Working offline

Once you've panned over a region in Map view, those tiles are
available offline until evicted or cleared. Useful before a trip
where you know you'll want to browse without connectivity.

To clear the cache (e.g. before donating the drive), use
**Settings → Map → Clear tile cache**.

## Photos without GPS

Photos that lack EXIF GPS tags don't appear on the map. You can
still find them in:

- The Timeline (everything's there)
- A search for the place name (if the place was filled in manually)

## Place names

When GPS is present, Smriti resolves it to a city + country via the
local **GeoNames** database (part of the asset pack). Place names
feed into [Search](search.md) and album-suggestion detection. If
your photos have GPS but no place names yet, run **Settings → Fill
in place names**.

The GeoNames data ships with the asset pack — it's offline.
No request to a geocoding API.

## See also

- [Search](search.md) — find photos by city or country
- [Albums](albums.md) — trip suggestions use geographic clusters
- [Privacy](../../PRIVACY.md) — map tile requests are the one
  always-on network call
