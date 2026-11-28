# Selected rows' height dragged together — proposed November 26, 2026

Status: proposed, Next item 7 of the backlog (D206).

## What

With several rows selected by Shift-click, dragging the lower edge of one of
them sets every selected row to the height dragged to, as Ableton's does; a
double-click on a selected row's edge puts every selected row back to its
default. A row not selected is dragged alone, as today, and Option still sets
every row.

## Why

The person's note of November 26, 2026: "You should be able to Shift-Click a
selection of tracks and adjust their all their track height together."

Today a drag sets one row, or every row with Option
([track height](track-height.md)); there is nothing between.

## Design

- **The selection is the header's,** tracks, groups and returns alike; the
  master keeps its height, as with Option.
- **One write** of `.daw/rows.json` when the drag ends, and Escape during the
  drag puts every selected row back, as for one row.
- **Folded rows:** a selected row inside a folded group is not shown and keeps
  its height.

## Done when

A test sets three selected rows' heights from one drag and leaves the rest;
a scripted run of the bundle drags one of two selected tracks and a picture
shows both at the new height.
