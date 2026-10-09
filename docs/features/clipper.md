# The clipper (implemented 2026-10-09)

Status: built in the pull request that closed the first Next item after the
listening markers, decision D106. Before it, the only device that held a level
was the limiter, which turns the whole signal down around a peak, and the only
clip was the saturation's `hard` mode, fixed at full scale, with no ceiling in
decibels and no report of what it took. There was no design file; what was
settled in building it is under Design.

## What

A `clipper` is an effect kind like any other, in a track's, a group's, a
return's, the master's or a Synth patch's chain: the signal driven into a
ceiling that no sample passes.

```yaml
groups:
- id: kit
  effects:
  - {type: clipper, id: shave, ceiling_db: -8, knee_db: 2}
master:
  effects:
  - {type: clipper, ceiling_db: 1}
  - {type: limiter, ceiling_db: -1}
```

| Field | Range | Default | What it does |
|---|---|---|---|
| `ceiling_db` | −60…24 | −1 | The level no output sample exceeds |
| `drive_db` | 0…36 | 0 | A gain into the ceiling: more of the signal is cut and the result is louder |
| `knee_db` | 0…24 | 0 | How far under the ceiling the curve starts to bend; 0 is a hard clip |
| `oversample` | 1, 2 or 4 | 4 | How many times the song's rate the curve runs at |

Each channel and each sample is cut by its own level. There is no look-ahead,
no release and no linking of the channels, so nothing pumps: what is over the
ceiling is gone, and comes back as harmonics. `ceiling_db`, `drive_db` and
`knee_db` are lane targets and glide over 5 ms when edited while the song
plays; `oversample` changes the device's latency, so a playing song fades
through a change of it. `daw effect add SONG master --type clipper --index 0
--ceiling-db 1` adds one before what is there, `daw set` reaches each field,
and in the app Add Effect › clipper puts a panel in the chain with a bar for
each level and a menu for the oversampling.

The render's report and `daw listen` say what it took off, as they do for a
compressor and a limiter, over the song and in each section:
`max_gain_reduction_db` is the most it cut off one sample,
`mean_gain_reduction_db` the mean over every sample, and
`fraction_over_1db_reduction` the share of samples it cut by more than 1 dB.
`daw compare` sets two renders' clippers side by side with the other devices.

## Why

Agents making loud mixes stopped at the limiter. A limiter that has 6 dB of a
kick's first milliseconds to take turns the whole mix down for as long as its
release, and the mix ducks at every hit. Those milliseconds can be cut off
instead: a peak that short loses a few decibels with little sound of it, and
the limiter after it has that much less to do. That is what a clipper is for
on a drum group and on a master before the limiter, and it is how loud
masters are made. The saturation's hard mode clips, but at full scale only,
which no signal inside a mix is set against, and says nothing of what it did.

## Design

- **The ceiling goes over full scale.** Inside a chain a signal passes 0 dBFS
  freely, and a master's limiter is often reached by peaks several decibels
  over it. A clipper is set against the level that reaches it, so its ceiling
  goes to +24 dB, where the limiter's stops at −0.1. As the last device of
  the master it has to be under 0, since a render refuses a sample at full
  scale, and the refusal then says to lower what follows the master clipper.
- **The knee** is one curve: under `ceiling_db − knee_db` the signal is as it
  came, and from there it is `start + room · tanh((level − start) / room)`,
  `room` being what is left to the ceiling. It leaves the line without a
  step, rises all the way and nears the ceiling without passing it. With a
  knee a sample at the ceiling comes out under it: a knee of 6 dB puts it
  1.1 dB down.
- **Oversampling takes the fold-back down, and only what is cut is
  filtered.** A clip makes harmonics past half the song's rate, which fold
  back under it as tones that belong to nothing. At `oversample` 2 or 4 the
  signal is interpolated to that many times the rate, the curve is applied
  there, and what the curve took off, not the signal, is lowpassed and
  brought back to the song's rate and added to the signal. So the signal
  itself goes through no filter: one under the knee comes out bit for bit,
  late by the filters' 16 frames, and one that is clipped keeps everything
  the clip did not touch. The interpolation leaves the samples as they are,
  so the curve sees each sample and three crests between it and the next,
  and a crest that passes the ceiling between two samples under it is cut
  too. The kernel is a Kaiser-windowed sinc that reaches 8 frames either
  side.
- **A last clip at the song's rate holds the ceiling, and it decides how much
  folds back.** The lowpassed signal rings past the ceiling, by 0.1 dB for a
  1 kHz tone 6 dB over it and 1.5 dB for a 9 kHz one, and the item was a
  clipper that holds a ceiling as the limiter does. So what is still over
  the ceiling is cut again at the song's rate, where it folds back as any
  clip does, though it is far less. Measured on tones, as the part of the
  output that is neither the tone nor a harmonic of it under half the rate,
  in dB under the whole:

  | Tone | Over the ceiling | 1× | 2× | 4× |
  |---|---|---|---|---|
  | 1 kHz | 6 dB | −47 | −58 | −59 |
  | 3 kHz | 6 dB | −32 | −41 | −42 |
  | 6 kHz | 3 dB | −29 | −43 | −50 |
  | 6 kHz | 12 dB | −16 | −25 | −25 |
  | 9 kHz | 6 dB | −13 | −21 | −21 |

  Without the last clip 4× measured −33 to −65 on the same tones, and the
  ceiling was passed by up to 1.5 dB. Running the oversampled stage twice or
  three times before the last clip gained about 5 dB a pass, at the price of
  its latency again each time; a kernel four times as long gained about
  1 dB; 8× gained about 1 dB over 4×. None of them is built: one pass, at 1,
  2 or 4.
- **4× is the default.** It costs 16 frames of latency, which the engine
  makes up for as it does the limiter's look-ahead, and about two hundred
  multiplications a frame, over half of them skipped while nothing is being
  cut. `oversample: 1` is the plain clip, with no latency.
- **What it took off is read from the curve,** the level of the frame's
  louder channel against what the curve makes of it, and not from the output,
  where the lowpass moves quiet samples by amounts that are large in
  decibels and mean nothing. So the three numbers are the same at any
  oversampling. They are not a limiter's: a clipper that takes 3 dB off a
  kick's first milliseconds reads a maximum of 3 and a fraction near 0, where
  a limiter doing the same reads its release as well.
- **No output gain, mix or channel link.** A level after it is a utility's,
  a dry signal mixed in would pass the ceiling, and a link would turn the
  quiet channel down, which is what a limiter does.

## Done when

- A hard clip holds its ceiling exactly and leaves every sample under it as
  it came; a knee follows its curve; oversampled, the ceiling is held, a
  signal under it comes out bit for bit and 16 frames late, less folds back
  than at the song's rate, and a crest between the samples is cut; in every
  block size, and going on from another device's state: held by
  `crates/aaw-dsp/src/clipper.rs`.
- Through `daw render`, a chain with a clipper renders as the arithmetic
  says with its latency made up for, a lane moves its ceiling, a master
  clipper lets a hot mix render, and the report and `daw listen` say what it
  took off in each section: held by the Python suite.
- The agent adds and sets one through `daw effect add` and `daw set`, and
  `daw describe effects` says what each field does and where to set the
  ceiling.
- The app draws its panel from `describe`: seen in a scripted picture, and a
  scripted drag on the ceiling's bar changed the song.

## Open questions

- Whether the last clip's fold-back is heard on real material, where what is
  cut is mostly a drum's first milliseconds and not a tone, and whether a
  switch to leave it out, before a limiter that holds the ceiling anyway, is
  worth a field.
- Whether a clipper wants a reading in its panel of what it is taking off,
  as the equalizer has its spectrum: today the number is in a render's
  report only.
- Whether the default ceiling of −1, the limiter's, is of any use on a
  group, where the level that reaches it is whatever the tracks make.
