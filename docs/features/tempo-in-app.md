# Tempo changes made in the app — implemented 2026-11-27

Status: implemented; the tempo half of item 5 of [tempo changes and warp
markers](tempo-and-warp.md#5-both-in-the-app), which the person asked for on
November 26, 2026 (D206). Warp markers in the app are still to come.

## What

The person makes and changes the song's tempo steps in the app, as the agent
does with `daw tempo`:

- **The master's Tempo lane.** + Lane on the master offers Tempo, which shows
  the lane with one step, the song's tempo. A step is dragged up and down for
  its tempo, the BPM shown beside it as it moves, and along from bar line to
  bar line, ⌘ off them. A double-click on the lane adds a step on the nearest
  bar line; a double-click on a step, or Delete, removes it.
- **The transport's tempo field** shows the tempo at the playhead and, typed
  into, sets the step the playhead is in, or the song's one tempo when it has
  no lane.
- **Tempo ▸ Follow This Clip** in one audio clip's menu runs `daw tempo follow`
  on it ([tempo follow](tempo-follow.md)).
- **An audio clip at its own speed**, one without `source_bpm`, draws its
  waveform and its beat map's beats through the tempo's steps, as it plays;
  before, they were drawn at the tempo where the clip starts and drifted from
  what plays across a step.

## Why

The song holds tempo changes and `daw tempo` writes them, but the person could
not make or change one in the app ([tempo changes](tempo-changes.md#limits)).

## Design

**A step is a point of the lane.** The lane is the master's `param: tempo`
([tempo changes](tempo-changes.md)), and the app edits it with the lane edits
every lane has: `lane.set` when Tempo is chosen, `point.add`, `point.set` and
`point.remove`. Each is one command through the host and one undo step, and a
drag is one step however often it is sent. The host keeps the lane's rules for
these commands, for the app and for `daw` alike:

- A point added to the tempo lane, or a point of `lane.set` for it, holds
  (`curve: hold`) unless it says otherwise; a ramp is refused, as before.
- The first point, the song's opening tempo, stays at beat 0, and no other
  point goes there: a move of it, or onto beat 0, is refused.
- The first point is not removed while others remain. Removed as the lane's
  last point, it takes the lane with it, and the song keeps its tempo as its
  one tempo.

**In the app.** A step lands on the nearest bar line where a point of another
lane lands on the grid; with ⌘, or with Snap to Grid off, it goes to a
thousandth of a beat as any point does, and `daw check` warns of a step off a
bar line. The opening step drags only up and down, and the others stop a bar
short of beat 0 (a thousandth, free). Option-click, which makes another lane's
point hold or not, says that a tempo step holds. Delete leaves the opening
step out of the selected points unless every step is selected, and then
removes it last. Typed into, the tempo field sets the value of the step the
playhead is in, the last at or before it.

**Follow This Clip** runs the bundle's `daw` as `daw tempo follow PROJECT @KEY`,
off the main thread, a step a bar; the command reaches the app's own host, so
its lane is one undo step. While it measures, the item is dimmed; done, the
lanes are shown; refused, its reason is the banner. A build that is not a bundle
has no `daw` and says so.

**Drawing through the steps.** `ClipTime` (`TempoMap.swift`) gives the second of
an audio clip's file at a beat. For a clip at its own speed in a song whose
tempo steps it reads the song's seconds through the map, times the clip's speed
(its repitch, the host's seconds a beat at its start times that tempo over 60);
for one with `source_bpm`, or in a song of one tempo, a beat is its seconds a
beat everywhere, as before. The waveform is drawn a piece a step, each at its
step's rate, and the beat map's ticks are placed by the inverse. A track's
peaks, which the backlog item named too, are sent by the host but drawn
nowhere in the timeline, so nothing of theirs drifts.

## Done when

- The host takes tempo steps as lane points, holds them, and keeps the opening
  one at beat 0 (`aaw-host/tests/tempo.rs`); the app's edits add the Tempo
  lane, add, drag, refuse and remove steps through the FFI
  (`aaw-ffi/tests/song.rs`).
- The bar snap, the drag's range, the order of removal, the step at the
  playhead, the pieces and a clip's file time across a step are tested
  (`Tests/AAWAppTests/TempoEditingTests.swift`).
- Scripted runs of the debug app on a generated song add a step, drag one,
  type a tempo at two places, refuse Delete on the opening step and picture
  a clip across a step, its file's accents a bar apart before it and 1.25
  bars after a step from 120 to 150. + Lane › Tempo and Follow This Clip
  are menus a scripted run does not choose from; the FFI test and
  `daw tempo follow`'s own tests cover what they send.

## Decisions

- **Steps are edited as points, not as `tempo.set`.** `tempo.set` writes the
  lane again from the map, which gives every point a new handle: a drag sent
  many times would lose its point after the first. Lane edits keep the handles,
  and the host's rules keep the steps a tempo. Rejected: lowering the app's
  point edits to `tempo.*` commands in the FFI.
- **A lane of one tempo shown in the app stays.** `daw tempo` writes a song
  back to one tempo without its lane (tempo changes' decision), but a lane the
  person showed to draw on would vanish under them as its last step went or a
  tempo was typed. Such a song saves as `schema_version: 3` until the lane is
  removed, by × or with its last point.
- **A step lands on the bar line.** The design asked for a step at the bar under
  the pointer, and a tempo nearly always changes on one; ⌘ goes off it as it
  goes off the grid for any point. Rejected: the grid, which put steps on beats
  a person rarely means.
- **The tempo field sets the step's value**, a `point.set` of the step the
  playhead is in, so a song of one step keeps its lane (open question 3 of the
  design). Rejected: scaling every step, which is `daw timeline --tempo-for`'s
  question, and asking.
- **Follow This Clip runs `daw`.** The beat map comes from `daw samples beats`
  in Python; the command already measures and writes in one step through the
  host, so the app runs it as File › Export Audio… runs `daw export`.

## Limits

- A step's tempo is dragged within the lane's range, the song's tempos with
  room either side, which grows after each edit; a tempo far outside it is
  typed into the field.
- Fades and leads in milliseconds are drawn at the song's opening tempo.
- Warp markers in the app are a later item of
  [tempo and warp](tempo-and-warp.md#5-both-in-the-app).
