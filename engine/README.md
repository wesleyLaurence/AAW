# Rust engine

The Cargo workspace for the native rebuild planned in
[docs/Rust-Swift-Update.md](../docs/Rust-Swift-Update.md). The Python package in
`src/agent_daw` remains the reference implementation and the shipping CLI until
the cutover (M7).

| Crate | Responsibility |
|---|---|
| `aaw-model` | Schema v1 types, validation, exact beats, canonical YAML, fingerprints |
| `aaw-dsp` | Sampler voices, resampler and devices |
| `aaw-engine` | Song compilation, scheduling, mixing, offline and real-time drivers |
| `aaw-cli` | The `daw` binary |

The session host, Swift bindings and Python bindings join the workspace in the
milestones that need them.

## Build and test

Install Rust stable with [rustup](https://rustup.rs), then from this directory:

```sh
cargo test
cargo build --release   # target/release/daw
```

If the checkout lives in a synced folder, exclude `target/` from syncing. For
Dropbox: `mkdir -p target && xattr -w com.dropbox.ignored 1 target`, repeated
after `cargo clean`, which deletes the directory.

The Rust `daw` is not on the PATH; `uv run daw` remains the Python CLI. Run the
Rust one as `target/debug/daw` or `cargo run -q -p aaw-cli --`. It implements
`fmt`, `apply` and `model` so far.

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
  `"🎵"`, is rejected. PyYAML keeps it as two lone surrogates; libyaml
  and the YAML spec reject it. JSON written with `ensure_ascii` produces these.
- Beats and note octaves accept ASCII digits only; Python also accepts other
  Unicode decimal digits.
- YAML syntax errors are reported in libyaml's words rather than PyYAML's.
- Error messages quote values with an approximation of Python's `repr` for
  unprintable characters.
