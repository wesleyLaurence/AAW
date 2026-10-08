# A finer spectrum in numbers — proposed October 7, 2026

Status: proposed, not built. Backlog: Next, A finer spectrum in numbers. It
adds to [perception](perception.md), which measures seven bands.

## What

`daw listen` reports the spectrum of the mix and of each stem in third octaves,
its tilt as one number, and each stem's resonances: narrow peaks that stay
where they are while the notes move, each with a frequency, a height and a
width.

```json
"spectrum_db": [-61.2, -48.0, -37.5, "... 31 values, 20 Hz to 20 kHz"],
"tilt_db_per_octave": -1.4,
"resonances": [
  {"freq_hz": 2480, "note": "D#7", "prominence_db": 9.1, "q": 11,
   "present_fraction": 0.86, "could_be_note": false}
]
```

## Why

An equalizer's band takes a frequency, a gain and a width. `daw listen` gives
seven bands, the narrowest an octave wide, so the agent can tell that a stem is
heavy between 2 and 4 kHz and not that it rings at 2.5 kHz. It sets a bell from
a guess, or reads a frequency off the spectrogram's picture, which it does
coarsely. A person sweeps a narrow boost until the ring jumps out; the same
answer can be a line of numbers.

Seven bands also hide the shape a reference is compared by: a mix with a bump
at 300 Hz and one that is full from 250 to 500 Hz report the same `low_mid`.

## Design

**Third octaves.** `spectrum_db` is the power in each of the 31 third-octave
bands from 20 Hz to 20 kHz, with the centers listed once under `methods`. It is
read from the same Welch spectrum as `band_dbfs`, with a window of 32 768
frames so the lowest bands hold more than a bin or two; in audio too short for
that window the bands a bin cannot resolve are `null`. `band_dbfs` and
`band_fraction` stay as they are.

**Tilt.** `tilt_db_per_octave` is the slope of a line fitted to `spectrum_db`
from 50 Hz to 10 kHz. Pink noise, which has equal power in every octave, is 0;
white noise is +3. It is brightness as one number that does not move with the
level.

**Resonances,** for each stem, not the mix. The stem's spectrum is taken a bar
at a time. In each bar, a peak is a bin that stands 6 dB or more over the
spectrum smoothed across an octave. A resonance is a peak found at the same
frequency, within 3%, in at least half the bars where the stem sounds:

- `freq_hz`, and `note`, the nearest pitch, for reading against the notes;
- `prominence_db`, its median height over the smoothed spectrum;
- `q`, its frequency over its width 3 dB down, which is the `q` a bell takes;
- `present_fraction`, the share of sounding bars it is found in;
- `could_be_note`: true when the track plays one pitch throughout, or the
  frequency is a harmonic of a pitch it plays in most bars, read from the
  song's schedule. A peak that is the music is not a resonance, and where the
  two cannot be told apart the report says so.

Up to five a stem are printed, the most prominent first.

**How much is printed.** `spectrum_db`, the tilt and the resonances over the
whole render, for the mix and each stem. By section they are written to
`listen.json` and printed with `--section ID`, since thirty-one numbers for
every stem in every section is most of a report nobody asked for.

**Comparing.** `daw compare` reports the change in each third octave after
matching loudness, the change in tilt, and resonances that went, came or
changed height. A bell cut at a reported resonance shows as that resonance's
prominence falling. Once this is built, the
[reference comparison](reference-comparison.md) compares third octaves and the
tilt where it compared seven bands, and [masking](masking.md) reports its bands
from this list.

**A picture.** `spectrum.png`: the third-octave curves of the stems over one
another with the mix's on top, for when the question is which stem makes a
bump.

## Done when

- Generated pink noise reports a tilt within 0.2 of 0 and white noise within
  0.2 of +3. A 1 kHz tone is in the 1 kHz band and no other.
- A sawtooth melody through an `eq` bell of +9 dB at 2.5 kHz with a `q` of 8
  reports one resonance within 3% of 2.5 kHz, with a `q` between 5 and 12 and
  `could_be_note: false`. Without the bell it reports none. A track that holds
  one note reports its harmonics with `could_be_note: true`.
- `daw compare` of a render before and after a cut at the reported frequency
  reports the prominence falling by about the cut.
- `daw describe` says what each field means.

## Open questions

- The thresholds, 6 dB over an octave's smoothing and half the bars, are a
  first guess and were not run on audio.
- Resonances of an audio clip of a whole song, where no schedule says which
  peaks are the music.
- Holes: a band the mix lacks against its neighbors. A dip is harder to tell
  from an arrangement's choice than a peak is from a note.
- Whether the printed report should be shorter by default now that it grows,
  with the rest behind a flag.
