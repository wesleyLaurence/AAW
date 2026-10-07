# The utility (implemented 2026-10-07)

Status: built in the pull request that closed the first of the person's notes
of October 6, 2026 that remained, decision D90. Before it, a track's level and
pan were the fader's alone, a chain had no place to turn a signal down between
two effects, and nothing summed a bass to mono or flipped a channel's polarity.

## What

A `utility` is an effect kind like any other, in a track's, a return's, the
master's or a Synth patch's chain, as Ableton's Utility is: the small channel
moves that need no device of their own.

```yaml
effects:
  - {type: utility, id: trim, gain_db: -3}
  - {type: utility, mono_below_hz: 120, width_percent: 130}
  - {type: utility, invert: left}
```

| Field | Range | Default | What it does |
|---|---|---|---|
| `gain_db` | −96…24 | 0 | A level, as a second fader anywhere in the chain |
| `pan` | −1…1 | 0 | A balance: the louder side stays at full level and the other is turned down, as a stereo track's pan does |
| `width_percent` | 0…400 | 100 | Scales the side signal: 0 is mono, 100 leaves the signal as it came, 200 doubles the difference between the channels |
| `mono` | true/false | false | Sums the channels to their average |
| `mono_below_hz` | 20…1000, or left out | left out | Sums only what lies below the frequency, so a bass sits in the middle while the rest keeps its width |
| `invert` | `none`, `left`, `right`, `both` | `none` | Flips the polarity of the channel named, or of both |

They apply in this order: invert, mono, mono below, width, gain, pan. `gain_db`,
`pan` and `width_percent` are lane targets, as `effects.REF.gain_db` and so on,
and glide over 5 ms when edited while the song plays; `mono`, `mono_below_hz` and
`invert` are a jump, so a playing song fades through a change of them as it does
through an added effect. `daw effect add SONG tracks.T utility --gain-db -3`
adds one, `daw set` reaches each field, and in the app Add Effect › utility puts
a panel with a bar or a switch for each field in the chain.

## Why

The person's note of October 6, 2026 asked for a utility: gain, pan, width,
mono, mono below a frequency and phase, as Ableton's Utility. A mix is made of
such moves more than of any effect: a sample trimmed 3 dB before a compressor so
the threshold is right, a wide pad narrowed under a vocal, a stereo bass sample
whose low end should sit in the middle, a layered kick whose sample is the wrong
way up. Each had a way before, but a poor one: a pad's or a track's gain for the
level, which is after the chain; nothing at all for width, mono or polarity.

## Design

- **Mono below a frequency** is a Linkwitz-Riley crossover of fourth order, 24 dB
  per octave: each channel's highs plus the lows of the middle. The lowpass and
  the highpass of such a pair sum to an allpass, so a centred signal comes
  through at its level at every frequency, the crossover included, and a signal
  above the frequency keeps its exact stereo image, since the same allpass is
  applied to both channels. A highpass on the side signal alone was tried first
  and rejected: it shifts the side's phase against the middle, so a sound on one
  channel leaks into the other above the cutoff.
- **Width** is the side signal scaled, a mid/side matrix. Above 100 it adds
  difference the signal did not have, and can add level; the signal is not
  limited.
- **Pan** is a balance rather than an equal-power pan, as the track's pan is for
  stereo audio, so a utility's pan and the fader's pan read the same.
- **Bit-identical at rest.** Each field is left out of the arithmetic at its
  default, so a utility with nothing set passes the signal through unchanged,
  as `bypass` or `mix_percent: 0` do for the others.
- **No DC filter, mute or channel swap**, which Ableton's Utility has. A mute is
  the track's, a swap has had no use yet, and the saturation's tube mode removes
  its own offset. Any of them is a line in the backlog when it is missed.

## Done when

- A utility of each field renders as the arithmetic says, in every block size
  and with its knobs moving: held by `crates/aaw-dsp/src/utility.rs` and the
  effects contract test, and through `daw render` by the Python suite.
- The agent adds and sets one through `daw effect add` and `daw set`, and
  `daw describe effects` says what each field does.
- The app draws its panel from `describe`, with a bar for each number, a switch
  for `mono`, a menu for `invert` and the optional frequency as the delay's cuts
  are drawn.

## Open questions

- Whether `width_percent` above 100 wants a limit on the level it can add, or a
  note in `daw check`.
- Whether the crossover's allpass phase shift around the frequency is heard on a
  kick with a long tail, and whether a lower order would be preferred.
