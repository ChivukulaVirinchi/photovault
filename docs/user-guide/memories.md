# Memories

Memories surfaces "*this day, N years ago*" style cards when you open
Smriti. They're generated locally, on demand, when you visit the
Memories view (or read the banner on the Timeline). Nothing is
pre-rendered, no server, no schedule.

## How memories are generated

Smriti runs four generators against today's date, then ranks the
results and picks the best ~20 to surface:

- **On this day** — photos from the exact same calendar day in any
  previous year.
- **Fallback window** — when "on this day" finds nothing, widens to a
  ±7-day window so a sparse library still sees something.
- **Seasonal recap** — gathers a whole month or season from a past
  year if there's enough density.
- **Year recap** — ultimate fallback: at least one card from a year
  with any history at all.

A hero photo is chosen for each memory: landscape orientation
preferred, photos with faces ranked higher, recency breaks ties.

## When memories appear

- **Library age** — Memories only surface once your oldest photo is
  at least three months old. New libraries see nothing for the first
  quarter.
- **Photo age** — Photos newer than three months don't appear in
  memories. The point is nostalgia, not yesterday.
- **Daily refresh** — Each day surfaces a new set; yesterday's cards
  are recomputed.

## What you can do

- **Open a memory card** to see all the photos behind it as a
  filmstrip.
- **Slideshow** — autoplay through the photos with arrow-key
  navigation. Pause with <kbd>Space</kbd>.
- **Dismiss a memory** if you don't want to see it again — useful for
  trips or events you'd rather forget. Dismissed memories don't
  resurface in future months.

## Privacy

Memories are computed entirely on-device. No server-side generation,
no curated highlight reel, no notification that you "should look at
this." The Memories view is something you visit on purpose.

## Disabling memories

**Settings → Memories → Enable memories** toggles the feature
entirely. When off, the Timeline banner and the Memories view both
hide.

## See also

- [Timeline](timeline.md) — the daily Memories banner lives here
- [People](people.md) — face data feeds the memory hero-photo picker
- [Settings → Memories](settings.md)
