# Architecture: how AAW is built

This describes what exists, in the present tense, and is changed in the same pull
request as the code. What the app should become is in [concept.md](concept.md);
what is not built yet is in [backlog.md](backlog.md). Three things beside the code
are more exact than this page and are where the detail lives:

| For | Read |
|---|---|
| The song's schema, every field's bounds and defaults, and how to author a song | `daw describe` and its topics, printed from the code |
| The engine, the session host and every command | [../engine/README.md](../engine/README.md) |
| The Mac app: its window, its input, its bundle and its limits | [../apps/mac/README.md](../apps/mac/README.md) |
| One feature | Its file in [features/](features/) |

## The parts

A Rust core with a JSON-first CLI, a Mac app over the same core, and Python tools
for the sample library and perception. The agent is the composer/operator; the
engine provides deterministic editing, sample retrieval, sequencing, playback,
rendering and technical inspection. There is no embedded autonomous composer.

```
  Mac app (Swift, SwiftUI + AppKit)            agent in a terminal
  arrangement, mixer, device view,             daw set / move / add / play
  transport, space bar, drag edits                     │
         │ in-process calls (UniFFI)                   │ local socket
         ▼                                             ▼
  ┌──────────────────── session host (Rust) ─────────────────────┐
  │ in-memory song · commands · undo/redo · change log           │
  │ revision + project_sha256 · saves each edit · external edits │
  └───────────┬───────────────────────────────┬─────────────────┘
              │ compiled program              │ canonical YAML
              ▼                               ▼
  ┌──────── engine (Rust) ─────────┐     song.yaml, samples/, renders/
  │ scheduler · sampler · devices  │
  │ automation · routing · mixer   │     Python: samples scan/search/
  │ real-time driver (CoreAudio)   │     analyze/beats/import, reference,
  │ offline driver (WAV, stems)    │     listen, compare, check,
  └────────────────────────────────┘     timeline, joins, export
```

With no app or `daw host` running for a project, `daw` applies a command to the
file itself and exits, under the project's `.daw.lock`. A running host registers
its project and socket under `~/Library/Application Support/AAW/hosts`.

A project is a folder with `song.yaml`, its samples and its renders, and a
command takes the folder or the file. The app opens on an Untitled project, a
folder in its data folder (`~/Library/Application Support/AAW`, or
`AAW_DATA_DIR`). Save As… is a command to the host, which moves an Untitled
project's folder or copies a named one's, carries on there with its history and
what is playing, and keeps answering at the path it had. The app keeps an index
of the projects it knows, `projects.json` in the data folder, which Open Recent
shows and `daw projects --all` reads. See
[new-and-untitled-projects.md](features/new-and-untitled-projects.md).

The first build was in Python and rendered offline only. The Rust rebuild ported
the model, the sampler, the effects and automation against that engine as the
reference and then retired it; the document format, the fingerprints and the
render artifacts did not change
([archive/Rust-Swift-Update.md](archive/Rust-Swift-Update.md)).

Rust, in `engine/crates` (see [../engine/README.md](../engine/README.md)):

- `aaw-model`: the strict schema (version 1, and 2 with MIDI tracks), exact musical time, stable YAML, validation,
  hashes, the event schedule, the schema `daw describe` prints, and the
  warnings `daw check` gives of a valid song, each with a code (`check`); see
  [musical-checks.md](features/musical-checks.md).
- `aaw-dsp`: bandlimited repitch, lane envelopes, the filter, EQ,
  compressor/sidechain, limiter, delay, reverb, chorus, saturation and utility
  devices with explicit block state, the spectrum tap an equalizer's panel
  draws from, the analyzer's ring and the meter that reads it for its
  levels, loudness, spectrum, stereo field, spectrogram and waveform (`meter`),
  wavetables, and the Synth, a polyphonic
  synthesizer played from a patch; see
  [effects.md](features/effects.md), [automation.md](features/automation.md),
  [analyzer.md](features/analyzer.md) and [synth.md](features/synth.md).
- `aaw-engine`: compiles a song into a program of voices, chains, groups,
  routing and latency-aligning delays, and runs it as one stream for real-time
  playback and for WAV/stem export.
- `aaw-host`: the session host. It holds an open song, applies edits as commands
  with origins, handles and undo, saves after each, and plays the song as edits land.
  It also makes a blank project, moves or copies a project's folder,
  reads and writes Standard MIDI files of one part (`midi_file`), and draws the
  song as a grid of tracks by bars for `daw map` (`map`).
- `aaw-cli`: the `daw` binary, a thin command shell with machine-readable results
  and errors.
- `aaw-ffi` and `apps/mac`: the Mac app, which embeds the host; see
  [../apps/mac/README.md](../apps/mac/README.md).
- `aaw-py`: the song model for Python, built into the package as `agent_daw.aaw_py`.

Python, in `src/agent_daw`:

- `model.py`: reads, validates and saves songs through `aaw_py`. A song is plain
  data, the full dump of the validated document.
- `analysis.py`: audio-derived pitch, onsets, tempo and loop/one-shot kind; see
  [sample-analysis.md](features/sample-analysis.md). Cached in the index by `library.py`.
- `beats.py`: the beat and downbeat map of a whole song, its phrase changes and a
  click audition; see [beat-map.md](features/beat-map.md). Kept beside the audio by
  `library.py`.
- `library.py`: shared folder registration in the app data directory, incremental SQLite filename/folder search, metadata, basic signal
  inspection, audition WAVs and content-addressed project imports.
- `perception.py`: saved-render loudness, spectrum, stereo and energy analysis;
  snapshot-derived musical context, render comparisons and PNG summaries.
- `reference.py`: a song the person names as a reference, measured once with
  its beat map's phrases as sections and kept as numbers in the workspace
  library, `~/Music/AAW/library/references/` (or `AAW_WORKSPACE`), with the
  file's path and hash and no audio; and a render compared with one, whole and
  section by section, with the differences in words; see
  [reference-comparison.md](features/reference-comparison.md).
- `export.py`: a named deliverable from a render, as WAV, AAC or MP3, with one gain
  for the level and a record of the render beside it; see [export.md](features/export.md).
  The Mac app's File › Export Audio… runs it through the bundle's `daw`, so there
  is one export; see [export-in-app.md](features/export-in-app.md).
- `joins.py`: checks of a rendered edit of a song: each join's beat, splice and
  level, the file's length, and an excerpt of each join; see
  [join-checks.md](features/join-checks.md).
- `timeline.py`: a song's places in beats and seconds, where each track's sound is,
  the start of a sound that ends on a beat, and fitting the session's length; see
  [timeline.md](features/timeline.md).
- `cli.py`: `samples`, `reference`, `listen`, `compare`, `check`, `timeline`,
  `joins` and `export`. `check` is `inspect` with measured root notes and the
  model's warnings, as objects with a code.
  Every other command is passed to the Rust `daw`, which passes these eight back, so
  there is one command either way. A
  sample import copies the file, or decodes a compressed one, and adds it to the
  song with `daw apply`, so a running host takes it as an undoable edit.

The sampler preloads and resamples source files. One renderer serves playback, full
mixes, stems and previews: its output does not depend on how the stream is cut into
blocks, so a song played from the start equals its offline render, and its
processing never allocates, so it runs in the audio callback. No second engine exists.

## Format

Required top-level fields: `session`. Optional `samples`, `patterns`, `tracks`,
`groups`, `returns`, `sections` and `master`. `schema_version` is 2 in a song with a MIDI
track and 1 in any other, which saves byte for byte as it did before MIDI
tracks; either is read (D63). Unknown fields are rejected, and an edit that
names one is told the nearest field. Run `daw describe TOPIC` for each field's
path, bounds and default, and `--schema` for the exact generated JSON schema.

Session: `title`, `tempo`, `time_signature` (`N/D`, 1 to 32 beats over 1, 2, 4,
8 or 16, `4/4` by default, one for the whole song; see
[time-signature.md](features/time-signature.md)), `sample_rate` (44100 or 48000),
`length_beats`, `master_gain_db`, `end_fade_ms`, and `stretcher` (`signalsmith` or
`rubberband`; see [time-stretch.md](features/time-stretch.md)).

Sample: relative `path`, optional `sha256`, original `source`, optional
`source_sha256` and optional `root_note` with octave. Selected assets are copied
into `samples/HASH_original-name.wav`. Supported library formats: WAV, AIFF and
FLAC. Mono and stereo rendering only.

An import also takes `.m4a` (AAC or Apple Lossless) and `.mp3` by path. The engine
reads PCM only, so the file is decoded once into the project as 32-bit float WAV at
its own sample rate, named by the original's hash; `sha256` is the decoded file's
and `source_sha256` the original's. Importing the same file again finds the copy.
`afconvert`, which macOS has, decodes it, and `ffmpeg` where that is missing. Both
leave out the padding an encoder puts before the audio, so positions in the copy
are the original's; `ffmpeg` keeps about 15 ms of padding after AAC. A decoded
lossy file can peak a fraction of a decibel above full scale, which float keeps.
Files bought from the iTunes Store are plain AAC and decode; files downloaded
through an Apple Music subscription are copy-protected and cannot be read. The
library index includes compressed files using temporary decoding for metadata; `inspect`, `analyze` and
`audition` read the decoded copy in the project.

An import refuses a file the engine cannot play (unreadable, empty, or more than
two channels), and `daw check` warns of a sample like that in a song.

Track: unique `id`, `gain_db`, `pan` (-1…1), `mute`, `solo`, named `pads`, `clips`,
`audio` (audio clips: parts of a sample file on the timeline, see
[audio-clips.md](features/audio-clips.md); one with `loop_beats` and
`length_beats` plays its first beats again at each wrap until its length, each
repetition a copy with the clip's fades, see
[looping-clips.md](features/looping-clips.md)), `effects`, `sends`, `automation` and an optional `group`, the
group the track's output goes into instead of the master sum. A send is `{to, gain_db, pre_fader}`, at most
one per return.

Group: `id` (unique across tracks, groups and returns), `gain_db`, `pan`, `mute`,
`solo`, `effects`, `sends` and `automation`, as a track has them. A group sums the
tracks that name it, runs its effects, then its gain and pan, and joins the master
sum and the returns it sends to, as a drum bus. Its tracks are next to each other
in `tracks[]`, so that the app draws the group's row above them. A muted group
silences its tracks and their sends; a soloed group is heard with its tracks, and a
soloed track is heard through its group. A group cannot be in a group, and a
sidechain cannot name one. See [groups.md](features/groups.md).

Return: `id` (unique across tracks, groups and returns), `gain_db`, `pan`, `mute`,
`effects` and `automation`. A return sums the sends of tracks and groups, runs its effects and joins the stereo master.
Post-fader sends tap after track gain and pan, pre-fader sends after the inserts.
Muted or solo-muted tracks send nothing; returns are never solo-muted. No
return-to-return sends exist yet.

Effects: `filter`, `eq` (the parametric EQ: bells, shelves and passes with
their slopes, drawn in the app as one curve over the playing spectrum; see
[parametric-eq.md](features/parametric-eq.md)), `compressor` (optional `sidechain` track), `limiter`,
`delay`, `reverb`, `chorus`, `saturation`, `utility` (gain, pan, width, mono
below a frequency, polarity; see [utility.md](features/utility.md)) and
`analyzer` (changes nothing; the app shows the sound through it, in the
device panel and in a window of its own; see
[analyzer.md](features/analyzer.md); not inside a Synth patch), listed in order under
`tracks[].effects`, `groups[].effects`, `returns[].effects`, `master.effects` or, inside a Synth
patch, `tracks[].instrument.synth.effects`, each with an optional `id` unique
within its chain. A patch's effects come before the track's inserts, which
come before track gain and pan; a group's effects run on the sum of its tracks
and precede its gain and pan; return effects precede return gain and pan;
master effects follow `master_gain_db` and precede the end fade. See
[effects.md](features/effects.md) for parameters and semantics. A rack is a
chain as a YAML file in the workspace library, `~/Music/AAW/library/racks/`
(or `AAW_WORKSPACE`), which `daw rack save` writes from any chain and `daw
rack load` and the browser add to a chain in any song, an `id` the chain has
numbered and a sidechain kept only where the song allows it; see
[effect-racks.md](features/effect-racks.md).

Automation: `tracks[].automation`, `groups[].automation`, `returns[].automation` and `master.automation`
list lanes `{param, points}`. `param` is `gain_db`, `pan`, `sends.RETURN.gain_db` (tracks and groups),
`effects.REF.FIELD` or, on a MIDI track with a synth, `instrument.FIELD`, the patch's effects included as `instrument.effects.REF.FIELD` (master: `gain_db` and effects). Points `{at, value, curve, shape}`
are in time order; `curve` is `linear` or `hold`, and `shape` bends a linear segment. A lane overrides the static value
for the whole song and holds its first and last values outside its points. See
[automation.md](features/automation.md).

MIDI track: `type: midi`, with `id`, `gain_db`, `pan`, `mute`, `solo`,
`effects`, `sends` and `automation` as any track, an `instrument`, and note
clips under `clips`; no pads, pattern clips or audio clips. A note clip is
`{id, at, length_beats, loop_beats, notes}` and owns its notes, each `{id, pitch, at,
duration, velocity}` with `pitch` a MIDI number (a name such as `C4` is taken
and stored as 60) and `at` from the clip's start. A clip's ID is unique in the
song and a note's in its clip; one that arrives without an ID is given the
next `clipN` or `nN`. A note sounds until its end or its clip's, and one that
starts before its clip, at a negative `at`, or at or after its end is kept and
does not play: a clip trimmed from either edge keeps the notes it passes. With
`loop_beats`, the notes of the clip's first so many beats play again and again
from its start until its end, the last repetition cut off there, a note held
across the loop's end cut there and one at or after it kept and not played;
the schedule unrolls the loop, so the engine plays what copies would play. An
audio clip loops by the same field, with `length_beats` for how long; see
[looping-clips.md](features/looping-clips.md). `instrument` is
null, which plays nothing, `{sampler: {pads, map}}` or `{synth: {...}}`: pads as below, and map
entries `{notes, pad, pitched}` naming a note or an inclusive range, which may
not overlap; a pitched entry repitches its pad from its sample's root note, or
from middle C (C4, 60) when the sample has none, to the note. The schedule gives a MIDI track's notes to its instrument as pitch,
velocity, start and note-off, and the sampler makes of them the hits a pattern
event would make. The Synth plays them itself, every frame, from a patch of
oscillators with unison and wavetables, a filter a voice, envelopes, LFOs, a
modulation matrix, macros and a chain of effects of its own, whose fields a
track's lanes reach as `instrument.FIELD`; see
[synth.md](features/synth.md) and `daw describe synth`. A patch is that
mapping as a YAML file: twelve factory patches are built into `daw`, and
`daw patch save` writes the person's own to the workspace library,
`~/Music/AAW/library/patches/` (or `AAW_WORKSPACE`), the first folder of the
workspace, which `daw patch load`, `daw synth add --patch` and the browser
load into any song. A Standard MIDI file of one part is imported as a note clip
and a note clip exported as one; the file is not kept or linked, and its tempo
is not taken. See [midi-clips.md](features/midi-clips.md) and `daw describe midi`.

Pad: `sample`, `mode` (`one_shot` or `gate`), `gain_db`, `pan`, `transpose` in
semitones, `start_seconds`, `end_seconds`, `attack_ms`, `release_ms`, optional
`choke_group`, `reverse`, optional `source_bpm`, `stretch` (`repitch` or
`preserve_pitch`), and `mono` for downmixing.

Pattern: positive `length_beats`, `grid` in beats per step, `steps` mapping pad IDs
to strings, optional explicit `events`, `swing` (0.5…0.75). Step rows and explicit
events are additive. Swing delays odd-numbered step cells only; explicit events
retain their exact times. Explicit events: `at`, `pad`, `velocity` (1…127), optional
`note`, `duration`, `transpose`. Gate mode requires a positive duration.

Clip: `pattern`, `at`, positive integer `repeats`, `velocity_scale`. Repetitions
use the pattern's declared length. Clips must fit the session, but sample tails may
cross pattern boundaries. Sections have unique `id`, `at`, `length_beats`.

## Timing and audio semantics

All `at`, `duration` and `length_beats` values are quarter-note beats, whatever the
time signature: a bar of 3/4 is 3 beats, of 6/8 3 and of 7/8 3.5. Position zero
is the first downbeat. Strings such as `1/3` represent exact rational beats. Decimal
values are converted through their string representation. The scheduler computes
each absolute frame independently using round-half-up; it never accumulates rounded
step intervals. This differs from the older draft's bar.beat.fraction notation.

Velocity maps linearly to sample amplitude. A target note is relative to the sample's
explicit root note. Transposition and source-tempo matching use bandlimited repitch:
changing pitch changes playback duration, unless the pad or audio clip is stretched
at its own pitch ([time-stretch.md](features/time-stretch.md)). There is no sustain
loop or glide. The engine does not detect a sample's root note or tempo;
`daw samples analyze` measures them ([sample-analysis.md](features/sample-analysis.md)).

Gate note-off starts a linear release; natural sample end also fades. A note cannot
outlast its source. Choke groups are scoped to each track and release the previous
voice; use a common group for hats or monophonic bass. Very short releases can
still click on low-frequency material; longer releases may be needed on bass.

Mono pad pan uses equal-power gains; stereo pad and track pan use balance, preserving
center stereo levels. Mono conversion averages source channels. Tracks sum in
float64; master gain is `session.master_gain_db` unless a master `gain_db` lane
replaces it. Sidechain keys are the source track after its
inserts and before its gain, pan, mute and solo. Mute takes precedence over solo.
A grouped track's output enters its group early by the group's latency, and every
other track waits for the slowest group, so the faders, sends, group outputs and
return inputs share one timeline; a grouped track's sends are delayed to it. Session end is
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
person. The transport bar’s editable BPM field sets `session.tempo` through this
same command path, including save and undo. The host validates it against the whole song, saves `song.yaml`, records it
in one undo history and change log with its origin, and plays it: levels, pans,
sends and the knobs automation can move glide to their new values over 5 ms, and a
change of structure, such as an added effect, fades through a 10 ms dip. Automation
written in the document is never smoothed, and a render reads only the saved values,
so playback from the start and a render are the same audio. With no host, the same
commands edit the file. [../engine/README.md](../engine/README.md) lists the commands
and describes the host; decisions D36 to D38, D41 and D44 record the choices.
The agent is asked to work in steps the person can watch, a part a command as
soon as it is decided, and a host whose agent sent nothing for five minutes
says so in the reply of its next edit, as `hint`
([watchable-steps.md](features/watchable-steps.md), D87).

`note.preview` is a transport command too: a note of a pitch, velocity and
length played now through a MIDI track's instrument, a Synth or a Sampler,
and the track's chain, from where the stream stands, whether or not the song
plays, outside the timeline and the undo history and in no render. A
Sampler's note is prepared by the host, as the same note in a clip would be,
and handed to the audio thread, which hands it back to be freed. The app's
Synth panel sends it from its keys, the piano roll as notes are drawn,
clicked and moved, and `daw synth audition --play` from the terminal. While the
transport stands still the stream is cut by nothing: the end fade and the
gate at the song's end apply only while it rolls, so a tail or a previewed
note is heard whole wherever the transport is.

The metronome is host transport state, initially off, outside the song and its
undo history. The player mixes its synthesized click after song processing,
at the renderer's audible timeline and current BPM, including on an empty song.
It follows playback, locate and loops, clicks on the note the time signature
counts with an accent on the first of each bar, and fades its monitoring level
over 5 ms when switched. The offline renderer never receives
it, so exports and stems contain only the song. See [metronome](features/metronome.md).

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
Reference tests use a generated song of a quiet and a loud half: its phrases as
sections, a mix with its sub 6 dB up reported by section, sections matched by
ID and by loudness, a file compared whole, a band one side lacks, and an AAC
file of which nothing is kept.

## What is not built

[backlog.md](backlog.md) lists it, in order. Each feature's own limits are in its
file under [features/](features/), and the app's in
[../apps/mac/README.md](../apps/mac/README.md).
