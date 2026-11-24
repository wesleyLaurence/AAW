# A time selection in the app (implemented 2026-11-24)

Status: built for the person's note of November 24, 2026: a group's or a
track's bars selected as a span, with the empty beats in it, then copied,
pasted or duplicated to move parts and sections around. It closes the gesture
question of [editing a range of bars](bar-ranges.md) and revises D80 (D204).

## What

- **Selecting beats.** A drag that starts in the clear of the timeline selects
  the beats it crosses, from grid line to grid line or with ⌘ off the grid, on
  every track and group row it crosses. A group row stands for itself, the
  groups in it and every track under it, folded or not. A drag along the ruler,
  under the loop strip, selects the beats on every track. The beats are lit on
  their rows, and in the bar and time strips above. Shift-click in the clear
  reaches the selection to the pointer from its far edge, on its own rows.
- **The clips inside.** The clips wholly inside the selected beats on its
  tracks are selected with them and outlined, so a drag of one of them moves
  them all, as before. A clip pressed, a row, a point or a marker selected, a
  click in the clear or Escape ends the time selection; the arrow keys move
  the clips and leave the beats behind.
- **The keys.** Each is one undo step, and acts on the selection's rows only:
  - **Delete** empties the beats. Clips across an edge are cut there and
    nothing moves (`range.clear`).
  - **⌘C** takes the beats as they are now: each track's clips cut to them,
    its empty beats, its lanes over them and the lanes of the groups in the
    selection. **⌘X** takes them and empties them.
  - **⌘V** puts them at the start position over their whole length, so empty
    beats in the copy empty the beats where it lands. They go to the tracks
    they came from, or from the track whose lane was clicked last down, a
    group's lane standing for its first track. A copy of every track goes back
    on every track. What was pasted is the selection after, and the start
    position moves to its end, so the next paste follows it.
  - **⌘D** copies the beats to right after themselves, over what is there,
    and moves the selection onto the copy, so ⌘D again goes on.
- **L and Export.** L loops the selected beats, and File › Export Audio…
  starts with them as its range.

At the end of a drag the start position is the selection's start, and the
track whose lane was clicked last is its first track, so ⌘C then a click
elsewhere and ⌘V puts the part there.

The agent has the same through `daw range` (`--track` names a group too), and
`daw section`.

## How

- **The host.** The range verbs take a group in `tracks`: every track under
  it, in the groups in it too, and its own lanes and theirs. A group that
  holds every track is the whole song, as every track named is.
  `aaw_host::range::take` writes down a range without touching the song: a
  track's pieces cut to it with positions from its start, the track's place
  among the tracks, the lanes over it as points with the value at each edge,
  and each group's lanes with its first track taken and how many groups up
  it is. `range.paste` cuts the destination's clips at its edges, removes
  those inside, puts the pieces there and pastes each lane over what is
  there, making a lane the track has none of where its parameter is one the
  track can have, and leaving it out where not. A paste to tracks that cannot
  hold a piece, audio on a MIDI track, or past the last track is refused.
- **The FFI.** `Song.copy_range(start, length, rows)` for ⌘C, and the edits
  `RangeClear`, `RangeCopy` (over, not inserting) and `RangePaste`.
- **The app.** `SongModel.timeSelection` is the span and its rows, empty for
  every track; `select(time:)` selects it with the clips inside, and any other
  selection ends it. `Clipboard.range` holds what ⌘C took, its length and the
  tracks it covered, for the selection after a paste. `ArrangementView` draws
  it over the lanes and drags it as `Drag.time`; the clip rectangle of D80 is
  gone from the track lanes, and stays in the automation lanes for points.

## Decisions

- **The drag in the clear selects time, as in Ableton,** rather than keeping
  Logic's rectangle and selecting time only from the ruler or with a
  modifier. The person asked for a drag over a group's or a track's bars, and
  chose this when asked. The clips wholly inside are selected too, so what the
  rectangle was for, moving several clips together, still works.
- **⌘D copies over what follows, on the selection's rows,** rather than
  inserting time. Inserting on some tracks alone would put them out of time
  with the rest; inserting time across every track, Ableton's Duplicate Time,
  is a line of its own. The person chose this when asked.
- **Paste replaces the whole length,** so the empty beats of a copy empty
  where it lands: the rests are part of a part, which is what the person
  asked for. It is `range copy` without `--insert` from a clipboard.
- **Copy takes the beats as they are,** in text the host wrote, rather than
  remembering the range and copying it at paste time: a Cut empties the
  source, and the song changes between Copy and Paste.
- **A copy maps onto tracks by place,** as clip Paste does: the first copied
  track onto the track clicked last, the others below it in order. A group's
  lanes go to the group as many levels up from the track its first copied
  track lands on, so a copy of one group pasted on a group built the same way
  carries its lanes; where there is no such group they are left out.
- **Selecting every track by hand is the whole song.** A group or the rows
  dragged that cover every track copy the sections and the returns' and
  master's lanes too, as `daw range` with every track named does. Paste
  never places sections.

## Limits

- Time is not inserted or deleted from the app (Duplicate Time, Delete Time),
  and a section is not dragged with its content; a backlog line.
- The time selection is not sent to the host, so `daw status` does not show it
  to the agent.
- The drag does not scroll the timeline when it reaches the window's edge.
- A paste never places sections or markers, even from a copy of every track.

## Code

`engine/crates/aaw-host/src/range.rs` (`take`, `range_paste`, groups in
`scope`), `command.rs` (`range.paste`); `aaw-ffi` `copy_range` and the three
edits; `apps/mac/Sources/AAWApp/SongModel.swift` (`TimeSelection`,
`select(time:)`, the keys) and `ArrangementView.swift` (`TimeDrag`,
`drawTimeSelection`). Tests: `crates/aaw-host/tests/range.rs` (a taken range
pasted is the copy `range copy` makes, byte for byte; a paste replaces its
whole length; a group is its tracks and its lanes),
`crates/aaw-ffi/tests/song.rs` and
`apps/mac/Tests/AAWAppTests/TimeSelectionTests.swift`.
