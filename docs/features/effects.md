# Effects and returns — implemented September 22, 2026

Tracks, returns and the master bus take serial insert chains. The effects cover
the most common mix moves in sample-based work: removing low end from melodic
material, ducking a bass under a kick, putting sounds in a shared space with
reverb and tempo-synced echoes, and reaching a safe peak level without turning
the whole mix down. Every parameter is in real units. Effects are deterministic
and run in the one engine that plays and renders, so playback, mixes, stems and
previews all use a single implementation.

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
| `eq` | `bands` (1–16) of `shape` bell/low_shelf/high_shelf/highpass/lowpass, `freq_hz`, `gain_db` ±24 (0 unless given), `q`, `slope_db_per_octave` 12/24/36/48 for a pass ([parametric-eq.md](parametric-eq.md)) |
| `compressor` | `threshold_db`, `ratio` 1–20, `attack_ms`, `release_ms`, `knee_db`, `makeup_db`, `sidechain` |
| `limiter` | `ceiling_db` −24…−0.1, `release_ms`, `lookahead_ms` 0.5–20 |
| `delay` | `time_beats` (0–16, fractions allowed), `feedback_percent` 0–95, `lowcut_hz`, `highcut_hz`, `ping_pong`, `mix_percent` |
| `reverb` | `decay_seconds` 0.1–12, `predelay_ms` 0–250, `damping_hz` 500–20000, `lowcut_hz` 20–2000, `width_percent`, `mix_percent`, `seed` |
| `chorus` | `rate_hz` 0.05–10, `depth_ms` 0–20, `delay_ms` 1–40, `mix_percent` (50 unless given) |
| `saturation` | `mode` soft/hard/tube, `drive_db` 0–36, `output_db` ±24, `mix_percent` |
| `utility` | `gain_db` −96…24, `pan` ±1, `width_percent` 0–400, `mono`, `mono_below_hz` 20–1000 or left out, `invert` none/left/right/both ([utility.md](utility.md)) |
| `analyzer` | No fields: changes nothing, and the app shows the sound through it ([analyzer.md](analyzer.md)); not inside a Synth patch |

## Semantics

- **filter**: Butterworth. The slope is the filter order times 6 dB per octave.
  There is no resonance control.
- **eq**: the parametric EQ, RBJ cookbook biquads in series. For `bell`, `q`
  sets bandwidth; for shelves it sets the shelf slope, and 0.71 is maximally
  flat. A `highpass` or `lowpass` band, added October 7, 2026, cuts past
  `freq_hz` at `slope_db_per_octave`, the Butterworth sections of that order
  with `q` the resonance at the corner, 0.71 flat; it ignores `gain_db`, as a
  bell or a shelf ignores the slope. The app draws the bands as one curve over
  the spectrum of the equalizer's output, each a point dragged; see
  [parametric-eq.md](parametric-eq.md).
- **compressor**: stereo-linked sample-peak detector with a soft knee of `knee_db`
  centred on the threshold. Required reduction is held and decays with
  `release_ms`, the time for the reduction to fall by a factor of e. A one-pole
  `attack_ms` smooths the onset. `makeup_db` is gain added after reduction;
  `threshold_db` and `makeup_db` can be automated (see [automation.md](automation.md)).
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
  tail. The response is seeded Gaussian noise shaped so each frequency decays
  exponentially: `decay_seconds` is the RT60 (time to fall 60 dB) up to
  `damping_hz`, and above it the RT60 falls in proportion to 1/f, so highs die
  first. It has a 3 ms onset, a `lowcut_hz` 12 dB/octave highpass and
  `predelay_ms` of silence before the tail. The tail is cut where the undamped
  decay reaches −90 dB. `width_percent` 0 gives identical channels; 100 gives
  decorrelated ones. The input is summed to mono, so a hard-panned source still
  gets a centred tail. The response is energy-normalized: steady white noise in
  gives a wet signal of the same RMS. Tonal material varies with the tail's
  spectrum. The reverb adds no latency. Its noise comes from the Rust engine's
  own generator, so a tail has the decay, spectrum and energy of the same reverb
  in a render made by the earlier Python engine, but not its samples.
- **chorus**: a stereo chorus, added October 4, 2026 with the Synth's last
  item. Each channel goes through a delay of `delay_ms` moved `depth_ms` either
  side by a sine at `rate_hz`, the right channel a quarter cycle behind the
  left, so the two sides drift apart; the line is read with linear
  interpolation. `mix_percent` 100 is the wet signal alone, a vibrato. The
  sweep is a function of the frames processed, so a render is the same bytes
  twice, and it goes on while the transport stands. Adds no latency.
- **saturation**: the signal driven by `drive_db` into a curve, added the same
  day. `soft` is `tanh`, odd harmonics that thicken as the drive rises; `hard`
  clips at full scale; `tube` is `tanh(x + 0.3x²)`, asymmetric, even harmonics
  too, with its offset removed by a one-pole highpass at 10 Hz. `output_db`
  trims the result and `mix_percent` blends it under the dry signal. No
  oversampling: a hard clip high up aliases a little.
- **utility**: the channel moves that need no other device, added October 7,
  2026, in this order: `invert` flips the polarity of the left, the right or
  both channels; `mono` sums the channels to their average; `mono_below_hz`
  sums only what lies below it, through a fourth-order Linkwitz-Riley crossover
  whose halves meet flat; `width_percent` scales the side signal, 0 mono and
  100 as it came; `gain_db`; and `pan`, a balance as a stereo track's. At its
  defaults it passes the signal through bit-identical. See
  [utility.md](utility.md).
- **analyzer**: an effect that changes nothing, added October 7, 2026: its
  output is its input bit for bit, with no latency, and the audio thread
  writes every frame of it into a ring the app's analyzer window reads for
  the levels, the loudness, the spectrum and the stereo field. Bypassed, it
  measures nothing. Not offered inside a Synth's patch. See
  [analyzer.md](analyzer.md).
- **mix_percent**: delay, reverb, chorus and saturation output `input × (1 −
  mix) + wet × mix`. The default is 100, fully wet, which is what a return
  needs, and 50 for the chorus. When using a delay or reverb as a track insert,
  set it lower. `mix_percent: 0` passes the input through bit-identical.

Parameters are static unless an automation lane moves them; see
[automation.md](automation.md) for which ones can be automated.
Delay and reverb tails that run past the session end are cut by the end fade;
leave room after the last note.

A Synth patch carries a chain of these kinds of its own,
`tracks[].instrument.synth.effects`, run on the sum of its voices before the
track's inserts and edited by the same commands on the synth's path; a
compressor there has no sidechain. See [synth.md](synth.md).

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

Effects live in `engine/crates/aaw-dsp` as block-processing devices with explicit
state, and `aaw-engine` runs each chain on the timeline. Output does not depend
on the block size, and processing never allocates, so the same devices run in the
audio callback and in a render. Filters are Butterworth sections designed as
`scipy.signal.butter` designs them and EQ bands are RBJ biquads, a pass band
the Butterworth sections of its slope with its q on the last; their state
carries across blocks. An equalizer on a track, a return or the master also
writes what it puts out, as mono, into a ring of 4096 frames the app reads for
the spectrum under its curve, one atomic store a frame. The compressor's and limiter's release hold is a recurrence
on the required reduction, and the limiter's smoothing sums in fixed point, so
partitioning cannot change its rounding. The delay reads a line as long as its
time and feeds back through its filters. The reverb's convolution is
non-uniformly partitioned: the first 64 taps run directly and the rest in
overlap-save FFT blocks of 64 to 16384 frames, each covering the response from at
least its own length in, with the larger blocks' work spread over the block that
follows; nothing waits for a block, so there is no latency, and the reverb counts
its own input, so the caller's blocks never change its arithmetic.

Only the limiter has latency. The engine aligns it by delay: tracks are delayed
to the slowest before their faders and sends, a track keyed by a source with
latency renders its voices that much later, and a render runs that many frames
longer and places every stem on the timeline.

The devices were ported from the Python engine operation for operation and held
to it until it was retired (decisions D34, D40 and D44): over generated songs
with effects, returns and automation the largest difference in any mix or stem
was 4e-9, and reverb tails agreed in decay time, octave levels and energy.

Tests use generated audio. In Rust they cover each device: filter designs
against scipy's, the cookbook bands, the compressor's curve, a keyed compressor,
the limiter's ceiling and look-ahead, exact echo frames and feedback gains,
ping-pong and darkening repeats, the reverb's decay, damping, predelay, energy,
seeding, width, low cut, equality with direct convolution and steady-noise level,
and block-partition invariance of each. Through `daw render` the Python suite
covers filter attenuation at each slope, bell and shelf gains, the compressor's
steady-state curve and reduction report, below-threshold transparency, limiter
ceiling and latency compensation, bypass, sidechain ducking, pre-fader keys,
preview/stem equivalence, hot mixes rescued by a master limiter, validation
errors and round-trip formatting; pre- and post-fader sends, mute and solo,
return stems summing with track stems to the mix, return previews, section
tails, a return ducked by a sidechain and block-size invariance of whole renders.

A 179-second local song with 12 tracks, 3 returns, 32 effects and 10 lanes
renders in about 9 seconds on an M2, where the Python engine took over 30, and
plays at 128-frame buffers using about 0.1 ms of each 2.67 ms callback.

Not yet implemented: a clipper, a phaser and other modulation effects, groups,
return-to-return sends, sidechain filtering, RMS detection, true-peak limiting,
loudness-target export, impulse-response samples for the reverb, and a notch or
a mid/side mode in the equalizer.
