# A track's height dragged — implemented November 23, 2026

Backlog: Next 1, the last of the person's notes of November 22, 2026 (D194):
"to be able to adjust the size of a track ... when you go to the lower
boundary it should show an icon that then allows you to click and drag down
to make the track bigger or smaller in display, that would allow making it
appear bigger when editing or smaller collapsed if needed."

## What

The pointer over the lower edge of a track's, a group's or a return's
header shows the up-and-down resize cursor; dragging it makes the row and
its lane taller or shorter, from 22 points to 400, as Ableton's and Logic's
do. Shorter than two lines (42 points), a track's or a group's header is one
line: its name and chain, arm, mute and solo, without the volume bar and
pan. Taller, its clips and their notes and waveforms are drawn larger.
Option-drag sets every track, group and return to the height dragged to; a
double-click on the edge puts the row back to its default, every row with
Option. Escape during a drag puts the heights back. The master keeps its
height.

## Where it is kept

A view setting, not a song field: `.daw/rows.json` in the project's folder,
beside `threads.json`, holds each row's height in whole points by its path in
the song, `tracks.drums`, `groups.keys`, `returns.verb`. A row not in it has
its kind's default, 46 points for a track or a group and 32 for a return.
`song.yaml` and the agent's text of the song do not change when the person
resizes. The file is written when a drag ends and read when the project
opens; Save As writes it into the new folder.

## The app

- `RowHeights` reads and writes the file, keeps a height in range, and
  follows each revision's rows by their keys, so a row renamed by the
  person or by the agent while the app has the project open keeps its
  height under the new path; two rows that swap names swap heights.
- `HeaderLayout` takes the row's own `line` height in place of the fixed
  one: the sends and lanes start under it, `showsLevels` is false under
  `twoLines`, a return's one line moves up to stay inside a short row, and
  `edge` is the strip across the header's lower edge, under its sends and
  above its lanes, reaching two points into the row under it.
- `ArrangementView` keeps each row's and each clip's `line` from the
  heights, draws clips, a group's overview and hit tests at it, and adds
  the edges as `.resizeUpDown` cursor rects. A press on an edge comes
  before anything else in the header. The drag redraws at once, without
  easing, and writes the file when it ends.

## Decisions

- **In the project, not in the song.** The design asked for a view
  setting; the song stays the agent's text, and an agent's diff never shows
  a resize. Kept by path rather than by the app's keys, which change on
  every open.
- **Not undoable.** A height is not an edit of the song, as in Ableton and
  Logic.
- **One line below 42 points, not a separate collapse.** The volume line
  ends 40 points down; any shorter height cuts it, so the row drops to its
  first line instead. A collapse toggle was left out: the drag down to 22
  is the collapse.
- **Option sets every row to the same height,** returns included, as
  Ableton's Option-drag does; the returns' smaller default is lost until a
  double-click with Option puts every row back.

## Limits

- A row renamed while the app does not have the project open, by `daw`
  headless or an edit of the file, loses its height: the rename is seen
  only as a revision the app follows.
- An automation lane keeps its fixed height.
