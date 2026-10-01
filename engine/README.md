# Rust engine

The Cargo workspace for the native rebuild planned in
[docs/Rust-Swift-Update.md](../docs/Rust-Swift-Update.md). The Python package in
`src/agent_daw` remains the reference implementation and the shipping CLI until
the cutover (M7).

| Crate | Responsibility |
|---|---|
| `aaw-model` | Schema v1 types, validation, exact beats, canonical YAML, fingerprints |
| `aaw-dsp` | Resampler (a port of `scipy.signal.resample_poly`); devices later |
| `aaw-engine` | Song compilation, scheduling, mixing, the transport, offline and real-time drivers |
| `aaw-host` | The session host: commands, handles, undo, change log, saving, external edits, socket |
| `aaw-cli` | The `daw` binary |
| `aaw-ffi` | What the Mac app calls, through UniFFI: a hosted song's arrangement, changes and transport |

The Python bindings join the workspace in the milestone that needs them.

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

The Rust `daw` is not on the PATH; `uv run daw` remains the Python CLI. Run the
Rust one as `target/release/daw` or `cargo run -q -p aaw-cli --`. It implements:

| Command | Does |
|---|---|
| `daw fmt`, `daw apply` | As the Python verbs |
| `daw model PATH...` | Canonical YAML and fingerprints, or validation errors |
| `daw schedule PROJECT` | Every hit's start frame, track, pad and release frame |
| `daw render PROJECT [--output DIR] [--track T] [--section S]` | Mix, stems, snapshot and `report.json` in the Python engine's formats |
| `daw host PROJECT` | Runs a session host until interrupted or `daw close` |
| `daw play PROJECT [--from BEAT] [--seconds S] [--buffer FRAMES]` | Plays through the default output; see below |
| `daw play PROJECT --benchmark` | Times the playback path in buffer-sized blocks without a device |
| `daw stop`, `daw locate PROJECT BEAT`, `daw loop PROJECT START LENGTH`, `daw loop PROJECT off` | Transport of a running host |
| `daw inspect`, `daw get PROJECT [PATH]`, `daw status`, `daw changes PROJECT --since REV` | Reading: summary, part of the song, host state, change log |
| `daw set PROJECT PATH VALUE`, `daw toggle`, `daw remove` | Any value by path, e.g. `tracks.drums.gain_db -4.5` |
| `daw track`, `return`, `clip`, `pattern`, `pattern event`, `pad`, `effect`, `send`, `lane`, `lane point`, `section` | The command catalog of the rebuild plan; `--help` lists each group's verbs |
| `daw undo`, `daw redo`, `daw batch PROJECT FILE` | History of a running host; a JSON list of commands as one step |

The engine covers the sampler: scheduling, choke groups, gates, repitch, trim,
reverse, downmix, pan laws, track gain, pan, mute and solo, master gain and the
end fade. Effects, sends, returns and automation come in M6. Until then `render`
refuses a song that uses them and `play` plays it without them, with a warning.
A bypassed effect is not processed, so it does not count. Commands edit all of
them already.

## Session host

A host holds a song in memory as the authority for it, applies commands one at
a time, keeps an undo history and a change log, and plays the song with each
edit heard as it lands. `daw host PROJECT` runs one until interrupted or `daw
close PROJECT`; `daw play PROJECT` without a running host hosts the project for
as long as it plays. Each change prints to the host's stderr as a JSON line.

Every other command reaches the running host of its project through a Unix
socket registered under `~/Library/Application Support/AAW/hosts` (or
`AAW_HOST_DIR`), outside the project because projects may be synced. With no
host, commands run headless: load, apply, save, exit, under the `.daw.lock` the
Python writers take. Edits behave the same either way; undo, redo, the change
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
- **References stay valid.** Renaming a track renames the sidechains naming it;
  renaming or removing a return updates or removes its sends and their lanes;
  inserting, moving or removing an effect rewrites or removes the lanes that
  address effects by index; a lane's last point takes the lane with it. Removing
  something still referenced, such as a pad a pattern plays, is refused.
- **External edits**, such as a text edit, `git checkout` or a Python `daw
  apply`, load as one undoable `external` change. A file that does not validate
  pauses edits, with the error in `daw status`, until it is fixed; `daw fmt`
  writes the host's song over it instead.
- **Playback** recompiles the song after each edit, reusing prepared sample
  audio, and the audio thread crossfades into it over 5 ms at the same beat. A
  tempo change keeps the beat. Stop, locate and loop jumps let the old position
  ring out for 5 ms without new hits and fade in voices picked up mid-sample;
  hits at the new position play in full. The audio thread never allocates,
  locks or blocks; replaced programs return to the host to be freed.

A process can embed a host instead of running `daw host`: `host::spawn` runs one
on its own thread and calls an observer with the opened song, each change with
the song after it, transport changes, warnings and the close, while `Clock`
gives the playhead straight from the audio thread. The Mac app does this through
`aaw-ffi`, whose `Song` turns each revision into the arrangement the app draws
(`view.rs`) and names the tracks, clips, returns and sections a change touched.
Such a host is reached through its socket like any other, and compiles each
revision as it lands so that play starts at once.

`crates/aaw-host/tests` cover every command, handles, undo, batches, external
edits, concurrent clients, an embedded host and a real-time stress run: random edits compiled and
swapped in while a thread renders 128-frame blocks on schedule under allocation
checking. `tests/test_rust_host.py` checks `inspect` and command results against
the Python model and drives a `daw host` process from outside.
`crates/aaw-ffi/tests` open a song as the app does and check the arrangement,
what each kind of change touches, and the transport shared with an agent.

## Model parity

`aaw-model` ports `model.py` exactly: it accepts and rejects the same documents,
`save` writes the same bytes, and `project_sha256` and the legacy fingerprints
are the same, so existing projects, render reports and `--expect` SHAs carry over.
It reads YAML with PyYAML's YAML 1.1 rules (`010` is 8, `1e5` is a string, `yes`
is true, merge keys apply), applies pydantic's coercions, and writes with a port
of PyYAML's emitter.

`daw model PATH...` prints each document's canonical YAML and fingerprints, or its
validation errors as `{loc, type, msg}`, which match pydantic's. The pytest file
`tests/test_rust_model_parity.py` compares both models over a generated corpus
of valid songs, the same songs with one field broken, and YAML edge cases, and
compares `fmt` and `apply` end to end. `AAW_PARITY_SIZE=2000 uv run pytest
tests/test_rust_model_parity.py` runs a larger corpus.

Known differences, all in input no tool writes:

- An escaped UTF-16 surrogate pair in a double-quoted string, such as
  `"\ud83c\udfb5"`, is rejected. PyYAML keeps it as two lone surrogates; libyaml
  and the YAML spec reject it. JSON written with `ensure_ascii` produces these.
- Beats and note octaves accept ASCII digits only; Python also accepts other
  Unicode decimal digits.
- YAML syntax errors are reported in libyaml's words rather than PyYAML's.
- Error messages quote values with an approximation of Python's `repr` for
  unprintable characters.

## Engine parity

Rust renders of effect-free songs are byte-identical to the Python engine's:
`mix.wav`, every stem and the snapshot. `tests/test_rust_engine_parity.py`
renders generated songs with generated audio in every format the library reads
(8- to 32-bit PCM, float and double WAV, AIFF and FLAC at 22.05 to 96 kHz) with
both engines, requires audio within -120 dBFS, and compares schedules. The
engine's own tests prove that output does not depend on block partition, that
playing from the middle equals the same frames of a render from the start, and
that processing never allocates.

Two details keep the bytes equal. The repitch is a port of `resample_poly`,
including `firwin`'s Kaiser window, Cephes `i0` and numpy's pairwise sum; it
matches scipy to a few units in the last place. Decoding uses libsndfile, but
the 24-bit mix is written here: Homebrew's libsndfile rounds 24-bit PCM
differently from the build that Python's `soundfile` bundles, and the engine
writes `lrint(x * 2**31) >> 8` as the latter does.

