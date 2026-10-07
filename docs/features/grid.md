# The grid that can be found (implemented 2026-10-06)

Status: built in the pull request that closed the first of the person's notes
of October 6, 2026, decision D86. [The piano roll](piano-roll.md) named the
piano roll's grid and the pattern's step as note values; this is the grid of
the whole app: where it is seen, how the timeline's is chosen, and how a note
goes off it.

## What

- **The grid in the transport bar.** After the tempo, meter and length, the
  timeline's grid as a note value: `Grid 1/16`. Gray while it follows the
  zoom, as it always did; white once chosen. It is a menu: Follow Zoom, the
  sizes 1 Bar, 1/2, 1/4, 1/8, 1/16, 1/32 and 1/64, Finer, Coarser, Triplets
  and Snap to Grid.
- **View › Grid**, the same menu in the menu bar, with Ableton's keys: ⌘1
  finer, ⌘2 coarser, ⌘3 triplets on or off, ⌘4 snap on or off. It acts on
  the editor that has the keys: the piano roll's grid, the pattern's step
  (an edit of the song, as the Step menu makes), or else the timeline's.
  Follow Zoom is the timeline's alone; a size the pattern cannot have, a bar
  or a half note, is gray while the pattern has the keys.
- **A chosen grid holds whatever the zoom.** Clicks, drags, double-clicks,
  the loop brace, automation points, dropped files and the arrow keys all
  use it. Its lines are drawn once they have room, every second or fourth
  line where they do not, as long as those fall on the bars; a triplet grid
  zoomed far out draws the zoom's lines and still snaps to its own. Each beat
  is drawn a shade brighter than the lines between.
- **Triplets.** A size with Triplets on is its triplet: 1/8 is 1/8T, a third
  of a beat. Choosing a size keeps Triplets as it is. The piano roll's Grid
  menu and the Step menu list 1/2T and 1/4T too.
- **Snap to Grid off** places everything where the pointer is, to a
  thousandth of a beat, as ⌘ does with it on; ⌘ then snaps instead. The
  transport bar says `no snap`. It is on when a project opens.
- **Where ⌘ is said.** The hint under an empty pattern says that ⌘ puts an
  event off the grid, a little behind the beat, and that Step is the grid;
  the Grid and Step menus' help names ⌘1 to ⌘3.

## How

- `Grid` (`apps/mac/Sources/AAWApp/Grid.swift`) is the one list of values
  in beats as the song writes them, their names, and the steps finer,
  coarser and triplets take through it, tested in `GridTests`.
- `TimelineLayout.fixedGrid` is the chosen grid; `grid` is it or `zoomGrid`,
  and `drawnGrid` is what has room. `SongModel.timelineGrid` and
  `snapsToGrid` hold the choice for the window, and `free(_:)` reads ⌘
  against Snap for every editor. The arrangement reports `zoomGrid` for the
  transport bar.
- `SongWindowController` finds the editor for a Grid command by the window's
  first responder, as Copy and Paste do.

## Limits

- The choice is the window's and is not saved with the song.
- The list's `1 Bar` is the song's bar, from its [time signature](time-signature.md); a grid chosen as a bar keeps its beats when the meter changes.
- 1/64T is not listed.
