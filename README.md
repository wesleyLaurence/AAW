# Agentic Audio Workspace (AAW)

An AI-native digital audio workstation. A next-generation DAW.

Open this with Codex or Claude and start creating music.

AAW is an open-source workspace where humans and agents collaboratively build
music at the track, object, and project level. Here are notes on what the project
is and why it is being created.

## Why AAW

Generative AI music tools are impressive, but the prompt-to-finished-track workflow
can be a poor fit for how producers work. A finished stereo track sounds complete,
but it does not necessarily give you the underlying project. Turning down a guitar,
extending a section by four bars, rewriting only the drums, or changing one
transition can require generating another version instead of editing the original.
You receive the result of a process, but not the process itself.

AAW starts with a different idea: AI should build and manipulate a real, persistent
music-production state. The long-term workspace contains tracks, clips, MIDI,
instruments, plugins, routing, automation, effects, stems, arrangement data, and
project history. Each decision remains available to inspect and change.

Instead of asking:

> Generate another version of this song with quieter guitars.

You could say:

> Turn the rhythm guitars down 1.5 dB in the second chorus, leave the vocals
> untouched, add four bars before the bridge, and increase the bass compression
> slightly.

The agent would translate that request into operations on the project while
preserving the rest of the session.

Generative audio has an important role as one tool among many. An agent could
generate a drum fill, synth texture, guitar layer, riser, vocal harmony, or sound
effect and place it on its own editable track. It could also use synthesis, MIDI,
samples, plugins, stem separation, DSP, or recording tools when those are more
appropriate.

The core loop resembles an AI coding agent working in an audio session:

**inspect → plan → modify → render → listen/analyze → revise**

AI becomes a collaborator in production, engineering, arrangement, sound design,
and technical operation. The human retains authorship and control: decisions can
be inspected, modified, rejected, or refined. The agent accelerates the craft while
keeping the creative process editable.

The word *workspace* is intentional. The long-term environment can bring together
the timeline, mixer, piano roll, files, plugins, agents, code, generated assets,
version history, tasks, and specialized production tools in one shared project.

The goal is **a programmable, agent-native environment for making music**.

See [the project idea](docs/idea.md) for the earlier design principles and
[the documentation index](docs/README.md) for the specification, build plan,
decision log, and design notes.

## What works today

The current MVP is an offline, agent-operated sample workstation. Search a local
sample library, copy sounds into a project, sequence drum and pitched notes, and
render a mix and stems. It also provides render analysis and comparisons.

The broader workspace described above is the vision. UI, effects, synths, plugin
hosting, MIDI import/export, recording, and a realtime engine are not implemented.
[docs/mvp.md](docs/mvp.md) describes the implemented behavior.

## Repository scope

This repository tracks the tools for creating music: the engine, CLI, tests,
documentation, and generic examples. Personal songs and their arrangements, samples,
source manifests, notes, revisions, renders, exports, and composition scripts stay
local under ignored `projects/` or `content/`, or outside this repository.
Song version history can be maintained separately in a private repository.
The original sample library is never modified.

## Setup

```sh
uv sync --extra dev --locked
uv run daw --help
uv run pytest -q
```

Python 3.12+; the MVP is developed for macOS. Dependencies are pinned in `uv.lock`.
The runtime is local and does not call a model or require a Splice connection.

## Agent workflow

```sh
uv run daw samples scan ~/Splice/sounds/packs
uv run daw samples search 'kick' --type one-shot --limit 8
uv run daw samples search 'bell' --bpm 160 --key 'A#m'
uv run daw samples inspect SAMPLE_ID
uv run daw samples audition SAMPLE_ID --output /tmp/candidate.wav

uv run daw init projects/my-beat --tempo 160 --bars 20
uv run daw samples import SAMPLE_ID --project projects/my-beat/song.yaml --id kick
uv run daw samples import BASS_ID --project projects/my-beat/song.yaml --id sub --root-note C1
uv run daw describe sampler
uv run daw describe project

uv run daw check projects/my-beat/song.yaml
uv run daw inspect projects/my-beat/song.yaml
uv run daw render projects/my-beat/song.yaml
uv run daw render projects/my-beat/song.yaml --track drums
uv run daw listen projects/my-beat/renders/latest.json
uv run daw compare projects/my-beat/revisions/before-render.json projects/my-beat/renders/latest.json
```

All commands emit JSON. Errors emit JSON to stderr and exit nonzero. The index is
`.daw/library.sqlite`; supply `daw samples --db /path/index.sqlite ...` to use another.
Filename BPM/key/category are **hints**, not audio-derived facts. Names with C/F/etc.
do not establish an octave: inspect a pitched source before setting its root note.
Audition exports a short WAV for listening; it does not start playback automatically.

`inspect` returns the current `project_sha256`. To revise a project safely, write a
JSON merge patch and run:

```sh
uv run daw apply projects/my-beat/song.yaml /tmp/revision.json --expect SHA_FROM_INSPECT
```

Objects merge, arrays replace, and null removes a field. The edit is validated before
an atomic write; stale expected revisions fail. CLI writes use an advisory project
lock. Raw file editing is also supported, but outside that concurrency contract.
`fmt` produces stable compact YAML. `check` validates schema, references and asset
hashes; the renderer additionally validates audio content and trim bounds.

## Authoring contract

See [docs/mvp.md](docs/mvp.md) for the implemented schema and timing/audio semantics.
All musical times are **quarter-note beats**, zero-based. Fractions such as `1/3`
are accepted. At 160 BPM, 80 beats / 20 bars equals exactly 30 seconds.

```yaml
schema_version: 1
session:
  title: A beat
  tempo: 160
  length_beats: 8
samples:
  kick: {path: samples/kick.wav}
patterns:
  groove:
    length_beats: 8
    grid: 1/4
    steps:
      kick: '9.....8....9....9.....8...9..7..'
tracks:
- id: drums
  gain_db: -6
  pads:
    kick: {sample: kick}
  clips:
  - {pattern: groove, at: 0, repeats: 1}
```

A quarter beat grid means sixteenth notes. `x` triggers velocity 100, digits 1–9
encode increasing velocity, and `.` is a rest. Step rows must exactly span the pattern.
Pitched/gated patterns use events with `at`, `pad`, `note`, `duration`, and `velocity`.

## Audio output

Each render produces a stereo 48 kHz or 44.1 kHz PCM24 `mix.wav`, aligned float WAV
stems, a project snapshot, and `report.json`. Stems include master gain and ending
fade and reconstruct the mix within quantization tolerance. No limiter, normalization,
compression, EQ, or added reverb is used. Unsafe PCM clipping aborts export.

`renders/latest.json` points to the latest full mix; previews have their own pointer.
Render directories are keyed by project, actual sample content, engine code,
dependencies and target. Re-rendering the same identity replaces its equivalent
artifacts. A different render cannot overwrite an occupied `--output` directory. Reports record engine/dependency versions, asset hashes, audio hash,
peak/RMS, a 4× oversampled peak estimate, track event counts, and render time.
Bit-identical output is checked in the same environment, not promised across machines
or dependency upgrades. Restore `song.snapshot.yaml` into the original project root
to use its project-relative sample paths.

Track and section previews evaluate the original timeline before slicing, so notes
and tails beginning before a section are retained. They currently perform full-track
work; there is no incremental cache yet.

## Perception and comparison

`daw listen <render>` measures the mix, tracks and sections, then writes JSON and
an energy/spectrogram PNG. `daw compare <before> <after>` reports actual and
loudness-matched deltas plus a comparison chart. Both accept render directories,
render pointers, `report.json`, or standalone audio. Add `--no-images` for JSON only.
Outputs live under `analysis/` inside the render directory or beside standalone
audio. Reports use the saved project snapshot and do not change the song or audio.
See [docs/perception.md](docs/perception.md) for definitions and limitations.

The older files in `docs/` describe the broader product vision. The implemented MVP
is deliberately narrower; `docs/mvp.md` is authoritative for the current build.

## License

AAW is open source under the [MIT License](LICENSE).
