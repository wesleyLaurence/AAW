# A limiter that holds a true peak (implemented 2026-10-09)

Status: built in the pull request that closed the first Next item after the
clipper, decision D107. Before it the limiter held the samples alone, the
render's estimated true peak passed its ceiling, and `daw describe effects`
told the agent to leave a margin. There was no design file; what was settled
in building it is under Design.

## What

`true_peak` is a switch on the `limiter`, off unless given. On, the limiter
holds the level between the samples as well as the samples, so its
`ceiling_db` is a true peak:

```yaml
master:
  effects:
  - {type: limiter, ceiling_db: -1, true_peak: true}
```

A render of that song reports an `estimated_true_peak_dbtp` at or under −1,
and `daw listen` and `daw export` read the same from the file. `daw effect
add SONG master --type limiter --true-peak true` adds one, `daw set SONG
master.effects.0.true_peak true` switches one that is there, and in the app
the limiter's panel has a True peak switch under its three bars.

With it on, the limiter takes off a little more, what was over between the
samples, and is 19 frames later than its look-ahead, which the engine makes
up for as it does the look-ahead. A signal under the ceiling still comes out
bit for bit.

## Why

A file's samples are not its level. A converter, or a lossy encoder, makes
the curve between them, and that curve passes the highest sample: by a few
tenths of a decibel on most mixes and by up to 3 dB where the top is loud. A
limiter that holds samples at −1 dBFS therefore leaves a true peak over −1,
and an AAC or MP3 made from it can clip in the decoder. The agent's answer
was to set the ceiling lower by guess, or to turn the whole export down with
`--peak`, which gives the loudness back. A limiter that reads what the
render's report reads needs neither.

## Design

- **The limiter reads the estimate the reports give.** The render's report,
  `daw listen` and `daw export` estimate a true peak as the highest of the
  file four times oversampled, through the filter of scipy's
  `resample_poly`: 81 taps, so each of the three points between two samples
  is read from the 9 samples before and the 10 after. `truepeak::Between`
  is that filter a frame at a time, held to the resampler to twelve places
  by a test, and a frame's level is the highest of its sample and the three
  points up to the next, of either channel. The analyzer's meter reads the
  same estimate now. It had a shorter filter of its own, which read 0.04 dB
  over the ceiling on a real mix held at it, and a decibel under the
  report on noise.
- **The gain is the same across the samples a point is read from.** The
  limiter's gain is applied to the samples, and a point between them comes
  out as the filter makes it of the samples after the gain. So that it is
  the gain times what the point was, the gain has to be the same on all
  twenty: the window over which required reduction is maximized is 19
  frames longer and the audio 19 frames later, which puts every average of
  held reduction over those twenty samples over the peak. A peak that
  stands alone then comes out exactly at the ceiling, as a held tone does.
- **It aims 0.01 dB under the ceiling.** Where the gain is moving, towards a
  louder peak nearby or back from one, it is not the same across a point's
  samples, and the point comes out a little off. Measured on twenty seconds
  of white noise 3 to 36 dB over the ceiling, the worst case, since its
  peaks lie between the samples as often as on them and the gain never
  rests: 0.001 dB over with the look-ahead of 3 ms, 0.004 with 1 ms, 0.00001
  with 10 ms, and up to 0.03 with the shortest, 0.5 ms. The limiter
  therefore aims 0.01 dB lower, which covers a look-ahead of 1 ms and up and
  costs a hundredth of a decibel of level; `ceiling_db: -1` reads −1.01. A
  second stage correcting what the first leaves would hold the shortest
  look-ahead too and is not built.
- **The switch is off unless given,** so a song without it renders as it
  did. Its default is dropped from the forms an earlier song's fingerprint
  is checked against.
- **It is part of the device's structure.** The latency changes with it, so
  a playing song fades through the switch as it does through a change of
  look-ahead, and a limiter takes over another's state only when both read
  the same peaks.
- **Nothing else is added.** There is no margin field, no choice of filter
  and no oversampled gain: the gain at four times the rate, brought back by
  keeping every fourth sample, is the same gain on the same samples.

## What it does not hold

- **A cut.** A section preview is cut out of the song, and a cut in the
  middle of a sound is a step, whose curve between the samples overshoots:
  on a square wave, a clipped tone and a tone at a quarter of the rate the
  cut end read 0.3 to 0.9 dB over the ceiling. No limiter before the cut holds it. The
  same goes for a song with `end_fade_ms: 0` that stops while it sounds; the
  20 ms fade a song has unless told otherwise only lowers what the limiter
  held. On one real song the three sections that reach the ceiling read at
  it as previews too.
- **What follows it.** Only the last limiter of the master holds the file's
  true peak; the master's gain comes before the master's effects.
- **A certified reading.** The estimate is four times oversampled, as the
  report says. Four points a sample miss a crest that falls between them:
  a meter that oversamples further reads up to 0.2 dB higher on a tone at
  a quarter of the rate and up to 0.7 on one at half of it.

## Done when

- The estimate a frame at a time is the resampler's; on a true peak a tone
  between its crests, noise from 3 to 36 dB over the ceiling at four
  look-aheads and two rates and a lone peak between two samples come out at
  or under the ceiling, with no sample over it; under the ceiling the signal
  is bit for bit and 163 frames late at 3 ms; in every block size, and going
  on from another limiter's state: held by `crates/aaw-dsp/src/truepeak.rs`
  and `dynamics.rs`.
- Through `daw render`, a chain with the switch holds the estimate where the
  plain limiter leaves it 3 dB over, its latency is made up for, a master
  limiter's report reads at or under the ceiling in any block size, and
  `daw listen` reads the same: held by the Python suite.
- `daw describe effects` says what the switch does, where to set it and what
  it does not hold; `daw describe export` and the export's warning point to
  it.
- The app draws the switch from `describe`: seen in a scripted picture, and
  a scripted click on it changed the song.

## Open questions

- Whether the limiter on a true peak is heard to duck more than without, on
  a bright mix where it has a decibel more to take.
- Whether a new limiter should start with the switch on, since a master's
  limiter is what most are.
- Whether the 0.01 dB should be told apart in the report, which says −1.01
  for a ceiling of −1 and nothing of why.
