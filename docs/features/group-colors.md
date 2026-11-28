# A group's color for its tracks — proposed November 26, 2026

Status: proposed, Next items 5 and 6 of the backlog (D206).

## What

1. **A new track in a group takes the group's color.** Add Track and Add MIDI
   Track in a group, any other way the app makes a track inside a group, and
   `daw track add --group G` write the group's color, its own or the one it shows
   by its place, into the new track's `color`.
2. **Apply Group Color to Tracks** in a group's right-click menu, beside
   Color ▸, colors every track under the group with the group's color, in one
   undo step.

## Why

The person's notes of November 26, 2026: "when you create a track within a
group it should inherit the color of the group" and "Apply color to tracks in
group should be a button in the right click menu when you click a group and it
takes the color of the group and applys to all tracks within it."

Today a new track with no color takes one by its place among the tracks
([colors](colors.md)), so a track added in a blue group comes out coral, and
coloring a group's tracks to match is a selection of each and a pick from the
grid.

## Design

- **Written, not followed.** The track gets the color as its own when it is
  made; changing the group's color later does not change it, as in Ableton.
  The menu item is how a group's tracks are brought back to it.
- **Every track under the group,** in groups within it too; the groups within
  keep their own colors. The edit is the one that colors several selected
  rows today.
- **A group without a color of its own** gives the color it is drawn with, its
  default by place, so what the person sees is what the tracks get.
- **The item is disabled** on a group with no tracks under it.
- **The agent:** `daw track add --group G` without `--color` takes the
  group's; `daw group color-tracks SONG G` is the menu item, one batch.

## Done when

Tests hold a track added in a group, from the host and from `daw track add
--group`, carrying the group's color, and the menu's edit coloring every track
under a nested group in one step; a scripted picture shows the group's tracks
recolored.
