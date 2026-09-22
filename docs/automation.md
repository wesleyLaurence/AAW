# Automation — implemented September 22, 2026

Automation lanes change mix and effect parameters over time: a filter that opens
into a drop, a rhythm part turned down 1.5 dB for the second chorus, a delay
throw on one phrase, a reverb that swells at the end of a section. A lane is part
of the project document, so it can be inspected, edited, diffed and rendered like
everything else. Values use the parameter's own units. Positions are beats, like
every other position.

Run `uv run daw describe automation` for the generated schema, the parameters that
can be automated and the semantics below.

## Authoring

```yaml
tracks:
- id: chords
  gain_db: -4
  pads:
    stab: {sample: stab}
  clips:
  - {pattern: stabs, at: 0, repeats: 8}
  effects:
  - {type: filter, id: sweep, mode: lowpass, cutoff_hz: 18000, slope_db_per_octave: 24}
  sends:
  - {to: echo, gain_db: -96}
  automation:
  - param: effects.sweep.cutoff_hz
    points:
    - {at: 48, value: 300}
    - {at: 64, value: 18000}
  - param: gain_db
    points:
    - {at: 64, value: -4, curve: hold}
    - {at: 96, value: -5.5, curve: hold}
    - {at: 112, value: -4}
  - param: sends.echo.gain_db
    points:
    - {at: 79, value: -96}
    - {at: 79, value: -10, curve: hold}
    - {at: 80, value: -96}
returns:
- id: echo
  effects:
  - {type: delay, time_beats: 3/4, feedback_percent: 45}
master:
  automation:
  - param: gain_db
    points:
    - {at: 120, value: -6}
    - {at: 128, value: -30}
```

`tracks[].automation`, `returns[].automation` and `master.automation` are lists of
lanes. Each lane has a `param` and at least one point. A point is
`{at, value, curve}` and points are listed in time order.

## What can be automated

| Owner | `param` |
|---|---|
| track | `gain_db`, `pan`, `sends.RETURN.gain_db`, `effects.REF.FIELD` |
| return | `gain_db`, `pan`, `effects.REF.FIELD` |
| master | `gain_db` (replaces `session.master_gain_db`), `effects.REF.FIELD` |

`REF` is an effect's `id` or its zero-based index in the chain. Any effect accepts
an optional `id`, unique within its chain. Prefer ids: inserting an effect earlier
in the chain shifts indexes but not ids. EQ fields are addressed per band, e.g.
`effects.tone.bands.1.gain_db`.

| Effect | Automatable fields | Interpolation |
|---|---|---|
| `filter` | `cutoff_hz` | log |
| `eq` | `bands.N.freq_hz`, `bands.N.q` / `bands.N.gain_db` | log / linear |
| `compressor` | `threshold_db`, `makeup_db` | linear |
| `delay` | `feedback_percent`, `mix_percent` | linear |
| `reverb` | `mix_percent` | linear |

Everything else is rejected: switches such as `mode`, `slope_db_per_octave`,
`ping_pong` and `bypass`, and parameters whose change would rebuild a device's
state, such as `time_beats`, `decay_seconds`, `lookahead_ms` and the limiter's
ceiling. Values must lie within the parameter's normal bounds.

## Semantics

- **Override**: a lane replaces the parameter's static value for the whole song.
  Before its first point the lane holds the first value; after its last point it
  holds the last value. A one-point lane is a constant. There is one lane per
  parameter; an id and an index naming the same effect count as the same lane.
- **Curves**: a point's `curve` shapes the segment after it. `linear` (the
  default) moves at a constant rate in the parameter's domain. dB, pan and
  percent move linearly. Frequencies and `q` move in equal ratios per beat, so a
  sweep from 200 Hz to 12.8 kHz over six beats rises one octave per beat. `hold`
  keeps the value until the next point.
- **Jumps**: two points at the same `at` jump from the first value to the second
  at that position. At most two points may share a position. The example's send
  lane opens the echo for one beat, from beat 79 to 80.
- **Positions**: points may lie anywhere from 0 to the session length.
- **Timing**: values are evaluated at every audio frame of the timeline and the
  chain's latency compensation includes them, so a change at beat 16 lands on
  beat 16 even after a look-ahead limiter or a reverb. `gain_db`, `pan`, send
  levels, compressor thresholds and makeup, delay feedback and mix, and reverb mix
  change every frame. Filter and EQ coefficients are recomputed every 64 frames,
  counted from the start of the song, from the value at the first frame of each
  period, and the filter state carries across updates.
- **No smoothing**: a `hold` step or jump on a level changes it within one frame.
  On sustained material that can click; ramp over a few milliseconds instead
  (1/64 beat is 3.9 ms at 240 BPM and 7.8 ms at 120 BPM).
- **Fades**: `gain_db` moves linearly in dB, so the audible part of a fade to
  −96 dB is over early. Fading to about −40 to −60 dB and letting the session's
  end fade finish usually sounds more even.
- **Sidechains**: a sidechain key is the source track after its inserts and
  before its fader. Automating the source's `gain_db`, `pan` or sends does not
  change ducking; automating its effects does.
- **Master gain**: a master `gain_db` lane replaces `session.master_gain_db` and
  scales stems as well as the mix. Stems still sum to the mix when there are no
  master effects.
- **Bypass**: a lane on a bypassed effect does nothing. `daw check` warns about
  it.

## Stems, previews and reports

Stems, track and return previews and section previews include automation exactly
as the full mix does: every lane is evaluated on the whole timeline before the
section is sliced. `report.json` lists each track's and return's lane params
under `tracks.ID.automation`, the master's under `master_automation`, and each
automated effect lists its fields under `automated`. `daw inspect` lists lane
params per track and return and under `master_automation`. The report's
`automation_policy` summarizes the semantics.

## Implementation and verification

Lanes are validated in `model.py`: the target must exist and be automatable,
values must be in bounds, points in order and inside the session.
`automation.py` evaluates a lane at any set of timeline frames with
`numpy.searchsorted`. The renderer builds per-frame arrays for automated channel
gain, pan and send levels. Effects receive their envelopes and each device reads
the value for the frames it is processing, offset by the latency of the devices
before it. Automated filters and EQs design their biquads for every 64-frame
period in one vectorized step and run each group of equal coefficients through
`sosfilt`. Filters use RBJ Butterworth sections whose response matches the static
filter's. Unautomated effects run the same code as before, so projects without
automation render bit-identically.

Tests use generated audio. They cover envelope holds, interpolation, `hold`,
jumps and log-domain ratios. They check that constant lanes match static filters,
EQs and gains. They also check filter sweeps and EQ ramps, and block-partition
invariance of a chain where every automatable device is automated. Further tests
cover changes landing on the timeline after latency compensation, track gain
ramps measured in stems, pan, send and return lanes, and master gain in stems
and mix. The rest cover section previews, block-size invariance of whole
renders, validation errors, id and index collisions, round-trip formatting,
`inspect`, `check` and `describe`.

Each automated filter or EQ costs about one second per minute of continuous
change in the Linux development container. Constant stretches of a lane cost
nothing extra.

Not yet implemented: smoothing options and curved segments beyond linear and
hold. Also missing are LFOs and other modulation, automation of switches and
device-rebuilding parameters, automation of pad or sampler parameters, and tempo
automation.
