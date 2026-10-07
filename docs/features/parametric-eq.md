# The parametric EQ (implemented 2026-10-07)

Status: built in the pull request that closed the person's note of October 6,
2026 asking for a parametric EQ, decision D91. Before it, the `eq` effect had
bells and shelves alone, and the app showed them as rows of numbers: there was
no way to cut below or above a frequency in an equalizer, and nothing drew
what the bands did together.

## What

The `eq` effect is the parametric EQ: up to 16 bands of five shapes, drawn in
the app as one curve over the spectrum of what the equalizer puts out, each
band a point that is dragged. The effect that was there is this one grown, so
every song with an `eq` plays as it did.

```yaml
effects:
  - type: eq
    id: tone
    bands:
      - {shape: highpass, freq_hz: 80, slope_db_per_octave: 24}
      - {shape: bell, freq_hz: 400, gain_db: -3, q: 1.2}
      - {shape: high_shelf, freq_hz: 8000, gain_db: 2}
      - {shape: lowpass, freq_hz: 16000, q: 1.4}
```

| Field | Range | Default | What it does |
|---|---|---|---|
| `shape` | `bell`, `low_shelf`, `high_shelf`, `highpass`, `lowpass` | required | A bell raises or lowers a region; a shelf everything below or above; a pass cuts past the frequency |
| `freq_hz` | 20…20000 | required | The bell's centre, the shelf's corner or the pass's cutoff |
| `gain_db` | −24…24 | 0 | How far a bell or a shelf raises or lowers; a pass ignores it |
| `q` | 0.1…18 | 0.71 | A bell's width (higher is narrower); a shelf's slope and a pass's resonance at its corner, 0.71 being flat and higher lifting it |
| `slope_db_per_octave` | 12, 24, 36, 48 | 12 | How steeply a pass cuts: a Butterworth filter of that order, as the `filter` effect's; a bell or a shelf ignores it |

`freq_hz`, `gain_db` and `q` of each band are lane targets as
`effects.REF.bands.N.FIELD` and glide when edited while the song plays;
`shape` and `slope_db_per_octave` change the device's sections, so a playing
song fades through a change of them as it does through an added effect.
`daw effect add SONG tracks.T eq --bands '[...]'` adds one, `daw set` reaches
each field, and `daw describe effects` says what each does.

In the app the panel is the curve, 20 Hz to 20 kHz across and ±30 dB up, with
a numbered point for each band and the spectrum of the equalizer's output
filled under it while the song plays, from −90 dB at the foot to full scale
at the top. Under the curve are the bands' numbers and the selected band's
fields: its shape, frequency and q, and its gain or, for a pass, its slope.

| Input | Does |
|---|---|
| Drag a point | Moves the band's frequency across and its gain up and down; a pass's point sits at its corner and up and down sets its resonance instead. Heard as it moves, one undo step |
| Option-drag a point up or down | Narrows or widens the band: q doubles each 40 points up and halves each 40 down |
| Click a point, or a number under the curve | Selects the band, whose fields show |
| Double-click in the clear | Adds a bell at that frequency and gain, up to 16 |
| Double-click a point | Removes the band, unless it is the last |
| + in the title, − under the curve | Adds a band at 1 kHz, removes the selected one |

## Why

The person's note of October 6, 2026 asked for a parametric EQ: up to eight
bands drawn as one curve over the playing spectrum, high and low pass with
their slopes, shelves and bells, each a point dragged for frequency and gain
with its width, and the three-band EQ staying. The `eq` effect already had
bells and shelves with a q, so a second equalizer beside it would have left
the agent choosing between two devices that did the same things; the note's
"three-band EQ stays" is honoured by changing nothing an existing `eq` means.
What was missing was the passes and their slopes, and the panel: an
equalizer is set by ear and by eye at once, a dip dragged onto the resonance
the spectrum shows, which rows of numbers cannot do.

## Design

- **Pass bands are the `filter` effect's Butterworth sections** with the
  band's q applied to the last section, scaled so that 0.71 is the Butterworth
  at every slope: at 12 dB the one section's Q is the band's q, and at 24, 36
  and 48 the sections before it keep their Butterworth Qs. A tone at the corner
  comes through at the product of the sections' Qs, so q is heard as a lift at
  the cutoff whatever the slope, as EQ Eight's resonance is. The moving
  filter the lanes use has the same sections, so a swept cutoff matches.
- **The field a shape has no use for is kept and ignored**, rather than
  refused: a band's gain stays as it was when its shape is changed to a pass
  and comes back when it is changed again, and a slope on a bell waits for the
  shape that counts it. The panel shows only the fields that count.
- **`gain_db` is 0 unless given**, so that a pass band is written without it;
  the song still writes it on every band, as it did while it was required, so
  no saved song changes. The slope is written only when it is not 12, and the
  fingerprint forms earlier engines wrote drop it beside a band's shape, so
  their render reports still verify.
- **The curve is what plays**: the app evaluates the same cookbook sections
  at the song's sample rate, so what is drawn near 20 kHz is the digital
  filter's response, not an analog sketch of it.
- **The spectrum is the equalizer's output**, tapped on the audio thread into
  a ring of the last 4096 frames as mono, one atomic store a frame and no
  waiting; the app reads the ring each frame drawn and works out a Hann-
  windowed FFT off the audio thread, so the picture costs the audio nothing.
  The taps live beside the compiled program's cache, keyed by the row and the
  effect's index, so an edit leaves the ring where it was and the picture
  does not blink. Only inserts on tracks, returns and the master are tapped;
  an equalizer inside a Synth's patch keeps its rows in the Synth's panel.
- **Sixteen bands, not eight.** The model took sixteen before the note and
  reducing it would refuse songs that load today; the panel adds up to sixteen.
- **No notch, no band on or off, no mid/side.** Each is a line in the backlog
  when it is missed; a band is taken out with − or put at 0 dB.

## Done when

- A pass band renders as the Butterworth of its slope at q 0.71, lifts its
  corner by its q and ignores its gain, and a swept pass matches the still
  one: held by `crates/aaw-dsp/src/biquad.rs`, `svf.rs` and the effects
  contract test, and through `daw render` by the Python suite.
- The agent adds and sets one through `daw effect add` and `daw set`, and
  `daw describe effects` says what each shape does with each field.
- The app draws the curve from the bands, a point a band, the spectrum under
  it while the song plays, and a drag of a point sets the band as one undo
  step: the geometry in `EqLayout.swift` and its tests, the tap in the
  engine's renderer test, the spectrum call in the FFI's test.

## Open questions

- Whether Option-drag up should narrow a band, as built, or widen it.
- Whether the spectrum should be the equalizer's input instead of its output,
  or either on a switch, as EQ Eight offers.
- Whether the resonance of a pass at 36 and 48 dB, on the last section alone,
  is the lift a person expects, or the whole cascade should sharpen.
