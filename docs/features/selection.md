# Selecting several things in the app (implemented 2026-10-06)

Status: built in the pull request that closed backlog item 1, decision D80.
Clips could already be selected with Shift and moved, copied and deleted
together, and notes selected by a rectangle and copied with Option. This
file is what was added so that the timeline, the pattern editor and the
automation lanes work the same way, as the piano roll does.

## What

- **Clips by a rectangle.** A drag that starts in the clear of the timeline
  draws a rectangle and selects every clip it touches, across tracks. With
  Shift, they are added to the clips already selected. The press sets the
  start position, as a click there always has.
- **Clips copied by a drag with Option.** A clip dragged with Option is
  copied where it is let go, with every selected clip, and the clips stay.
  The copies are outlined on the way and are the selection after. Option is
  read while the drag goes on and at its end, as in the Finder and the piano
  roll, and with Option held a press on an edge or a fade's handle copies
  too. Copies follow the rules of ⌘D: a pattern clip's copy plays the same
  pattern, a note clip's has notes of its own, and an audio clip that ends
  past the song's end makes the song longer.
- **Events in the pattern editor.** Shift-click adds an event to the
  selection or takes it out; a drag in the clear of a row of held hits or of
  notes selects the events a rectangle touches, in any row; ⌘A selects every
  event. The selected events move together by a drag and by the arrow keys,
  as far as each stays in the pattern and in its row of notes; with Option
  they are copied. Delete removes them, ⌘D copies them to right after the
  span they cover, and ⌘C, ⌘X and ⌘V copy, cut and paste them. A paste goes
  where the pattern was last clicked in the clear, or else right after the
  events in their own pattern and at their own beats in another. With
  several selected, the fields beside the editor show how many and a
  Velocity for all. A row of steps still paints steps where it is pressed.
- **Automation points.** Shift-click adds a point to the selection or takes
  it out; a drag in the clear of a lane selects the points a rectangle holds,
  in every lane it crosses. A selected point dragged moves every selected
  point by as many beats, on the grid as the grabbed one is, and by as much
  of its lane's height, heard as they move and one undo step. None passes a
  point of its lane that stays, and they stop together at a lane's top or
  bottom, so their shape is kept. Delete removes them, and a lane goes with
  its last point. The value shows beside the point when one is selected.

## How

- Edits through the FFI, each one undo step of the host's own commands:
  `ClipsCopy` (as far as `ClipsMove` would move the clips, by the commands
  `ClipsDuplicate` uses), `EventsMove`, `EventsCopy`, `EventsDuplicate`,
  `EventsPaste`, `EventsSet` and `EventsRemove` in place of the edits of one
  event, `PointsSet` (each point to its place, sent while it is dragged under
  one gesture) and `PointsRemove` in place of `PointRemove`. Events moved or
  copied together are of one pattern; in pitch, those with a note or a pad
  with a root note move, and the others keep their place.
- `EventCopy` is an event as Copy took it: its pad, place and length as the
  song wrote them, velocity, note and transpose. Pasting into a pattern whose
  tracks lack the pad, or past its end, is refused by the host with the
  reason.
- `SongModel` holds `selectedEvents` and `selectedPoints` as sets, as it does
  clips and notes, and sends them to the host for `daw status`. Copy, Cut and
  Paste act on clips, notes or events by which view has the keys
  (`EditPlace`).
- `PatternLayout.stepRange`, `beatRange` and `semitoneRange` bound a group of
  events, and `TimelineLayout.pointRange` and `liftRange` a group of points;
  each is tested in Swift.

## Limits

- The rectangle on the timeline selects clips and not a time range: the
  time selection across tracks of [editing a range of bars](bar-ranges.md)
  needs a gesture of its own.
- An event's end is dragged for that event alone, which the press selects.
- Steps are not selected, copied or pasted; an event is.
- Points are copied by neither Option nor Copy.
- Events of two patterns are not moved or copied together.
