# A track's height dragged — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 10. The person's note of November
22, 2026 (D194): "to be able to adjust the size of a track ... when you go to
the lower boundary it should show an icon that then allows you to click and
drag down to make the track bigger or smaller in display, that would allow
making it appear bigger when editing or smaller collapsed if needed."

## What

The pointer over the lower edge of a track's header, a group's or a
return's, shows the up-and-down resize cursor; dragging it makes the row
and its lane taller or shorter, as Ableton's and Logic's do. Shorter than
the header's controls, the row collapses to one line with its name, mute and
solo; taller, a clip's waveform and notes are drawn larger. Option-drag
resizes every row together. A double-click on the edge returns the row to
its default height. Each row's height is kept with the project.

## Why

Every track's lane is `TimelineLayout.trackHeight`, 46 points, plus its
sends; a part being edited cannot be made larger, and a song of many
tracks cannot be made to fit.

## Design

- **Where it is kept.** A view setting, not a song field: kept by the app for
  the project outside `song.yaml`, by row ID, so the
  agent's text of the song does not change when the person resizes.
- **Layout.** `HeaderLayout` takes each row's height in place of the fixed
  one, from a collapsed 22 points up to 400; the header's controls lay out
  for the room they have, the volume bar and pan hidden when collapsed. Rows
  under a row move with it; automation lanes keep their own height under it.
- **The cursor** is a cursor rect over the edge, as the clip trim's is
  (the trim cursor, PR #116).

## Done when

- A row dragged taller and shorter redraws its header, lane and clips at the
  new height, collapses to one line, and keeps the height after the song is
  closed and opened.
- Option-drag sets every row; a double-click restores the default.
