# The overdrive (implemented 2026-10-20)

Status: built in the pull request that closed the first of the person's notes
of October 19, 2026, decision D128. Before it, the only distortion was the
saturation, `tanh` or a clip at full scale with a trim and a mix, and nothing
after a curve took the fizz off. The backlog's line asked for a distortion
harder than the saturation's curves with a drive, a tone, an output and a
mix; what was settled in building it is under Design.

## What

An `overdrive` is an effect kind like any other, in a track's, a group's, a
return's, the master's or a Synth patch's chain: the signal driven into a
curve, a lowpass after it, the result trimmed and mixed under the dry signal.

```yaml
tracks:
- id: bass
  effects:
  - {type: overdrive, drive_db: 18, tone_hz: 2500, output_db: -6, mix_percent: 40}
```

| Field | Range | Default | What it does |
|---|---|---|---|
| `drive_db` | 0…60 | 24 | The gain into the curve: how much of the signal is bent |
| `tone_hz` | 200…20000 | 6000 | A lowpass of 12 dB per octave after the curve; lower is warmer, 20000 keeps all of it |
| `output_db` | −24…24 | 0 | The trim after the tone; the level a driven signal comes out near |
| `mix_percent` | 0…100 | 100 | How much of the result is heard under the dry signal |

All four are lane targets, `tone_hz` in the log domain, and glide over 5 ms
when edited while the song plays, the tone jumping to its new value at the
filter's next update instead. It adds no latency. `daw effect add SONG bass
--type overdrive --drive-db 18 --tone-hz 2500` adds one, `daw set` reaches
each field, and in the app Add Effect › overdrive puts a panel in the chain
with a bar for each field.

## Why

The saturation is a color: a little `tanh` or a valve's bend under the
signal. A distortion that a bass, a lead or a drum loop is pushed through
wants a harder curve that still rounds the top, far more drive, and a tone
after it, since a hard curve makes harmonics all the way up and the top of
them is fizz. That is what every overdrive pedal has, and what the person
asked for after using the app.

## Design

- **The curve is the cubic soft clip.** `1.5x − 0.5x³` up to full scale and
  flat from there: it reaches full scale where `tanh` is three quarters of
  the way, and its bend ends in a corner, so it is harder than the
  saturation's `soft` mode, with more of the upper harmonics, and rounder
  than its `hard` mode, which cuts the top. It is symmetric, so there are
  odd harmonics only and no offset to remove. At `drive_db` 0 a signal at
  half scale comes out bent to 0.69 of it.
- **The tone is a second-order lowpass after the curve,** the
  state-variable filter the moving filter and equalizer use, so a lane moves
  it without a burst. It comes after the curve, not before: a lowpass before
  it would change what is bent, and the item asked for a tone on a
  distortion. At 500 Hz it takes a 100 Hz square's fifteenth harmonic down
  by about 19 dB and leaves its third. There is no highpass before the
  curve, which a guitar pedal has to keep its bass tight; a filter in the
  chain before the overdrive does that.
- **The output is where the top lands.** The curve's top is full scale, so a
  driven signal comes out near `output_db`, and the drive is about how much
  is bent, not how loud it gets. With the tone open the signal through the
  filter rings a hundredth of a decibel past the top, and with the tone low
  the filter leaves a square's fundamental, which is louder than the square
  by up to 2 dB (4/π), so a driven signal with a low tone comes out that
  much over `output_db`. A render refuses a sample at full scale, so an
  overdrive at the default output on a signal that reaches the top wants a
  trim of a decibel or more, or a limiter after it.
- **No oversampling,** as the saturation has none. Driven hard high up, the
  curve's harmonics past half the song's rate fold back under it; the tone
  takes most of them off, and whether what is left is heard on real material
  is for the person to say before the clipper's oversampler is given to it,
  which would filter the whole signal rather than what a clip takes off.
- **A kind of its own,** not the saturation grown a tone and more drive: the
  item asked for one, the saturation's modes keep their meaning, and an
  agent reading `daw describe effects` finds a distortion by its name.

## Done when

- The curve rises, is odd, stays under the line and over `tanh` and reaches
  full scale; a driven tone has odd harmonics only with its top at full
  scale; the tone takes the upper harmonics as a second-order lowpass does
  and leaves the lower; the output trims; a mix of 0 is the input bit for
  bit; the device renders alike in every block size and goes on from
  another's state: held by `crates/aaw-dsp/src/overdrive.rs`.
- Through `daw render`, a chain with an overdrive bends, darkens, trims and
  mixes as the arithmetic says, with no latency, a lane moves the drive, and a
  field past its range is refused: held by the Python suite.
- The agent adds and sets one through `daw effect add` and `daw set`, and
  `daw describe effects` says what each field does and when to pick it over
  the saturation and the clipper.
- The app draws its panel from `describe`, with no change of its own: seen
  in a scripted picture, and a scripted drag on the Drive bar changed the
  song.

## Open questions

- Whether the fold-back is heard with the tone open on a hat or a bright
  synth driven hard, and so whether oversampling is worth its latency.
- Whether a highpass before the curve, as a pedal has, is worth a field, or
  a filter before the overdrive in the chain is enough.
- Whether the default tone of 6 kHz is right for a bass, where 2–3 kHz is
  the usual setting, or the default should be lower.
