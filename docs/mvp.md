# Sampler MVP — implemented September 22, 2026; on the Rust engine since October 1, 2026

## Scope and architecture

A Rust core with a JSON-first CLI, a Mac app over the same core, and Python tools
for the sample library and perception. The agent is the composer/operator; the
engine provides deterministic editing, sample retrieval, sequencing, playback,
rendering and technical inspection. There is no embedded autonomous composer.

The MVP was first built in Python and rendered offline only. The plan in
[Rust-Swift-Update.md](Rust-Swift-Update.md) rebuilt the model, the sampler, the
effects and automation in Rust against that engine as the reference, and its
milestone M7 retired the Python engine. The document format, the fingerprints, the
render artifacts and the semantics below did not change.

Rust, in `engine/crates` (see [../engine/README.md](../engine/README.md)):

- `aaw-model`: strict schema v1, exact musical time, stable YAML, validation,
  hashes, the event schedule and the schema `daw describe` prints.
- `aaw-dsp`: bandlimited repitch, lane envelopes, and the filter, EQ,
  compressor/sidechain, limiter, delay and reverb devices with explicit block
  state; see [effects.md](effects.md) and [automation.md](automation.md).
- `aaw-engine`: compiles a song into a program of voices, chains, routing and
  latency-aligning delays, and runs it as one stream for real-time playback and
  for WAV/stem export.
- `aaw-host`: the session host. It holds an open song, applies edits as commands
  with origins, handles and undo, saves after each, and plays the song as edits land.
- `aaw-cli`: the `daw` binary, a thin command shell with machine-readable results
  and errors.
- `aaw-ffi` and `apps/mac`: the Mac app, which embeds the host; see
  [../apps/mac/README.md](../apps/mac/README.md).
- `aaw-py`: the song model for Python, built into the package as `agent_daw.aaw_py`.

Python, in `src/agent_daw`:

- `model.py`: reads, validates and saves songs through `aaw_py`. A song is plain
  data, the full dump of the validated document.
- `analysis.py`: audio-derived pitch, onsets, tempo and loop/one-shot kind; see
  [sample-analysis.md](sample-analysis.md). Cached in the index by `library.py`.
- `library.py`: incremental SQLite filename/folder search, metadata, basic signal
  inspection, audition WAVs and content-addressed project imports.
- `perception.py`: saved-render loudness, spectrum, stereo and energy analysis;
  snapshot-derived musical context, render comparisons and PNG summaries.
- `cli.py`: `samples`, `listen`, `compare` and `check`. `check` is `inspect` with
  measured root notes and automation warnings. Every other command is passed to the
  Rust `daw`, which passes these four back, so there is one command either way. A
  sample import copies the file and adds it to the song with `daw apply`, so a
  running host takes it as an undoable edit.

The sampler preloads and resamples source files. One renderer serves playback, full
mixes, stems and previews: its output does not depend on how the stream is cut into
blocks, so a song played from the start equals its offline render, and its
processing never allocates, so it runs in the audio callback. No second engine exists.

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
`effects`, `sends` and `automation`. A send is `{to, gain_db, pre_fader}`, at most
one per return.

Return: `id` (unique across tracks and returns), `gain_db`, `pan`, `mute`,
`effects` and `automation`. A return sums its sends, runs its effects and joins the stereo master.
Post-fader sends tap after track gain and pan, pre-fader sends after the inserts.
Muted or solo-muted tracks send nothing; returns are never solo-muted. No groups
or return-to-return sends exist yet.

Effects: `filter`, `eq`, `compressor` (optional `sidechain` track), `limiter`,
`delay` and `reverb`, listed in order under `tracks[].effects`, `returns[].effects`
or `master.effects`, each with an optional `id` unique within its chain. Track
inserts come before track gain and pan; return effects precede return gain and pan;
master effects follow `master_gain_db` and precede the end fade. See
[effects.md](effects.md) for parameters and semantics.

Automation: `tracks[].automation`, `returns[].automation` and `master.automation`
list lanes `{param, points}`. `param` is `gain_db`, `pan`, `sends.RETURN.gain_db` or
`effects.REF.FIELD` (master: `gain_db` and effects). Points `{at, value, curve}`
are in time order; `curve` is `linear` or `hold`. A lane overrides the static value
for the whole song and holds its first and last values outside its points. See
[automation.md](automation.md).

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
float64; master gain is `session.master_gain_db` unless a master `gain_db` lane
replaces it. Sidechain keys are the source track after its
inserts and before its gain, pan, mute and solo. Mute takes precedence over solo. Session end is
finite and applies an explicit final fade; tails beyond it are discarded.

The 4× oversampled peak reported by the engine is an estimate, not a certified
true-peak measurement. An independent FFmpeg EBU R128/peak check can supplement these measurements.
Listening determines musical quality; these numerical checks do not.

## Playback and live editing

`daw play PROJECT --from BEAT` plays through the default output from any beat.
Samples already sounding at that position are picked up partway through; effects
start empty when the stream does and then run on, so a reverb rings through a
locate or a stop. `daw stop`, `locate` and `loop` move a running transport.

While a session host runs for a song (the Mac app has it open, or `daw host` or
`daw play` is running), every edit is a command to that host, from the agent or the
person. The host validates it against the whole song, saves `song.yaml`, records it
in one undo history and change log with its origin, and plays it: levels, pans,
sends and the knobs automation can move glide to their new values over 5 ms, and a
change of structure, such as an added effect, fades through a 10 ms dip. Automation
written in the document is never smoothed, and a render reads only the saved values,
so playback from the start and a render are the same audio. With no host, the same
commands edit the file. [../engine/README.md](../engine/README.md) lists the commands
and describes the host; decisions D36 to D38, D41 and D44 record the choices.

## Verification

`uv run pytest -q` drives the built `daw` and the Python tools; `cargo test` in
`engine/` tests each crate, including every device, the scheduler, the host's
commands and playback under allocation checking.

The model is held to what the Python model did when it was retired: for a generated
corpus of valid songs, broken copies and YAML edge cases, `tests/golden_model.json`
pins each document's canonical YAML and fingerprints, or its errors' locations, types
and messages, and the outcomes of generated `fmt` and `apply` edits. Fingerprints of
each earlier schema are pinned too, so reports from every engine still verify. The
published schema is checked against validation field by field. Generated songs in
every sample format are rendered whole, in other block sizes, as sections and as
single channels, and the results are held to each other.

Tests cover timing/fractions, schema errors, references, pitch, source-rate conversion,
choke groups, swing, gate release, block-size invariance, stems reconstruction,
clipping rejection, copied-asset portability/tamper detection, incremental index
updates, stable formatting, stale-edit rejection, section equivalence and track previews.
Effect tests cover filter and EQ responses, compressor curves, limiter ceilings and
latency, delay echo timing, reverb decay and seeding, block-partition invariance,
sidechain ducking and preview/stem equivalence. Routing tests cover pre/post-fader
sends, mute/solo, return stems and previews and section tails. Automation tests
cover envelope semantics, constant lanes rendering exactly as static values, sweeps and ramps,
latency-compensated timing, block-partition invariance and validation.
Perception tests use known tones, gain changes, silence, stereo polarity, changed
frequencies, localized arrangement edits, previews and tampered render artifacts.

## Deliberately deferred

Recording, saturation, groups, synths,
plugin hosting, time stretching, MIDI import/export, modulation (LFOs), tempo
automation, semantic/audio embedding search,
key/chord detection, downbeat and swing detection, sample sustain looping, incremental render caching,
masking diagnosis, reference alignment and autonomous listening/revision. A bounded
perception layer is implemented; see [perception.md](perception.md). Monophonic pitch
and loop tempo measurement is implemented; see [sample-analysis.md](sample-analysis.md). The CLI is ready
for agent-driven iterative use. The milestones of
[Rust-Swift-Update.md](Rust-Swift-Update.md) are done; the app is packaged as a
bundle with `daw` inside it, and has not yet been signed with a Developer ID.
