# Effects dragged onto a track and along a chain — proposed November 26, 2026

Status: item 1, an effect dropped anywhere on a track, built November 28,
2026 (see [Built](#built-an-effect-dropped-anywhere-on-a-track)); item 2,
an effect dragged along its chain, proposed, Next item 1 of the backlog
(D206).

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
  its whole width in an AppKit view, as the timeline's does, so that the
  gap is chosen by the view's own geometry and a scripted run can hand it a
  drop, as `--drop-device` does the timeline (a scripted `--drag` never
  completes a drag session; see Decisions). The
  gap is the nearest boundary between two devices to the pointer, refused at
  the two either side of the dragged one, as `DeviceChain.moveTarget` says
  today. A rack's frame is a chain of its own, as its strips are now.
- **The agent** has `daw effect add` and `daw effect move`; `daw effect move`
  gains `--to` another row for the move between rows.

## Done when

Scripted drags of the bundle on a generated song drop a browser effect on a
track's lane and an effect between two others of a chain, each landing as
the matching `daw` command writes it; a test holds the gap chosen at each x.

## Built: an effect dropped anywhere on a track

- **The row is the target.** A device from the browser, an effect, a rack, a
  plugin, a MIDI effect or an instrument, lands on the row under the pointer
  over its header or its lane, any track, group, return or the master that
  takes that kind (`SongModel.canAddBrowserDevice`), last in its chain; the
  row is lit while the drag is over it, the header more than the lane. Under
  the tracks an instrument still makes a new track, the band a new header
  would have lit.
- **An effect out of the device panel** moves to the end of the chain of the
  row it is let go over, with the lanes on it, and that row's devices show
  with it selected; with Option it is copied there, onto any row, its own
  too. A move onto the row it is on does nothing and lights nothing.
  `SongModel.effectLanding` says which, and is tested.
- **The engine** moves an effect between rows: `effect.move` with an
  `owner` of another row's chain or a rack of it, `daw effect move --to
  OWNER`, `Edit::EffectMoveTo` from the app. The lanes on it move to the row
  it lands on, an id that chain has numbered (`warm-2`) and the lane renamed
  with it; validation refuses a compressor onto the track that keys it, with
  the reason, as a copy is refused.
- **Declared drag types.** The app's drag types are declared in its
  Info.plist (`UTExportedTypeDeclarations`, each conforming to
  `public.data`): `org.aaw.browser-device`, `-patch`, `-rack`, `-plugin`,
  `-instrument-rack`, `org.aaw.effect`, `org.aaw.synth-effect` and
  `org.aaw.synth-source`; a test holds the list. A device from the browser
  carries `org.aaw.browser-device` besides its own
  `org.aaw.browser-device.KIND`, which stays undeclared, and the browser
  holds what it is dragging (`Browser.draggedDevice`) from when the drag
  begins, as the panel holds `draggedEffect`; the timeline and the device
  panel read the kind there.
- **Scripted runs** hand a device to the timeline with `--drop-device
  KIND,X,Y`, as `--drop FILE,X,Y` hands a file.

## Decisions

- **What failed.** Neither gesture had been made by hand. Besides the header
  alone taking a device and Option alone taking an effect, macOS knew none of
  the app's drag types: `UTType("org.aaw.effect")` is nil for an identifier
  no app declares, and SwiftUI's drags and drops work in `UTType`s, which
  matches the `dyn.…` types a scripted drag logged in #120. An AppKit view
  registered for the literal name could then never be offered the drag.
  Declaring the types was chosen over carrying the data as plain text, which
  every text field would take.
- **One declared device type,** with the kind held by the browser, over
  declaring a type for each kind: the kinds come from the engine, and a new
  one would go undeclared without a sign.
- **A move without Option,** as Ableton moves a device dragged to another
  track, over the copy alone that #71 made: the person's note asked to drag
  an effect onto a track.
- **Not checked by a real drag.** A scripted `--drag` posts mouse events
  to the window, which never complete a drag session, and a drag made at the
  system's level would take the person's own pointer. The landings are held
  by tests and `--drop-device` runs; the drag itself waits for the person's
  use of the app.

## Open questions

- Whether a drop on a clip adds the effect to the clip's track, or is left to
  the clip, for a clip effect that does not exist yet.
