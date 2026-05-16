# Albums

Albums let you group photos into named collections. Smriti supports
both **manual** albums (you create them) and **suggested** albums
(Smriti detects trips and events automatically and offers them for
your review).

## Manual albums

Create an album from the Albums view, or directly from a Timeline
selection:

1. Select one or more photos in any view.
2. Click **Add to album** in the action bar.
3. Pick an existing album or type a name to create a new one.

Albums support:

- **Inline rename** — click the title in the album header.
- **Cover photo** — right-click any photo in the album → **Set as
  cover**, or let Smriti pick automatically (landscape with faces
  scores higher).
- **Reordering** — drag photos within an album to change the order.
- **Bulk remove** — select photos, **Remove from album** in the
  action bar.

Removing a photo from an album doesn't delete the photo — it stays in
the Timeline.

## Suggested albums

Smriti analyses your library for two kinds of clusters and proposes
them as drafts you can accept, ignore, or dismiss:

- **Trips** — runs of photos taken away from your home city for at
  least a day. Photos must clear a few gates (duration, photo count,
  distance from home, rarity) before they're suggested. Detected
  trips are titled something like *"Trip to Tokyo · March 2019"*.
- **Events** — bursts of activity within your home area: a birthday,
  a wedding, a concert. Detected events lean on photo density,
  duration, and face overlap with people you've already named.

Open the Albums view to see pending suggestions. Each card shows:

- A cover photo,
- The detected title,
- A photo count,
- **Accept** / **Dismiss** buttons.

**Accept** promotes the suggestion to a real album that you can edit
like any manual one. **Dismiss** marks it as not-an-album so Smriti
doesn't re-suggest it the next time it scans.

## Settings that affect suggestions

- **Home city override** in Settings tells trip detection where you
  live. By default Smriti infers home from the city where most of
  your photos are taken. Override when you've moved or the inference
  is wrong.
- **Geocoding** — trip detection needs your photos to have place
  names. If GPS data is present but place names are missing, run
  **Settings → Fill in place names**.

## Album cover photo selection

When you don't manually set a cover, Smriti picks one with a small
heuristic: prefer landscape orientation, prefer photos with faces,
break ties with recency. You can always override.

## See also

- [Memories](memories.md) — related "on this day" surfacing
- [Map](map.md) — pivot from a place to all photos there
- [Settings → Map](settings.md) — home-city override
