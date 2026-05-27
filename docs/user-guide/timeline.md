# Timeline

Timeline is the main view — a chronological grid of every photo in your
library, grouped by day, with a sticky year header that follows the
scroll.

## Browsing

- **Scroll** in either direction. Thumbnails load as they enter the
  viewport, so even 250K-photo libraries scroll smoothly.
- **Sticky year header** at the top of the grid shows where you are.
- **Day separators** label each day's photos. Empty days are
  collapsed.
- **Hover a photo** to see its filename and camera info as a quick
  tooltip.
- **Photo stacks** collapse conservative burst and duplicate groups
  into one timeline tile. The tile shows the suggested best photo and
  a stacked badge with the number of photos in the group.

## Keyboard

| Key | Action |
|---|---|
| <kbd>↑</kbd> <kbd>↓</kbd> <kbd>←</kbd> <kbd>→</kbd> | Move highlight between thumbnails |
| <kbd>Enter</kbd> | Open the highlighted photo in the viewer |
| <kbd>Space</kbd> | Toggle selection on the highlighted photo |
| <kbd>Shift</kbd>+click | Range-select |
| <kbd>Ctrl</kbd>/<kbd>Cmd</kbd>+click | Add to selection |
| <kbd>PageUp</kbd> / <kbd>PageDown</kbd> | Scroll a viewport |
| <kbd>Home</kbd> / <kbd>End</kbd> | Jump to start / end |
| <kbd>/</kbd> | Focus the search bar |

## Selection actions

With one or more photos selected, the action bar at the bottom of the
window offers:

- **Add to album** — pick an existing album or create a new one.
- **Trash** — soft-delete; restorable from the Trash view.
- **Open as slideshow** — fullscreen with arrow-key navigation.

## Photo viewer

Click any photo to open the viewer. Within it:

| Key | Action |
|---|---|
| <kbd>←</kbd> / <kbd>→</kbd> | Previous / next photo |
| <kbd>Esc</kbd> | Back to gallery |
| <kbd>I</kbd> | Toggle the info panel (EXIF, location, people) |
| <kbd>+</kbd> / <kbd>−</kbd> | Zoom in / out |
| <kbd>0</kbd> | Fit to screen |
| <kbd>1</kbd> | Actual size (1:1 pixel) |
| <kbd>[</kbd> / <kbd>]</kbd> | Rotate left / right |
| <kbd>F</kbd> | Toggle fullscreen |

The info panel (toggled with <kbd>I</kbd>) shows EXIF metadata,
detected people, GPS location with a small map preview, and the file
path on disk.

If the current photo belongs to a stack, the toolbar shows a small
stack button. Opening it reveals the other photos in that stack without
changing normal timeline navigation: <kbd>←</kbd> and <kbd>→</kbd>
still move to the previous or next visible timeline photo. The stack
tray lets you browse members, mark a different best photo, remove a
member from the stack, unstack the group, or move all non-best members
to Trash.

## Thumbnail size

Three sizes, switched in **Settings → Appearance → Thumbnail size**:

- **Compact** — denser grid, more photos per row.
- **Default** — balanced.
- **Large** — fewer per row, more detail per thumbnail.

## See also

- [Search](search.md) — jump to specific dates, people, or places
- [Memories](memories.md) — the "N years ago today" banner
- [Keyboard shortcuts](keyboard-shortcuts.md) — full list
