# Rust engine

The Cargo workspace of the native build planned in
[docs/archive/Rust-Swift-Update.md](../docs/archive/Rust-Swift-Update.md): the song model, the
engine that plays and renders, the session host and the `daw` command. It
replaced the Python engine at that plan's cutover (M7). The Python package in
`src/agent_daw` keeps the sample library, sample analysis and perception, and
reads songs through `aaw-py`.

| Crate | Responsibility |
|---|---|
| `aaw-model` | Schema v1 types, validation, exact beats, canonical YAML, fingerprints, the event schedule, the schema `daw describe` prints |
| `aaw-dsp` | Resampler (a port of `scipy.signal.resample_poly`), automation envelopes and the six effects |
| `aaw-engine` | Song compilation, routing, latency alignment, mixing, the transport, offline and real-time drivers, waveform peaks |
| `aaw-host` | The session host: commands, handles, undo, change log, saving, external edits, socket |
| `aaw-cli` | The `daw` binary |
| `aaw-ffi` | What the Mac app calls, through UniFFI: a hosted song's arrangement, devices, lanes, patterns, waveforms, changes and transport, and the sample library |
| `aaw-py` | The model for Python, through PyO3: the `agent_daw.aaw_py` module |

## Build and test

Install Rust stable with [rustup](https://rustup.rs) and libsndfile (`brew install
libsndfile`; set `SNDFILE_LIB_DIR` to use another copy), then from this directory:

```sh
cargo test
cargo build --release   # target/release/daw
```

If the checkout lives in a synced folder, exclude `target/` from syncing. For
Dropbox: `mkdir -p target && xattr -w com.dropbox.ignored 1 target`, repeated
after `cargo clean`, which deletes the directory.

`aaw-py` is a Python extension module, so plain `cargo` commands leave it out
(the workspace's `default-members`). The Python package builds it for its own
interpreter with setuptools-rust: `uv sync`, and `uv run` after a change to the
model, as [pyproject.toml](../pyproject.toml) sets up. An Intel Python on Apple
silicon needs `rustup target add x86_64-apple-darwin`. To check it alone:
`PYO3_PYTHON=../.venv/bin/python cargo check -p aaw-py`.

`daw` is not on the PATH. Run it as `target/release/daw`, or as `uv run daw`,
which runs that binary (or the one `AAW_DAW` names). The Mac app's bundle holds
a copy, which the app's menu links onto the PATH
([apps/mac/README.md](../apps/mac/README.md)). It implements:

| Command | Does |
|---|---|
| `daw init DIRECTORY [--tempo T] [--bars N]` | Creates `DIRECTORY/song.yaml`, an empty song |
| `daw describe [project\|sampler\|effects\|automation\|edit\|beats\|joins\|export]` | The authoring contract: the schema and what its fields mean, and how to edit a finished song, map its beats, check its joins and export it |
| `daw fmt PROJECT` | Rewrites the song in canonical form |
| `daw apply PROJECT PATCH --expect SHA [--label TEXT]` | Replaces fields from a JSON merge patch, unless the song changed since SHA; a label names the edit in the change log and for undo |
| `daw samples ...`, `daw listen`, `daw compare`, `daw check`, `daw timeline`, `daw joins`, `daw export` | Run in Python, with the same arguments and output: the sample library, perception, `inspect` with measured root notes and warnings, the timeline in beats and seconds, the checks of an edit's joins, and a named deliverable from a render. The binary uses the checkout's `.venv/bin/python`, or `AAW_PYTHON` |
| `daw audio add\|cut\|split\|trim\|crossfade` | Audio clips on a track: parts of a sample file placed on beats, a range removed with the gap closed, and the fades of a join |
| `daw model PATH...` | Canonical YAML and fingerprints, or validation errors |
| `daw schedule PROJECT` | Every hit's start frame, track, pad and release frame |
| `daw render PROJECT [--output DIR] [--track T] [--section S]` | Mix, stems, snapshot and `report.json`, which `daw listen` and `daw compare` read |
| `daw host PROJECT` | Runs a session host until interrupted or `daw close` |
| `daw play PROJECT [--from BEAT] [--seconds S] [--buffer FRAMES]` | Plays through the default output; see below |
| `daw play PROJECT --benchmark` | Times the playback path in buffer-sized blocks without a device, and a compile, a recompile, building a renderer and each track's waveform peaks |
| `daw stop`, `daw locate PROJECT BEAT`, `daw loop PROJECT START LENGTH`, `daw loop PROJECT off` | Transport of a running host |
| `daw inspect`, `daw get PROJECT [PATH]`, `daw status`, `daw changes PROJECT --since REV` | Reading: summary, part of the song, host state, change log |
| `daw set PROJECT PATH VALUE`, `daw toggle`, `daw remove` | Any value by path, e.g. `tracks.drums.gain_db -4.5` |
| `daw track`, `return`, `clip`, `pattern`, `pattern event`, `pad`, `effect`, `send`, `lane`, `lane point`, `section` | The command catalog of the rebuild plan; `--help` lists each group's verbs |
| `daw undo`, `daw redo`, `daw batch PROJECT FILE [--label TEXT]` | History of a running host; a JSON list of commands as one step, which a label names in the change log and for undo |

The engine covers the whole song: the sampler (scheduling, choke groups, gates,
repitch, trim, reverse, downmix, pan laws), the six effects on tracks, returns
and the master, sidechains, pre- and post-fader sends, automation lanes, track
gain, pan, mute and solo, master gain and the end fade. `render` writes the
mix, a stem for each track and return, the snapshot and `report.json` with what
each effect did; `--track` renders a track or a return as its stem, with only
the tracks that key or feed it.

## The engine

A song is compiled into a program (`program.rs`): each track's voices, each
channel's effect chain with the envelopes of its lanes, the routing, and the
delays that align latency. A `Renderer` (`render.rs`) plays a program as one
stream, for export and for playback alike. Output does not depend on how the
stream is cut into blocks, and processing never allocates.

- **Devices** (`aaw-dsp`) were ported from the Python engine operation for operation:
  Butterworth filters designed as `scipy.signal.butter` designs them, RBJ
  equalizer bands, the state-variable filter that automation moves, the
  compressor, the look-ahead limiter and the tempo-synced delay. A lane whose
  points share one value is that static value.
- **The reverb** has the Python engine's impulse response except for its noise,
  which comes from a generator of its own (D40, D44), so a tail is statistically
  the same and not sample-identical. Its convolution is non-uniformly
  partitioned and adds no latency.
- **Latency.** Only the limiter delays its input. A track keyed by a source
  with latency renders its voices that much later, tracks are delayed to the
  slowest before their faders and sends, and the output trails the transport by
  the total; `daw status` reports it as `latency_frames`. An offline render
  runs that much longer and places every stem on the timeline.
- **A compile redoes only what changed.** Each track's voices, prepared pad
  audio and reverb kernels are kept from the last compile, so after a level or
  knob edit it takes under a millisecond. Pad audio a first compile lacks is
  repitched on several threads.
- **Peaks** (`peaks.rs`) are what a track's voices sum to, before its inserts
  and fader, as the least and greatest sample of every 64 frames and of
  coarser stretches four times as long each, for a display to draw at any
  zoom. A track's program carries an identity of what its voices are made
  from, so peaks are worked out again only for the tracks an edit changed.

## Session host

A host holds a song in memory as the authority for it, applies commands one at
a time, keeps an undo history and a change log, and plays the song with each
edit heard as it lands. `daw host PROJECT` runs one until interrupted or `daw
close PROJECT`; `daw play PROJECT` without a running host hosts the project for
as long as it plays. Each change prints to the host's stderr as a JSON line.

Every other command reaches the running host of its project through a Unix
socket registered under `~/Library/Application Support/AAW/hosts` (or
`AAW_HOST_DIR`), outside the project because projects may be synced. With no
host, commands run headless: load, apply, save, exit, under the project's
`.daw.lock`. Edits behave the same either way; undo, redo, the change
log, handles and the transport need a host.

- **Commands** validate the whole resulting song before they apply and carry an
  origin, `agent` by default or `--origin user`. `--expect SHA` refuses an edit
  over an unseen change. A host saves canonical `song.yaml` before it replies.
- **Values** are JSON, or text when they do not parse as JSON: `-4.5`, `true`,
  `1/3`, `C3`, `'{"sample": "kick"}'`. Fields of objects a command adds or
  changes are `--name value` pairs, such as `daw pad set PROJECT drums kick
  --gain-db -3 --release-ms 40`.
- **Paths** are dot-separated. A list item is an index, its `id` (a send: its
  `to`), or a handle: while a host runs, clips, effects, events and points have
  handles such as `@12` that keep naming the same object as other edits land.
  `daw inspect` lists each clip's reference and `daw get` puts one on every list
  item.
- **A batch builds on itself.** A track, return or pattern a batch adds can be
  added to by its later commands: a pad and a clip on a new track, steps and
  events in a new pattern.
- **References stay valid.** Renaming a track renames the sidechains naming it;
  renaming or removing a return updates or removes its sends and their lanes;
  inserting, moving or removing an effect rewrites or removes the lanes that
  address effects by index; a lane's last point takes the lane with it. Removing
  something still referenced, such as a pad a pattern plays, is refused.
- **Gestures.** A request may carry a `gesture` ID, as the app's drags do.
  Edits of one gesture and origin that land one after another are one undo
  step and one change-log entry, named for the whole move, and are saved once
  they pause for 250 ms instead of after each.
- **Selection.** `daw status` lists what the person has selected in the app as
  `{"ref", "path"}` pairs, so a request about "the selected clip" can be
  answered with `daw get PATH`. The app sets it with the `select` command.
- **External edits**, such as a text edit or `git checkout`, load as one
  undoable `external` change. A file that does not validate
  pauses edits, with the error in `daw status`, until it is fixed; `daw fmt`
  writes the host's song over it instead.
- **Playback** recompiles the song after each edit and the audio thread swaps
  to the new program between blocks, at the same beat; a tempo change keeps the
  beat. A program with the same channels and devices takes over in place:
  devices keep their state, so tails ring on; levels, pans, sends, mutes and
  the knobs automation can move glide to their new values over 5 ms; a filter
  or equalizer crosses over to its new setting; and where a track's voices
  changed, the old ones ring out under the new. A program with another
  structure, such as an added, removed or bypassed effect, another delay time
  or reverb response, or an added or removed track, is swapped in at the bottom
  of a 5 ms fade out and back in, and the devices still there keep their state.
- **The transport** moves only the voices. Stop, locate and loop jumps let the
  old position ring out for 5 ms without new hits and fade in voices picked up
  mid-sample; hits at the new position play in full. Effects run on, so a
  reverb rings through a locate and after a stop; the stream rests once it has
  been silent for a second. The audio thread never allocates, locks or blocks;
  replaced programs return to the host to be freed.

A process can embed a host instead of running `daw host`: `host::spawn` runs one
on its own thread and calls an observer with the opened song, each change with
the song after it, each revision's program once it is compiled, transport
changes, warnings and the close, while `Clock` gives the playhead straight from
the audio thread. The Mac app does this through `aaw-ffi`, whose `Song` turns
each revision into the arrangement the app draws (`view.rs`), with every
effect's fields as a control needs them, every lane's points and every
pattern's steps and events, names the tracks, clips, returns and sections a
change touched, and turns the person's edits into commands (`edits.rs`),
working out exact beats from the song for a clip moved by so many beats or
copied after itself, and for an event moved by steps of its pattern's grid.
What a device panel shows of an effect type comes from `aaw_model::describe`.
Such a host is reached through its socket like any other, and compiles each
revision as it lands so that play starts at once.

Waveforms follow each compile (`waveform.rs`): a thread of its own works out
the peaks of the tracks whose identity it has not seen, a few at a time, and
tells the app which audio each track has at the revision and the peaks it has
not been sent. Only the latest revision is worked on, and peaks are kept for a
while, so undo and redo find theirs.

The app's sample browser asks Python (`library.rs`, `aaw_host::python`):
`daw samples search` for what it lists, and `daw samples import --copy-only`
to copy a chosen file into the project, which the song then takes as one edit
with the pad, and the track if it is new, that plays it.

`crates/aaw-host/tests` cover every command, handles, undo, batches, gestures,
the selection, external edits, concurrent clients, an embedded host and a
real-time stress run: random edits compiled and
swapped in while a thread renders 128-frame blocks on schedule under allocation
checking. `tests/test_host.py` checks `inspect` and command results against
the model as Python reads it, and drives a `daw host` process from outside,
including a sample import that reaches the song through the host.
`crates/aaw-ffi/tests` open a song as the app does and check the arrangement,
what each kind of change touches, the person's edits and their place in the
shared history, the fields a device panel is drawn from, edits of effects,
equalizer bands, lanes and points, the transport shared with an agent, the
waveforms that follow each kind of edit, patterns as the editor draws them,
edits of steps, events, lengths and grids, a sample added as a pad or a track,
and, where the checkout's Python is there, a search of a generated library and
a copy from it.

## The model

`aaw-model` is the one implementation of the document. It began as an exact port
of the Python model (pydantic and PyYAML): it accepts and rejects the same
documents, `save` writes the same bytes, and `project_sha256` and the legacy
fingerprints are the same, so existing projects, render reports and `--expect`
SHAs carried over. It reads YAML with PyYAML's YAML 1.1 rules (`010` is 8, `1e5`
is a string, `yes` is true, merge keys apply), applies pydantic's coercions, and
writes with a port of PyYAML's emitter.

`daw model PATH...` prints each document's canonical YAML and fingerprints, or its
validation errors as `{loc, type, msg}`, which are pydantic's. Before the Python
model was deleted the two agreed on a generated corpus of 6,133 documents: valid
songs, the same songs with one field broken, and YAML edge cases. What the Python
model did with 533 of them, and with 120 generated `fmt` and `apply` edits, is
pinned in `tests/golden_model.json`, and `tests/test_model.py` holds the Rust model
to it. `AAW_UPDATE_GOLDEN=1 uv run pytest tests/test_model.py` accepts a
deliberate change. `crates/aaw-model/tests` pin the fingerprints each earlier
schema wrote, and walk a song with one of every model beside the schema `daw
describe` prints: every field, default, limit and choice in it is what validation
enforces.

Differences from the Python model, all in input no tool writes:

- An escaped UTF-16 surrogate pair in a double-quoted string, such as
  `"\ud83c\udfb5"`, is rejected. PyYAML kept it as two lone surrogates; libyaml
  and the YAML spec reject it. JSON written with `ensure_ascii` produces these.
- Beats and note octaves accept ASCII digits only; Python also accepted other
  Unicode decimal digits.
- YAML syntax errors are reported in libyaml's words rather than PyYAML's.
- Error messages quote values with an approximation of Python's `repr` for
  unprintable characters.

## What holds the engine to the Python engine's sound

The engine was ported against the Python engine and compared with it until the
cutover. The last comparison, on October 1, 2026:

- **Without effects**, renders of 60 generated songs, with generated audio in
  every format the library reads (8- to 32-bit PCM, float and double WAV, AIFF
  and FLAC at 22.05 to 96 kHz), were byte-identical: `mix.wav`, every stem and
  the snapshot, with the same schedules.
- **With effects, sends, returns and automation**, over 60 generated songs the
  largest difference in any of 257 mixes and stems was 5.8e-11, and the reports
  agreed on each effect's latency, lanes and gain reduction.
- **Reverb** tails come from another noise generator (D40), so they were
  compared on what a tail is made of: the frame it starts on, its energy and
  width, and over several seeds its decay time and the level of each octave.
- **Copies of five local songs**: one was refused by both engines for clipping,
  with the same message; one without effects was byte-identical in all 11 files;
  in the other three every stem without reverb was within 6e-8, reverb returns
  within 0.45 dB and mixes within 0.02 dB in level.

Two details keep effect-free bytes equal to the Python engine's. The repitch is a
port of `resample_poly`, including `firwin`'s Kaiser window, Cephes `i0` and
numpy's pairwise sum; it matches scipy to a few units in the last place. Decoding
uses libsndfile, but the 24-bit mix is written here: Homebrew's libsndfile rounds
24-bit PCM differently from the build that Python's `soundfile` bundles, and
the engine writes `lrint(x * 2**31) >> 8` as the latter does.

With the reference gone, tests hold the engine to its own promises. The engine's
tests prove that output does not depend on block partition, with and without
effects, that playing from the middle equals the same frames of a render from
the start, that stems sum to the mix before master effects, that a preview is
its channel's stem, that float stems keep the bytes of the first renders, and
that processing never allocates. Each device has tests of what it does
(`crates/aaw-dsp`). `crates/aaw-engine/tests/player.rs` plays through the
transport: a song with latency against its render, levels and knobs stepped
thirty times a second without a click, a tail ringing through an edit, a locate
and a stop, a change of structure fading through silence, and a stopped stream
coming to rest. The Python suite drives `daw render`: `tests/test_renders.py`
renders generated songs whole, in other block sizes, as sections and as single
channels and holds the results to each other and to the song, and the effect,
routing and automation tests run chains and songs with generated audio.
