# A finer spectrum in numbers — implemented 2026-10-09

Status: implemented (D101). `daw listen` reports the spectrum of the mix and of
each stem in third octaves, its tilt as one number, and each stem's resonances:
narrow peaks that stay where they are while the notes move, each with a
frequency, a height and a width. It adds to [perception](perception.md), which
measures seven bands.

```sh
daw listen projects/demo/renders/latest.json                 # third octaves, tilt, resonances
daw listen projects/demo/renders/latest.json --section drop  # one section's, of the mix and each stem
daw compare BEFORE AFTER                                     # each third octave's and resonance's change
daw compare RENDER --reference NAME                          # the balance in third octaves, and the tilt
```

```json
"spectrum_db": [-61.2, -48.0, -37.5, "... 31 values, 20 Hz to 20 kHz"],
"tilt_db_per_octave": -1.4,
"resonances": [
  {"freq_hz": 2490.0, "note": "D#7", "prominence_db": 6.3, "q": 8.3,
   "present_fraction": 1.0, "could_be_note": false}
],
"resonances_omitted": 0
```

## Why

An equalizer's band takes a frequency, a gain and a width. Seven bands, the
narrowest an octave wide, say that a stem is heavy between 2 and 4 kHz and not
that it rings at 2.5 kHz, so a bell was set from a guess or from a frequency
read coarsely off the spectrogram's picture. A person sweeps a narrow boost
until the ring jumps out; the same answer is a line of numbers.

Seven bands also hide the shape a reference is compared by: a mix with a bump
at 300 Hz and one that is full from 250 to 500 Hz report the same `low_mid`.

## Third octaves

`spectrum_db` is the power in each of the 31 third-octave bands at the standard
centers from 20 Hz to 20 kHz, in dBFS, the channels' powers averaged as
`band_dbfs` has them. The centers are listed once, as
`methods.third_octave_hz`. The lowest band starts at 17.8 Hz and the top one
ends at 20 kHz, where the seven bands end. It is read from a Welch spectrum
with a Hann window of 32 768 frames, so the 20 Hz band holds three bins;
`band_dbfs` and `band_fraction` keep their own window of 8192 frames and their
numbers. A band is `null` where the audio is too short for two bins to fall in
it, and in silence.

It is in every measurement: the mix, each section, each stem and each stem's
sections.

## Tilt

`tilt_db_per_octave` is the slope of a line fitted to `spectrum_db` from 50 Hz
to 10 kHz. Pink noise, which has equal power in every octave, is 0; white noise
is +3. It is brightness as one number that does not move with the level.

A band more than 60 dB under the loudest is fitted as 60 dB under, so the floor
of a sound with nothing up there does not set the slope, and the tilt is `null`
where over half the bands are that far under: a tone, a kick or a hat fills too
few bands to have a tilt.

## Resonances

For each stem, not the mix: a mix's peaks are its instruments.

**The curve.** The stem's spectrum is taken a bar at a time, or as many bars
as last a second, from Hann windows of 16 384 frames that lie inside the bar.
It is put on a curve of 48 points to the octave and smoothed over a twelfth of
an octave. That joins the partials of notes a semitone apart, so a scale's
partials are not each a peak, and still stands a ring apart from its
neighbors. A point's height is its level over the median of the two octaves
about it. A median, where the proposal had the spectrum smoothed across an
octave: a slope or a filter's corner is not a peak against a median, and a
peak's own height does not lift what it is measured from. Two octaves, since
one halves the height of a peak as wide as a `q` of 4.

**A resonance** stands 5 dB or more in the median of the curves of the bars
where the stem sounds, and is a peak of its own within 3% of that frequency in
at least half of those bars. A bar sounds when the stem is within 40 dB of its
loudest bar; a point counts within 70 dB of its bar's loudest point; peaks are
read from 40 Hz to 16 kHz, and two closer than a sixth of an octave are one.

| Field | Meaning |
|---|---|
| `freq_hz` | The top of the peak, to three figures |
| `note` | The nearest pitch, for reading against the notes |
| `prominence_db` | Its height over the median of the two octaves about it, in the median of the bars |
| `q` | Its frequency over its width 3 dB down, or halfway down a peak under 6 dB, which is where a bell of that gain counts its `q` |
| `present_fraction` | The share of the bars where the stem sounds in which it is a peak |
| `could_be_note` | `true`, `false` or `null`: whether the song's notes could explain it |

`freq_hz`, `prominence_db` and `q` are the three numbers an `eq`'s bell takes.
A bell of +9 dB at 2.5 kHz with a `q` of 8 on a sawtooth line that never
settles reads 2490 Hz, 6.3 dB and a `q` of 8.3: the bell's skirt lifts the
median a little, so the height reads under the gain. A cut of the reported
three numbers takes the height to 1.2 dB.

**`could_be_note`** is read from the song, through what each track plays
(`aaw_model::schedule::sounded`: every note with the pitch it sounds at, a
Synth's at each oscillator, and every hit without a note with its pad and
transpose):

- `true` when nothing the track plays moves in pitch, as a drum's hits or one
  held note: nothing then tells a ring from the sound's own partials. And when,
  in at least half the bars, a note it plays has one of its first sixteen
  partials within half a semitone of the peak, or a hit plays a sample as it
  is. The sixteenth partial is a semitone from the next, which is as far as the
  smoothing stands partials apart.
- `false` when the track's notes move and none explains a peak that stays,
  which is what a resonance is.
- `null` when the song does not say: the track plays audio clips, or nothing.

A group is read from its tracks, and a return from the tracks that feed it.

**How many.** Up to five a stem, those whose `could_be_note` is not `true`
first and then the most prominent, with `resonances_omitted` counting the
rest. `resonances` is `null` for a stem that sounds in fewer than four bars,
which are too few to tell what stays from what passes, and `[]` when it was
read and none was found.

## How much is printed

`spectrum_db`, the tilt and the resonances over the whole render, for the mix
and each stem. By section the third octaves and resonances are written to
`listen.json` and not printed, since thirty-one numbers for every stem in
every section is most of a report nobody asked for; a section's tilt, one
number, is printed.

`daw listen RENDER --section ID` prints one section in place of the report:
`mix`, the mix's measurements there, and `tracks`, each stem's, with their
third octaves and the stem's resonances over the section's bars.

## Comparing

`daw compare BEFORE AFTER` has, for the mix and each stem, `spectrum_db` under
`actual_delta` and `loudness_matched_delta`, the change in each third octave,
and the change in `tilt_db_per_octave`. A section has the tilt's change alone.

Each stem has `resonances`: every one either render lists, with `before_db`
and `after_db`, its height in each render, measured at that frequency whether
or not the render lists it (`listed_before`, `listed_after`), and `delta_db`,
the largest change first. A bell cut at a reported resonance shows as its
height falling by about the cut.

`daw compare RENDER --reference NAME` has `whole.delta.spectrum_db`, the mix's
balance against the reference's in third octaves with the median of the
differences taken out, as `band_db` has it in seven, and
`delta.tilt_db_per_octave`, whole and by section. Its observations say the
whole mix's balance in third octaves, each run of bands past 3 dB one way as
one range, and the tilt past 0.5 dB an octave. See
[reference-comparison.md](reference-comparison.md).

[Overlap](masking.md) takes its bands from this list: the third octaves from
100 Hz up, and the seven below as two, from 20 Hz.

## The picture

`spectrum.png`, beside `overview.png`: the third-octave curve of the mix in
black over those of its stems, for when the question is which stem makes a
bump. The eight stems that come nearest the mix in any band each have a color,
in the render's order, and the rest are gray and counted in the legend. The
numbers are each stem's `spectrum_db`.

## Time

On this Mac, an Intel one of eight cores, the third octaves and resonances add
about three seconds to `daw listen` of a generated three-minute song of twelve
stems and six sections, which takes about fifty in all.

## Limits

- The thresholds, 5 dB over two octaves' median, a twelfth of an octave, 3%
  and half the bars, were chosen on generated audio and read on one real
  render of fourteen stems. Nobody has listened: whether the peaks named are
  the ones a person hears is under Verify.
- `prominence_db` reads under the gain of a bell that made the peak, by about
  a third at a `q` of 8, and by more on a spectrum that leans steeply.
- A peak narrower than a twelfth of an octave reads as about that wide, a `q`
  of 16 or so, and at the power it holds in that width: a ring's own width and
  depth are not measured.
- A peak wider than a `q` of about 4 is not found, since it is most of the two
  octaves it is measured against. `spectrum_db` shows a broad bump.
- A part whose partials stand apart, a melody in the middle octaves or a riff
  that repeats every bar, lists its notes, with `could_be_note: true`, and a
  resonance among them can be `true` as well or left out of the five.
- A sample's pitch is taken as its `root_note` says, a Synth's pitch envelope
  and glide are not followed, and an audio clip's notes are unknown.
- Dips are not reported: a band a stem lacks against its neighbors is harder
  to tell from an arrangement's choice than a peak is from a note.
- A file that is not a render has third octaves and a tilt, and no resonances.
- A section's third octaves are not compared with a reference's: they follow
  the notes it plays.

`daw describe listen` says what each field means.
