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
