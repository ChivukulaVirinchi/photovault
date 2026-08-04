# Search

Smriti's search is one box for the library metadata it already has:
named people, dates, places, albums, favourites, media type, filenames,
camera names, and optional local visual meaning. It does not use cloud AI.

## What Search Understands

| Query | Meaning |
|---|---|
| `Goa 2023` | Photos from Goa taken in 2023 |
| `Dad 2024` | Photos containing Dad from 2024 |
| `Dad and Mom` | Photos containing both Dad and Mom |
| `only Dad` | Photos where every detected face is Dad |
| `only Dad and Mom` | Photos where every detected face is Dad or Mom, and both are present |
| `favourites Dad` | Favourite photos containing Dad |
| `videos Goa 2024` | Videos from Goa taken in 2024 |
| `album Goa Trip` | Photos in the matching album |
| `Nikon Goa` | Filename/camera/location fallback for unmatched words |
| `beach sunset` | Visually similar photos, when visual search is installed and indexed |

The interpreted filters appear as chips above the results so you can see
what Smriti understood.

## People

People search uses names from the People view. Matching is
case-insensitive and supports combinations:

```text
Dad
Dad and Mom
Dad with Mom
```

Multiple people are treated as an intersection: every returned photo must
contain every requested person.

## Strict `only`

`only` is intentionally strict.

```text
only Dad
only Dad and Mom
```

A photo matches `only Dad` only when:

- face processing has completed for the photo
- Dad appears in the photo
- no detected face is assigned to another person
- no detected face is unassigned or unknown

If a photo has an unrecognised face, it is excluded. This avoids showing
group photos when you asked for one person only.

## Dates

Search recognises:

- years: `2024`
- month + year: `March 2019`, `Mar 2019`, `2019 March`
- exact dates: `2024-03-15`
- relative dates: `today`, `yesterday`, `this week`, `last month`
- seasons: `spring 2020`, `fall 2021`

Date filters combine with every other filter.

## Places

Places are resolved from your library's stored city/country metadata.
There is no hardcoded city list in smart search. If GPS reverse-geocoding
has not been run, place search has less to work with.

## Albums, Favourites, And Media

Use `album <name>` or `in album <name>` to filter by album.

Use `favourites`, `favorites`, or `starred` to filter to favourite items.

Use `videos`, `video`, `photos`, or `photo` to filter media type.

## Visual Meaning

Visual search is optional. Click **Set up smart features** on Welcome;
Smriti installs the local model and runtime, then indexes automatically.
The model lives outside the app binary and per-library vectors stay under
`.photovault/semantic`.

When visual search is ready, natural queries such as:

```text
beach sunset
food photos 2024
videos mountains
```

are matched against local image/video-poster embeddings. Structured
filters still apply: `food photos 2024` is semantic `food` plus photo
media type plus the 2024 date range, not a separate loose search path.

## What It Does Not Do Yet

- No cloud lookup.
- No OCR dependency in the main search path.
- No regex or wildcard syntax.
- Trashed photos are excluded.

## Pivots From Elsewhere

Several views open Search with a pre-filled query:

- [Insights](insights.md) -> click a heatmap cell, person, or location
- [Map](map.md) -> click a cluster or place
- [People](people.md) -> click a person card

## See Also

- [Timeline](timeline.md) - full chronological view
- [People](people.md) - naming faces makes them searchable
- [Map](map.md) - geographic counterpart
