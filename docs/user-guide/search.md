# Search

Smriti's search is one box that takes natural language — names,
places, dates, or combinations of them. There's no special syntax to
learn, though knowing what the parser recognises helps you write
better queries.

## How a query is parsed

Smriti parses your query into three slots:

| Slot | What it matches |
|---|---|
| **Person** | A named face cluster from the People view |
| **Location** | A city or country from your photos' metadata |
| **Date range** | A specific year, month, day, or relative period |

Any combination of these is valid: a query can have one filter or all
three.

## Examples

### Just a person

```
Dad
```

Returns all photos where the face cluster named "Dad" appears.
Matching is case-insensitive.

### Just a place

```
Paris
```

Returns all photos with `Paris` as the city or with `France` as the
country, derived from EXIF GPS at scan time.

### Just a date

```
2018
March 2019
March 15 2019
this week
last month
last year
yesterday
spring 2020
```

The date parser recognises:

- **ISO format**: `2024-03-15`
- **Years**: any 4-digit year (1900–2100)
- **Month + year**: `March 2019`, `Mar 2019`, `2019 March`
- **Month alone**: `March` (current year)
- **Seasons**: `spring`, `summer`, `autumn`/`fall`, `winter`
  (current year unless qualified)
- **Relative**: `today`, `yesterday`, `this week`, `last week`,
  `this month`, `last month`, `this year`, `last year`

### Person + place

```
Dad in Tokyo
Sarah in France
```

The literal word **"in"** separates the person from the place.
The parser treats the word before "in" as the person, and the
word(s) after as the location.

### Person + date

```
Dad 2019
Sarah last month
```

Smriti tries to recognise the trailing words as a date. Anything
that doesn't parse as a date is treated as a person name.

### Person + place + date

```
Dad in Tokyo 2019
Sarah in Paris March 2019
```

All three filters apply.

## How it handles ambiguity

When a single word could be a place or a person, Smriti uses a small
known-locations list (cities, countries) to decide. Words that match
that list become location filters; everything else is treated as a
person name. If you've named a face `Tokyo`, search for `Tokyo` will
match the place rather than the person — name your faces so they
don't collide with city names if you care.

## Recent searches

Recent queries are stored locally for convenience. They appear as
chips below the search bar when it's focused. Clear them via
**Settings → Clear search history**.

## Limitations

- **No full-text search** of filenames, EXIF comments, or tags
  beyond the parsed slots. Smriti optimises for the three slots
  above because that's what most photo queries actually want.
- **No regex or wildcards.** Names match literally.
- **Trashed photos are excluded** from all search results. Restore
  from the Trash view first if you need to find something there.

## Pivots from elsewhere

You don't always have to type. Several views deep-link into search:

- [Insights](insights.md) → click a heatmap cell, person, or location
- [Map](map.md) → click a cluster
- [People](people.md) → click a person card

## See also

- [Timeline](timeline.md) — full chronological view
- [People](people.md) — naming faces makes them searchable
- [Map](map.md) — geographic counterpart
