# Effects and returns — implemented September 22, 2026

Tracks, returns and the master bus take serial insert chains. The effects cover
the most common mix moves in sample-based work: removing low end from melodic
material, ducking a bass under a kick, putting sounds in a shared space with
reverb and tempo-synced echoes, and reaching a safe peak level without turning
the whole mix down. Every parameter is in real units. Effects are deterministic
and run in the same offline renderer as the sampler, so mixes, stems and previews
all use a single implementation.

Run `uv run daw describe effects` for the generated schema, bounds and defaults.

## Authoring

```yaml
tracks:
- id: bass
  pads:
    sub: {sample: sub, mode: gate}
  clips:
  - {pattern: bassline, at: 0, repeats: 4}
  effects:
  - {type: filter, mode: highpass, cutoff_hz: 30, slope_db_per_octave: 24}
  - {type: compressor, threshold_db: -30, ratio: 8, attack_ms: 1, release_ms: 150, sidechain: kick}
- id: keys
  pads:
    chord: {sample: chord}
  effects:
  - {type: filter, mode: highpass, cutoff_hz: 180}
  - type: eq
    bands:
    - {shape: bell, freq_hz: 400, gain_db: -3, q: 1.2}
    - {shape: high_shelf, freq_hz: 8000, gain_db: 2}
  sends:
  - {to: plate, gain_db: -10}
  - {to: echo, gain_db: -16}
returns:
- id: plate
  effects:
  - {type: reverb, decay_seconds: 1.8, predelay_ms: 20, lowcut_hz: 200, damping_hz: 6000}
- id: echo
  effects:
  - {type: delay, time_beats: 3/4, feedback_percent: 35, highcut_hz: 4000, ping_pong: true}
  - {type: compressor, threshold_db: -30, ratio: 4, attack_ms: 1, release_ms: 150, sidechain: kick}
master:
  effects:
  - {type: limiter, ceiling_db: -1}
```

Effects run top to bottom. Track inserts come before the track's `gain_db` and `pan`.
A return's chain processes the sum of its sends, before the return's `gain_db` and
`pan`. The master chain follows `master_gain_db` and precedes the end fade. Any effect
accepts an optional `id`, unique within its chain, which automation lanes can use
to address it. Any effect accepts `bypass: true`. It stays in the document but does not process, so you can
render with and without it and use `daw compare`.

| Effect | Parameters |
|---|---|
| `filter` | `mode` highpass/lowpass, `cutoff_hz` 10–20000, `slope_db_per_octave` 12/24/36/48 |
| `eq` | `bands` (1–16) of `shape` bell/low_shelf/high_shelf, `freq_hz`, `gain_db` ±24, `q` |
| `compressor` | `threshold_db`, `ratio` 1–20, `attack_ms`, `release_ms`, `knee_db`, `makeup_db`, `sidechain` |
| `limiter` | `ceiling_db` −24…−0.1, `release_ms`, `lookahead_ms` 0.5–20 |
| `delay` | `time_beats` (0–16, fractions allowed), `feedback_percent` 0–95, `lowcut_hz`, `highcut_hz`, `ping_pong`, `mix_percent` |
| `reverb` | `decay_seconds` 0.1–12, `predelay_ms` 0–250, `damping_hz` 500–20000, `lowcut_hz` 20–2000, `width_percent`, `mix_percent`, `seed` |

## Semantics

- **filter**: Butterworth. The slope is the filter order times 6 dB per octave.
  There is no resonance control.
- **eq**: RBJ cookbook biquads in series. For `bell`, `q` sets bandwidth; for
  shelves it sets the shelf slope, and 0.71 is maximally flat.
- **compressor**: stereo-linked sample-peak detector with a soft knee of `knee_db`
  centred on the threshold. Required reduction is held and decays with
  `release_ms`, the time for the reduction to fall by a factor of e. A one-pole
  `attack_ms` smooths the onset. `makeup_db` is static.
- **sidechain**: a compressor's `sidechain` names another track. The key is that
  track **after its own inserts and before its gain, pan, mute and solo**. A muted
  or quiet kick still ducks the bass, and changing the kick's fader does not
  change the ducking. Self-sidechains, unknown tracks and cycles are rejected.
  A return's compressor may use a track as its key, for example to duck an echo
  under the kick. Sidechains cannot name a return. Master compressors cannot use
  a sidechain. Tracks render in dependency order, then returns.
- **limiter**: look-ahead brickwall limiter on sample peaks. Required reduction is
  maximized over the look-ahead window, held with the release and averaged over
  the same window, so no output sample exceeds `ceiling_db`. The look-ahead is a
  declared latency that the renderer compensates exactly: material below the
  ceiling passes through bit-identical. The estimated true peak in the report can
  still exceed the ceiling slightly. Leave some margin below 0 dBFS.
- **delay**: feedback delay synced to the session tempo. `time_beats` is the
  spacing of the echoes in quarter-note beats (`3/4` is a dotted eighth) and must
  come to between 1 ms and 10 s at the session tempo. Each repeat is
  `feedback_percent` of the previous one. `lowcut_hz` and `highcut_hz` are
  12 dB/octave Butterworth filters inside the feedback loop, so each repeat is
  filtered again and gets thinner or darker. With `ping_pong`, the input is summed
  to mono, the first echo is on the left and the repeats alternate channels.
  Without it, each channel echoes itself.
- **reverb**: convolution with a synthetic stereo impulse response generated
  from the parameters and `seed`; the same document always produces the same
  tail. The response is Gaussian noise shaped so each frequency decays
  exponentially: `decay_seconds` is the RT60 (time to fall 60 dB) up to
  `damping_hz`, and above it the RT60 falls in proportion to 1/f, so highs die
  first. It has a 3 ms onset, a `lowcut_hz` 12 dB/octave highpass and
  `predelay_ms` of silence before the tail. The tail is cut where the undamped
  decay reaches −90 dB. `width_percent` 0 gives identical channels; 100 gives
  decorrelated ones. The input is summed to mono, so a hard-panned source still
  gets a centred tail. The response is energy-normalized: steady white noise in
  gives a wet signal of the same RMS. Tonal material varies with the tail's
  spectrum. Convolution runs in fixed partitions of 4096–65536 frames; one
  partition is declared latency and compensated exactly.
- **mix_percent**: delay and reverb output `input × (1 − mix) + wet × mix`. The
  default 100 is fully wet, which is what a return needs. When using them as a
  track insert, set it lower. `mix_percent: 0` passes the input through
  bit-identical.

Parameters are static unless an automation lane moves them; see
[automation.md](automation.md) for which ones can be automated.
Delay and reverb tails that run past the session end are cut by the end fade;
leave room after the last note.

## Sends and returns

`returns` are buses. Each has an `id`, `gain_db`, `pan`, `mute` and `effects`.
A track's `sends` lists `{to: RETURN, gain_db, pre_fader}`, at most one per
return. Return IDs share the namespace of track IDs, because both name stems.

- A **post-fader** send (the default) taps the track after its inserts, gain and
  pan, so turning the track down turns its reverb down too. A **pre-fader** send
  (`pre_fader: true`) taps the track after its inserts and before its gain and pan.
- A muted track, or a track silenced because another track is soloed, sends
  nothing, whether the send is pre- or post-fader.
- Returns are never solo-muted: a soloed snare keeps its reverb. A return can be
  muted.
- A return sums its sends, runs its effects, then applies its gain, pan and mute
  and joins the master bus. Returns cannot send to other returns.

## Stems, previews and reports

Stems are each track's or return's audio after its inserts, gain, pan and
mute/solo, master gain and end fade, and **before master effects**. Track stems
are dry: a track's reverb and echoes are on the return stems. Without master
effects, track and return stems together sum to the mix within quantization
tolerance, as before. With a master limiter or compressor they no longer sum to
the mix. `report.json` records `stems_sum_to_mix`, the `stem_policy` and the
`send_policy`.

`render --track ID` renders that track together with its sidechain sources, and
outputs only that track. It omits returns and the master chain, so it matches that
track's stem from a full render. `render --track RETURN` renders the return with
every track that sends to it (and their sidechain sources) and outputs only the
wet return. It matches that return's stem. Section previews render the whole
timeline through returns and the master chain, then slice it. Tails from notes
that started before the section are kept.

Each report entry under `tracks` has a `kind`: `track` or `return`. Tracks list
their `events`; returns list their `senders`. Each entry has an `effects` list and
the report has `master_effects`. Each effect entry gives the effect `type` and
its `latency_frames`. Compressors and limiters add `max_gain_reduction_db`,
`mean_gain_reduction_db` and `fraction_over_1db_reduction` over the rendered
timeline. Bypassed effects show `bypass: true`. These describe what the processor
did, not how it sounds. `daw listen` measures return stems like track stems and
labels them with the same `kind`; `daw compare` diffs them.

## Implementation and verification

Effects live in `src/agent_daw/effects.py` as block-processing devices with
explicit state. Output does not depend on the block size. Filter state carries
across blocks, and the dynamics detector is vectorized as a running maximum.
The limiter's smoothing sums in fixed point, so partitioning cannot change its
rounding. The delay's recursion is computed in chunks no longer than the delay
time, each depending only on earlier output. The reverb buffers input into fixed
partitions aligned to its own stream and uses uniformly partitioned overlap-save
FFT convolution, so the caller's blocks never change its arithmetic. Python with
numpy/scipy is the device language (decision D25).

Tests use generated audio. They cover filter attenuation at each slope, bell and
shelf gains, the compressor's steady-state curve and reduction report,
below-threshold transparency, limiter ceiling and latency compensation,
block-partition invariance, bypass, sidechain ducking, pre-fader keys,
preview/stem equivalence, hot mixes rescued by a master limiter, validation
errors, dependency ordering and round-trip formatting. Delay tests check exact
echo frames and gains at fractional beat times, ping-pong alternation and
darkening repeats. Reverb tests check measured RT60, faster high-frequency
decay, predelay, equality with direct convolution after latency compensation,
seeding, width, energy normalization and the low cut. Routing tests check pre-
and post-fader sends, mute and solo, return stems summing with track stems to
the mix, return previews, section tails, a return ducked by a sidechain and
block-size invariance of whole renders.

On a 3-minute generated project, adding a 1.8 s plate reverb return and a
ping-pong delay return raised render time from 12.7 s to 15.8 s in the Linux
development container.

Not yet implemented: saturation, chorus and other modulation effects, groups,
return-to-return sends, sidechain filtering, RMS detection, true-peak limiting,
loudness-target export and impulse-response samples for the reverb.
