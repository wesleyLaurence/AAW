# Effects dragged onto a track and along a chain — proposed November 26, 2026

Status: proposed, Next items 2 and 3 of the backlog (D206).

## What

1. **An effect dropped anywhere on a track.** An effect, a rack or a plugin
   from the browser dropped on a track's, a group's, a return's or the
   master's row, on its header or its lane, goes last in that row's chain; the
   row lights while the drag is over it. An effect dragged by its title out of
   the device panel onto another row moves it there; with Option it copies it,
   as today.
2. **An effect dragged along its chain.** An effect's title dragged anywhere
   over its chain in the device panel moves it to the gap nearest the
   pointer, with a line drawn in the gap, as Ableton's chain does. The gaps
   between devices are the line, not a 12-point strip to find.

## Why

The person's notes of November 26, 2026: "Need to be able to drag an effect on
to a track" and "Need to be able to drag an effect to move its position in the
chain for a track".

Both were built in part and never dragged by hand
([completed](../completed.md): #71, #120, the browser):

- A device from the browser lands only on a row's **header**: the timeline
  refuses it over the lane (`deviceLanding` asks for `x < headerWidth`), where
  a sample from the browser does land. A drop on the lane does nothing and
  says nothing.
- An effect moves along its chain only when dropped on one of the
  **insertion strips** between devices, each 12 points wide, through SwiftUI's
  `onDrop`. A drop on a device itself does nothing. A scripted drag never
  reaches a SwiftUI drop, so #120 was tested only by pictures and pure
  functions.
- An effect dragged from the panel onto another row's header is refused
  without Option.

## Design

- **Find the failure first.** Before changing the targets, a debug build logs
  each drag's enter, update and drop on the timeline and the chain, and the
  person's two gestures are reproduced by the agent where a scripted run can,
  so the fix covers what failed and not only what is guessed.
- **The row is the target.** The timeline's drop takes a device over the row
  under the pointer, header or lane, and lights that row, as a sample's drop
  lights its landing. Below the last track an instrument still makes a new
  track; an effect there is refused, as now.
- **A move between rows** is one edit through the host: the effect leaves its
  chain and goes last in the other's, with its lanes, as `daw effect move
  --to OWNER` will take it; a compressor keyed from the row it lands on is
  refused with the reason, as a copy is.
- **The chain is one target.** The device panel's chain takes the drop over
  its whole width in an AppKit view, as the timeline's does, so that a
  scripted drag reaches it and the move can be checked by a scripted run. The
  gap is the nearest boundary between two devices to the pointer, refused at
  the two either side of the dragged one, as `DeviceChain.moveTarget` says
  today. A rack's frame is a chain of its own, as its strips are now.
- **The agent** has `daw effect add` and `daw effect move`; `daw effect move`
  gains `--to` another row for the move between rows.

## Done when

Scripted drags of the bundle on a generated song drop a browser effect on a
track's lane and an effect between two others of a chain, each landing as
the matching `daw` command writes it; a test holds the gap chosen at each x.

## Open questions

- Whether a drop on a clip adds the effect to the clip's track, or is left to
  the clip, for a clip effect that does not exist yet.
