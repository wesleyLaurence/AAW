# The compressor and the limiter seen working — implemented 2026-11-26

Status: implemented; the person's note of November 26, 2026 (D206).

## What

While the song plays, the panels of a compressor, a limiter and a clipper show
how much each is taking off, as Ableton's do:

- **The compressor** has a display beside its fields: the last four seconds
  of the level reaching its detector, filled, the level leaving it as a line
  over that, and the gain it takes off hanging from the top in yellow, all on
  one scale from full scale at the top to −60 dB at the foot, so the output
  sits under the input by what was taken off. Its threshold is an orange line
  across the display, its value written on it; a press on the line and a drag
  up or down sets `threshold_db`, heard as it moves and one undo step when let
  go. A threshold that a lane moves is drawn dashed, as it played, and is not
  dragged. A meter at the display's right edge shows the reduction now, from
  the top, on a 24 dB scale, and the display's lower right corner the most
  taken off, as `−5.6`.
- **The limiter and the clipper** have a meter beside their fields: the level
  reaching them, from −60 dB to +6, with the ceiling marked in orange and the
  bar turning orange past it, and beside it the reduction falling from the top
  in yellow, 24 dB deep for the limiter and 36 for the clipper, whose drive
  reaches 36 dB. The most taken off is under both as a number and a line
  across the reduction bar.
- **A folded compressor, limiter or clipper** keeps a thin yellow bar of its
  reduction along the strip's right edge, so a chain of folded devices still
  shows which one is working.

The most taken off is held from the moment the song is played from a stop
until a click on the meter or the display (anywhere off the threshold line).
While the song is stopped the display holds what was last played and the bars
fall away at 24 dB a second. A bypassed device shows nothing.

The agent has what it had: the render's report and `daw listen` give each
device's reduction by section ([dynamics in detail](dynamics-detail.md)).
Nothing new in `daw`.

## Why

The person's note of November 26, 2026: "The limiter and compresser are hard
to interpret, how much are things being compressed? In ableton there is a
color which shows while something is being plyed to show how much the limiter
is being activated. the compresser you can see the waveform in a screen with
the threshold as a line you can drag down to get it. Better more intuitive
visualizations in the compressor and limiter."

Both panels were rows of fields. What each took off was measured only in a
render, which the person does not see while playing, and setting a threshold
by ear with no picture of where the level sits against it was guesswork.

## How it works

- **A gauge a device** (`aaw_dsp::gauge`). A compressor, a limiter and a
  clipper on a track, a group, a return or the master each fold every frame
  they process while the transport rolls into a hop of 256 frames (5.3 ms at
  48 kHz): the loudest sample their detector heard (a compressor's key when it
  has one, a limiter's level between the samples when it reads true peaks, a
  clipper's input after its drive), the loudest sample they put out, the most
  gain they took off and the threshold or ceiling as automation had it. Each
  hop goes into a ring of 1024 (5.5 s at 48 kHz) as four atomic stores and a
  count; the audio thread never waits and never allocates. Frames played while
  stopped make no hops, which is why the picture holds.
- The gauges live in the program cache's `Taps` beside the equalizers' taps
  and the analyzers' rings, keyed by the row and the effect's index in the
  chain as the chain runs it, so an edit keeps the same gauge and a compile
  that drops the device (or bypasses it) drops it. A device inside a Synth's
  patch has none.
- **`Song::gain_reduction(effect, hops)`** in `aaw-ffi` gives the last hops,
  oldest first, as little-endian float bytes, with the count made so far.
- **The app** (`DynamicsView.swift`) reads it at the display's rate while the
  panel shows: the display asks for four seconds of hops, a meter and a strip
  for 64. The geometry, the folding of hops into columns, the threshold drag
  and the held most are `DynamicsLayout.swift`, tested in `Tests`.

Tests: the gauge's hops in `aaw-dsp`; in the renderer's tests, the most each
gauge of the mix song held over a whole render equals the reduction the
render's report measures, and stopped frames make no hops; in the FFI's, the
call for each kind and for a bypassed device; in the app's, the layout, the
threshold at a point and the held most.

## Decisions

- **The compressor's display goes beside its fields, not over them,** and its
  panel is 420 points wide. At the detail panel's least height the seven
  fields leave no room under or over them.
- **One dB scale in the display for level and reduction.** A reduction drawn
  on its own deeper scale would be easier to see when slight, but the output
  would no longer sit under the input by what was taken off. The meter beside
  the display reads the reduction on a 24 dB scale and the number gives it
  exactly.
- **The output is a line over the input's fill.** Filled, it covered the
  input wherever makeup gain brought it back to the same level, which is the
  usual case.
- **The clipper's meter reads to 36 dB,** its drive's range, rather than its
  ceiling's range, which is not a depth of reduction.
- **A hop is the most of its frames,** not the mean: a transient the limiter
  catches for 2 ms shows, as the render's report's maximum does.
- **The held number starts at a play from a stop and at a click.** A held
  peak over "the last seconds" would vanish while the person reads it.

## Limits

- The bars show the reduction of the last hop. A sparse part with a short
  release empties them between notes, and only the held number and the
  display show what the hits took.
- The display shows no transfer curve, input against output with the knee, as
  Ableton's other view does, and a track's header shows no reduction mark.
  Either can come back as a backlog item if the person asks.
