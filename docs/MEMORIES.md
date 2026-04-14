# Memories

PhotoVault's "N years ago today" rediscovery surface. Memories appear
automatically on every launch and refresh themselves once a day.

## What you'll see

### Four kinds of memory cards

**On This Day** — photos taken on the same calendar date in past years.
The most common type. One card per prior year that has at least one
photo on this date. Title format: *"3 years ago today"*, *"7 years ago
today"*. Capped at the 10 most-recent prior years.

**This Week** — fallback that only appears when On This Day is empty.
Photos taken within ±3 days of today's date in past years. Useful so
quiet anniversaries don't leave the banner empty. Title: *"2 years ago
this week"*.

**Seasonal Recap** — full-month rollup for past years that have ≥5
photos in the same month as today. Title: *"August 2020"*. Capped at
the 5 most-recent prior years.

**Year Recap** — ultimate fallback. Only fires when none of the three
generators above produced anything (sparse libraries where today's
calendar date doesn't happen to overlap with any photos). Surfaces up
to 5 cards, one per prior year that has any photos at all. Title:
*"3 years ago"*. Each card holds up to 50 photos from that year.

This guarantees that any library with at least six months of history
will always show *something*, even if the calendar luck didn't land
on today's date.

### Where memories appear

- **Timeline banner** — horizontal carousel above the photo grid,
  shows the top 5 ranked memory cards. Scroll horizontally to see them
  all. Click a card to open its filmstrip.
- **Memories sidebar entry** — full list of every active memory for
  today as wide rows. Same click behavior.

### What you can do with a memory

- **Click** to open the slideshow: one photo at a time, big and
  centered, auto-advancing every 4 seconds. Header shows the memory
  title and a `current / total` counter.
- **Slideshow controls**:
  - ◀ and ▶ buttons (or `←` / `→` arrow keys) to step manually.
  - Pause / Play button (or `Space` key) to stop / resume auto-advance.
  - Back button (or `Esc`) to return to the previous view.
  - Auto-advance loops back to the first photo at the end.
- **Hide a person from Memories**: open a person's detail view (People
  → click the person), use the "Hide from Memories" button in the
  header. The block is durable — that person is removed from current
  memory cards instantly and never appears in future memory cards.
- **Disable Memories entirely**: Settings → Memories → toggle off. The
  banner disappears, sidebar entry shows an empty state, the 60-second
  background tick stops running. Toggling back on triggers a fresh
  generation pass.

## How memories rank

When multiple memories qualify on the same day, the banner shows them
in a deliberate order. The score that drives this is:

```
log2(photo_count + 1) × √(years_ago + 1) × face_bonus × kind_bonus
```

In plain terms:

- Bigger collections beat smaller ones, but with diminishing returns.
- Older anniversaries beat newer ones, also with diminishing returns
  (so a 2-year memory with 50 photos beats a 10-year memory with 1
  photo).
- Memories that contain detected faces get a 1.3× boost.
- Seasonal Recaps get a 1.5× boost (they're rarer; one per month, not
  per day).

The banner shows the top 5 by score; the sidebar Memories view shows
all of them (capped at 20 per day).

## Hero photo selection

Each memory has a single hero image shown on its card. It's chosen
once at generation time based on:

- Landscape orientation preferred (better for the 16:9 banner aspect)
- Photos with detected faces preferred over no-face photos
- Photos with multiple faces get a small extra bonus
- Tie-broken toward smaller photo IDs for stable ordering

If a thumbnail isn't generated yet for the hero, the card renders with
a neutral background until thumbnails catch up.

## When memories are silent

Memories deliberately stay quiet in three cases — the banner and
sidebar are hidden / show empty state without explanation:

1. **Library too new** — your oldest non-trashed photo is less than
   six months old. Memories require some history to be meaningful.
2. **No qualifying memories today** — no photos exist on today's date
   in past years AND none in the ±3 day fallback window AND no past
   month with ≥10 photos. Rare but possible.
3. **Memories disabled** — you turned it off in Settings.

There's no "come back in N days" message, no nag — the surface is
simply absent.

## Refresh behavior

- **On launch**: regenerate immediately when a drive is selected and
  the database is ready.
- **On day rollover**: a 60-second background tick compares today's
  `NaiveDate` against the last regeneration date. When they differ
  (i.e., midnight has passed), regeneration runs in the background.
  The banner updates without restart.
- **On block change**: when you hide a person, blocked cards
  disappear from the in-memory list immediately. The persistent block
  is enforced again on the next regeneration so it survives restart.
- **On toggle on**: re-enabling Memories triggers a fresh generation
  pass.

## Performance

Memories are computed, not stored. There's no `memories` table — all
three generators run live SQL queries on the existing `photos` table
on every refresh. On a 100k-photo library this completes well under
50 ms; the work is bounded by the indexed `date_taken` column lookups.

The only persisted state is the small `memory_blocks` table holding
your "hide person from memories" decisions.

## What's intentionally not in v1

- **Per-card dismiss button** — daily rotation handles "I've seen
  this one" naturally, and the person-block handles the real
  emotional-safety case (ex / grief). Adding per-card dismiss adds
  state complexity without enough payoff.
- **Slideshow / music / transitions** — Google Photos does this; it's
  a separate, much bigger feature.
- **Auto-generated captions** — "You and Alice at the beach" requires
  scene ML + person linking + captioning. Out of scope.
- **Notifications** — PhotoVault is offline-only, no notification
  surface.
- **Trip memory cards** — pending the Auto Albums feature; once trips
  are detected, "Your trip to Tokyo, 2 years ago" becomes possible.
- **Person-of-the-year cards** — "Best of Alice in 2024" pending more
  user data (birthdays, year-end triggers).
- **Then-and-now side-by-side** — same person across years; needs
  pose / composition matching to look good.
- **Date-range hide** — schema supports it (`memory_blocks.kind =
  'date_range'`), no UI yet. Would be useful for hiding anniversaries
  of bad events.

## Settings reference

| Setting | Default | Effect |
|---------|---------|--------|
| Memories (toggle) | On | Master switch. Off hides everything. |

## Data model reference (for the curious)

Single new table:

```sql
memory_blocks (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,          -- 'person' (v1)
    target_key TEXT NOT NULL,    -- cluster_id as string
    created_at DATETIME,
    UNIQUE(kind, target_key)
);
```

That's it. Memories themselves are recomputed from `photos`, `faces`,
and `face_clusters` on every refresh — there's no second source of
truth to drift, no migrations to break, no caches to invalidate.
