# Rust engine

The Cargo workspace for the native rebuild planned in
[docs/Rust-Swift-Update.md](../docs/Rust-Swift-Update.md). The Python package in
`src/agent_daw` remains the reference implementation and the shipping CLI until
the cutover (M7).

| Crate | Responsibility |
|---|---|
| `aaw-model` | Schema v1 types, validation, exact beats, canonical YAML, fingerprints |
| `aaw-dsp` | Resampler (a port of `scipy.signal.resample_poly`); devices later |
| `aaw-engine` | Song compilation, scheduling, mixing, offline and real-time drivers |
| `aaw-cli` | The `daw` binary |

The session host, Swift bindings and Python bindings join the workspace in the
milestones that need them.

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
| `daw play PROJECT [--from BEAT] [--seconds S] [--buffer FRAMES]` | Plays through the default output and reports callback timing and dropouts |
| `daw play PROJECT --benchmark` | Times the playback path in buffer-sized blocks without a device |

The engine covers the sampler: scheduling, choke groups, gates, repitch, trim,
reverse, downmix, pan laws, track gain, pan, mute and solo, master gain and the
end fade. Effects, sends, returns and automation come in M6. Until then `render`
refuses a song that uses them and `play` plays it without them, with a warning.
A bypassed effect is not processed, so it does not count.

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

