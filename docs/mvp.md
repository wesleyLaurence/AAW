# Sampler MVP — implemented September 22, 2026

## Scope and architecture

A Python core with a JSON-first CLI. The agent is the composer/operator; the engine
provides deterministic editing, sample retrieval, sequencing, rendering and technical
inspection. There is no embedded autonomous composer or visual editor.

- `model.py`: strict schema v1, exact musical time, stable YAML, validation and hashes.
- `analysis.py`: audio-derived pitch, onsets, tempo and loop/one-shot kind; see
  [sample-analysis.md](sample-analysis.md). Cached in the index by `library.py`.
- `library.py`: incremental SQLite filename/folder search, metadata, basic signal
  inspection, audition WAVs and content-addressed project imports.
- `engine.py`: event scheduling, sample decoding and bandlimited repitch, explicit
  block-processing voice state, sidechain-ordered track rendering, master gain and
  WAV/stem export.
- `effects.py`: filter, EQ, compressor/sidechain and limiter devices with explicit
  block state and compensated latency; see [effects.md](effects.md).
- `cli.py`: thin command shell with machine-readable results and errors.
- `perception.py`: saved-render loudness, spectrum, stereo and energy analysis;
  snapshot-derived musical context, render comparisons and PNG summaries.

The sampler preloads/resamples source files and mixes voices in blocks. This is an
offline implementation, not a realtime-safe callback. It intentionally uses the same
sampler for full mixes, stems and previews. No second playback engine exists.

## Format v1

Required top-level fields: `session`. Optional `samples`, `patterns`, `tracks`,
`sections` and `master`; `schema_version` is 1. Unknown fields are rejected. Run `daw describe`
for the exact generated JSON schema, bounds and defaults.

Session: `title`, `tempo`, `time_signature` (4/4), `sample_rate` (44100 or 48000),
`length_beats`, `master_gain_db`, `end_fade_ms`.

Sample: relative `path`, optional `sha256`, original `source`, optional `root_note`
with octave. Selected assets are copied into `samples/HASH_original-name.wav`.
Supported library formats: WAV, AIFF and FLAC. Mono and stereo rendering only.

Track: unique `id`, `gain_db`, `pan` (-1…1), `mute`, `solo`, named `pads`, `clips`,
and `effects`. The output is the stereo master; no groups or returns exist yet.

Effects: `filter`, `eq`, `compressor` (optional `sidechain` track) and `limiter`,
listed in order under `tracks[].effects` or `master.effects`. Track inserts come
before track gain and pan; master effects follow `master_gain_db` and precede the
end fade. See [effects.md](effects.md) for parameters and semantics.

Pad: `sample`, `mode` (`one_shot` or `gate`), `gain_db`, `pan`, `transpose` in
semitones, `start_seconds`, `end_seconds`, `attack_ms`, `release_ms`, optional
`choke_group`, `reverse`, optional `source_bpm`, and `mono` for downmixing.

Pattern: positive `length_beats`, `grid` in beats per step, `steps` mapping pad IDs
to strings, optional explicit `events`, `swing` (0.5…0.75). Step rows and explicit
events are additive. Swing delays odd-numbered step cells only; explicit events
retain their exact times. Explicit events: `at`, `pad`, `velocity` (1…127), optional
`note`, `duration`, `transpose`. Gate mode requires a positive duration.

Clip: `pattern`, `at`, positive integer `repeats`, `velocity_scale`. Repetitions
use the pattern's declared length. Clips must fit the session, but sample tails may
cross pattern boundaries. Sections have unique `id`, `at`, `length_beats`.

## Timing and audio semantics

All `at`, `duration` and `length_beats` values are quarter-note beats. Position zero
is the first downbeat. Strings such as `1/3` represent exact rational beats. Decimal
values are converted through their string representation. The scheduler computes
each absolute frame independently using round-half-up; it never accumulates rounded
step intervals. This differs from the older draft's bar.beat.fraction notation.

Velocity maps linearly to sample amplitude. A target note is relative to the sample's
explicit root note. Transposition and source-tempo matching use bandlimited repitch:
changing pitch changes playback duration. There is no pitch-preserving stretching,
ADSR sustain loop, glide or automatic root/tempo detection.

Gate note-off starts a linear release; natural sample end also fades. A note cannot
outlast its source. Choke groups are scoped to each track and release the previous
voice; use a common group for hats or monophonic bass. Very short releases can
still click on low-frequency material; longer releases may be needed on bass.

Mono pad pan uses equal-power gains; stereo pad and track pan use balance, preserving
center stereo levels. Mono conversion averages source channels. Tracks sum in
float64; master gain is static. Sidechain keys are the source track after its
inserts and before its gain, pan, mute and solo. Mute takes precedence over solo. Session end is
finite and applies an explicit final fade; tails beyond it are discarded.

The 4× oversampled peak reported by the engine is an estimate, not a certified
true-peak measurement. An independent FFmpeg EBU R128/peak check can supplement these measurements.
Listening determines musical quality; these numerical checks do not.

## Verification

Tests cover timing/fractions, schema errors, references, pitch, source-rate conversion,
choke groups, swing, gate release, block-size invariance, stems reconstruction,
clipping rejection, copied-asset portability/tamper detection, incremental index
updates, stable formatting, stale-edit rejection, section equivalence and track previews.
Effect tests cover filter and EQ responses, compressor curves, limiter ceilings and
latency, block-partition invariance, sidechain ducking and preview/stem equivalence.
Perception tests use known tones, gain changes, silence, stereo polarity, changed
frequencies, localized arrangement edits, previews and tampered render artifacts.

## Deliberately deferred

Realtime playback, recording, UI, reverb/delay/saturation, sends, groups, synths,
plugin hosting, time stretching, MIDI import/export, automation, semantic/audio embedding search,
key/chord detection, downbeat and swing detection, sample sustain looping, incremental render caching,
masking diagnosis, reference alignment and autonomous listening/revision. A bounded
perception layer is implemented; see [perception.md](perception.md). Monophonic pitch
and loop tempo measurement is implemented; see [sample-analysis.md](sample-analysis.md). The CLI is ready
for agent-driven iterative use.
