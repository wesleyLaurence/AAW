# The compressor and the limiter seen working — proposed November 26, 2026

Status: proposed, Next item 2 of the backlog (D206).

## What

While the song plays, the compressor's and the limiter's panels show how much
each is taking off, as Ableton's do:

- **The compressor** grows a display: the last few seconds of what reaches it,
  scrolling, with its threshold as a line across it that is dragged up and
  down, and the gain it takes off hanging from the top in a color of its own.
  A meter beside it says the reduction now, and the most in the last seconds
  as a number.
- **The limiter** shows a gain reduction meter: a bar that falls from the top
  in a color as it turns the level down, the most it took off in the last
  seconds held as a number under it, the ceiling marked.
- **The clipper**, which reports what it takes off as the limiter does, shows
  the same meter.

A device folded to its strip keeps a thin reduction bar along it, so a chain
of folded devices still shows which one is working.

## Why

The person's note of November 26, 2026: "The limiter and compresser are hard
to interpret, how much are things being compressed? In ableton there is a
color which shows while something is being plyed to show how much the limiter
is being activated. the compresser you can see the waveform in a screen with
the threshold as a line you can drag down to get it. Better more intuitive
visualizations in the compressor and limiter."

Today both panels are rows of fields. What each takes off is measured only in
a render, by `daw render` and `daw listen`
([dynamics in detail](dynamics-detail.md)), which the person does not see
while playing. Setting a threshold by ear with no picture of where the level
sits against it is guesswork.

## Design

- **A tap per device, as the equalizer's spectrum is tapped.** The audio thread
  writes, for each compressor, limiter and clipper on a track, a group, a
  return or the master, a short ring of hops (one hop each 256 frames, about
  5 ms): the detector's level in, the level out and the most gain reduction in
  the hop, each one atomic store and no waiting. The taps live beside the
  compiled program's cache keyed by the row and the effect, as the
  [parametric EQ](parametric-eq.md)'s do, so an edit does not clear them. A
  device inside a Synth's patch is not tapped.
- **The app reads the rings each frame drawn** while the device panel shows
  them, and draws nothing new while stopped: the picture holds what was last
  played, and a play starts it scrolling again. Nothing is drawn from a
  bypassed device.
- **The compressor's display:** the input level as a filled envelope in dB,
  0 dB at the top and −60 at the bottom, the output level over it, and the
  reduction from the top. The threshold line is drawn at `threshold_db`; a
  drag on it sets `threshold_db`, heard as it moves and one undo step when let
  go, as a dragged EQ point is. A threshold under automation draws the lane's
  value and is not dragged, as an automated field is not typed into.
- **The meters** read the reduction in dB down from 0, to 24 for the
  compressor and the limiter and to the clipper's ceiling range for the
  clipper; the held number resets on a click and at play.
- **The agent** has what it has: the render's report and `daw listen` say each
  device's reduction by section. Nothing new in `daw`.

## Done when

Scripted runs of the bundle on a generated song with a compressor on a track
and a limiter on the master show, in a picture taken while playing, the
compressor's envelope and threshold line and both meters reading reduction;
a test holds that the taps write the reduction the render's report measures
for the same audio; a test holds the threshold drag's value at a point.

## Open questions

- Whether the compressor also shows its transfer curve, input against output
  with the knee, as Ableton's other view does.
- Whether a track's header shows a reduction mark for a device on it, so it
  is seen without opening the device panel.
