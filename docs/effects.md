# Effects — implemented September 22, 2026

Tracks and the master bus take serial insert chains. The first set of effects covers
the most common mix fixes in sample-based work: removing low end from melodic
material, ducking a bass under a kick, and reaching a safe peak level without
turning the whole mix down. Every parameter is in real units. Effects are
deterministic and run in the same offline renderer as the sampler, so mixes,
stems and previews all use a single implementation.

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
master:
  effects:
  - {type: limiter, ceiling_db: -1}
```

Effects run top to bottom. Track inserts come before the track's `gain_db` and `pan`.
The master chain follows `master_gain_db` and precedes the end fade. Any effect
accepts `bypass: true`. It stays in the document but does not process, so you can
render with and without it and use `daw compare`.

| Effect | Parameters |
|---|---|
| `filter` | `mode` highpass/lowpass, `cutoff_hz` 10–20000, `slope_db_per_octave` 12/24/36/48 |
| `eq` | `bands` (1–16) of `shape` bell/low_shelf/high_shelf, `freq_hz`, `gain_db` ±24, `q` |
| `compressor` | `threshold_db`, `ratio` 1–20, `attack_ms`, `release_ms`, `knee_db`, `makeup_db`, `sidechain` |
| `limiter` | `ceiling_db` −24…−0.1, `release_ms`, `lookahead_ms` 0.5–20 |

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
  Master compressors cannot use a sidechain. Tracks render in dependency order.
- **limiter**: look-ahead brickwall limiter on sample peaks. Required reduction is
  maximized over the look-ahead window, held with the release and averaged over
  the same window, so no output sample exceeds `ceiling_db`. The look-ahead is a
  declared latency that the renderer compensates exactly: material below the
  ceiling passes through bit-identical. The estimated true peak in the report can
  still exceed the ceiling slightly. Leave some margin below 0 dBFS.

Effect parameters are static for the whole render; automation is not implemented.

## Stems, previews and reports

Stems are each track's audio after its inserts, gain, pan, mute/solo, master
gain and end fade, and **before master effects**. Without master effects the stems
sum to the mix within quantization tolerance, as before. With a master limiter or
compressor they no longer sum to the mix. `report.json` records
`stems_sum_to_mix` and the `stem_policy`.

`render --track ID` renders that track together with its sidechain sources, and
outputs only that track. It omits the master chain, so it matches that track's
stem from a full render. Section previews render the whole timeline through the
master chain, then slice it.

Each track report has an `effects` list and the report has `master_effects`.
Each entry gives the effect `type` and its `latency_frames`. Compressors and
limiters add `max_gain_reduction_db`, `mean_gain_reduction_db` and
`fraction_over_1db_reduction` over the rendered timeline. Bypassed effects show
`bypass: true`. These describe what the processor did, not how it sounds.

## Implementation and verification

Effects live in `src/agent_daw/effects.py` as block-processing devices with
explicit state. Output does not depend on the block size. Filter state carries
across blocks, and the dynamics detector is vectorized as a running maximum.
The limiter's smoothing sums in fixed point, so partitioning cannot change its
rounding. Python with numpy/scipy is the device language (decision D25).

Tests use generated audio. They cover filter attenuation at each slope, bell and
shelf gains, the compressor's steady-state curve and reduction report, below-threshold
transparency, limiter ceiling and latency compensation, block-partition
invariance, bypass, sidechain ducking, pre-fader keys, preview/stem equivalence,
hot mixes rescued by a master limiter, validation errors, dependency ordering and
round-trip formatting.

Not yet implemented: reverb, delay, saturation, sends/returns, groups, automation,
sidechain filtering, RMS detection, true-peak limiting and loudness-target export.
