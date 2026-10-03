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

See [the concept](docs/concept.md) for what AAW should become, and
[the documentation index](docs/README.md) for the architecture, the backlog, each
feature and the decision log.

## What works today

AAW today is a sample workstation that an agent and a person operate together.
Search a local sample library, copy sounds into a project, sequence drum and
pitched notes, play the song from any beat and render a mix and stems. It also
provides render analysis and comparisons.

One native engine, written in Rust, plays the song in real time and renders it
offline, so what is heard while editing is what is exported. A session host holds
the open song, applies each edit as a command with undo, and plays it as it
lands. The agent drives it with the `daw` command; the person drives the same
host from a native Mac app with an arrangement that shows each clip's waveform,
a mixer, device panels, automation lanes, a pattern editor and a sample browser,
and sees the agent's changes as they happen. The app opens on a blank project
called Untitled, which Save As… gives a name and a place, and `daw projects`
lists what is open in it. See
[engine/README.md](engine/README.md) and [apps/mac/README.md](apps/mac/README.md).

Samples can be measured from their audio for pitch (with octave), loop tempo and
one-shot/loop kind.

Tracks, returns and the master bus take insert effects: filter, EQ, compressor
with sidechain, look-ahead limiter, tempo-synced delay and a seeded convolution
reverb. Tracks send pre- or post-fader to return buses, which render as their own
stems. See [docs/features/effects.md](docs/features/effects.md).

Automation lanes move track, return and master levels, pans, send levels and
effect parameters over time, such as a filter sweep into a drop or a quieter
second chorus. See [docs/features/automation.md](docs/features/automation.md).

MIDI tracks hold note clips that own their notes, with numeric pitches placed
on and off the grid, and a sampler that maps notes to pads plays them; the
instrument can be attached, swapped or removed without touching the notes, and
a copied clip changes independently. The agent writes them with `daw note` and
`daw instrument`; the person draws and edits them in the app's piano roll. See
`daw describe midi` and [docs/features/midi-clips.md](docs/features/midi-clips.md).

The broader workspace described above is the vision. Saturation, groups, synths,
plugin hosting, MIDI import/export and recording are not implemented.
[docs/architecture.md](docs/architecture.md) describes the implemented behavior.

## Repository scope

This repository tracks the tools for creating music: the engine, the app, the CLI,
tests, documentation, and generic examples. Personal songs and their arrangements, samples,
source manifests, notes, revisions, renders, exports, and composition scripts stay
local under ignored `projects/` or `content/`, or outside this repository.
Song version history can be maintained separately in a private repository.
The original sample library is never modified.

## Setup

```sh
brew install libsndfile                # the engine decodes samples with it
(cd engine && cargo build --release)   # the daw binary
uv sync --extra dev --locked           # the Python tools, with the Rust song model
uv run daw --help
uv run pytest -q && (cd engine && cargo test)
```

It needs Rust stable from [rustup](https://rustup.rs) and Python 3.12+, and is
developed for macOS. Dependencies are pinned in `engine/Cargo.lock` and `uv.lock`.
The runtime is local and does not call a model or require a Splice connection.

The engine, the song model, the session host and every command that reads, edits,
plays or renders a song are Rust (`engine/`). The sample library, sample analysis
and perception are Python (`src/agent_daw`), and read songs through the Rust model,
which `uv sync` builds into the package as `agent_daw.aaw_py`. If uv's Python is an
Intel build on Apple silicon, add its Rust target first: `rustup target add
x86_64-apple-darwin`.

`daw` is one command. `uv run daw` and `engine/target/release/daw` each pass the
other the commands it does not own (`samples`, `listen`, `compare`, `check`,
`timeline`, `joins` and `export` run in Python), so either works for everything. `uv run daw` runs the release build, or
the binary `AAW_DAW` names; the binary runs the Python of this checkout's `.venv`,
or the one `AAW_PYTHON` names. The Mac app builds with `apps/mac/build.sh`; its
bundle holds a copy of the binary, which **Install Command Line Tool…** in the app's
menu links onto the PATH as `daw`.

## Agent workflow

```sh
uv run daw samples scan ~/Splice/sounds/packs
uv run daw samples search 'kick' --type one-shot --limit 8
uv run daw samples search 'bell' --bpm 160 --key 'A#m'
uv run daw samples inspect SAMPLE_ID
uv run daw samples analyze --all
uv run daw samples search --pitched --note-range C1-B1
uv run daw samples analyze SAMPLE_ID
uv run daw samples audition SAMPLE_ID --output /tmp/candidate.wav
uv run daw samples beats SONG.wav --near 0:41

uv run daw init projects/my-beat --tempo 160 --bars 20
uv run daw samples import SAMPLE_ID --project projects/my-beat/song.yaml --id kick
uv run daw samples import BASS_ID --project projects/my-beat/song.yaml --id sub --root-note auto
uv run daw samples import ~/Music/song.m4a --project projects/my-beat/song.yaml --id song
uv run daw describe sampler
uv run daw describe effects
uv run daw describe automation
uv run daw describe project

uv run daw check projects/my-beat/song.yaml
uv run daw inspect projects/my-beat/song.yaml
uv run daw set projects/my-beat/song.yaml tracks.drums.gain_db -4.5
uv run daw play projects/my-beat/song.yaml --from 32
uv run daw render projects/my-beat/song.yaml
uv run daw render projects/my-beat/song.yaml --track drums
uv run daw listen projects/my-beat/renders/latest.json
uv run daw compare projects/my-beat/revisions/before-render.json projects/my-beat/renders/latest.json
uv run daw timeline projects/my-beat/song.yaml --seconds 0:41
uv run daw joins projects/my-beat/renders/latest.json --limit 60
uv run daw export projects/my-beat/song.yaml --to projects/my-beat/exports/my-beat-v1.wav
```

All commands emit JSON. Errors emit JSON to stderr and exit nonzero. A command's
PROJECT is the project's folder or the `song.yaml` in it: `projects/my-beat`
works wherever `projects/my-beat/song.yaml` does. `daw projects` lists the
projects open in the Mac app, the window in front first, and `daw move` and
`daw copy` save a project under another name; see
[docs/features/new-and-untitled-projects.md](docs/features/new-and-untitled-projects.md).
The index is
`.daw/library.sqlite`; supply `daw samples --db /path/index.sqlite ...` to use another.
Filename BPM/key/category are **hints**, not audio-derived facts. Names with C/F/etc.
do not establish an octave. `daw samples analyze` measures pitch with octave and
cents, onsets, loop tempo and one-shot/loop kind from the audio. `--root-note auto`
uses the measured note, and `daw check` warns when a declared root disagrees with
the audio. See [docs/features/sample-analysis.md](docs/features/sample-analysis.md).
`daw samples import` takes a file's path as well as an index ID. An `.m4a` or
`.mp3` file is decoded once into the project as WAV, and a file the engine cannot
play is refused; see "Format" in [docs/architecture.md](docs/architecture.md).
`daw samples beats` maps a whole song: its tempo, every beat, the downbeats with
their alternatives, where the arrangement changes, the beats near a timecode and a
click audition to check the grid by ear. See [docs/features/beat-map.md](docs/features/beat-map.md).
To edit a finished song from timecodes, the `song-edit` skill in `.claude/skills/`
says what to do, and `daw describe edit`, `beats`, `joins` and `export` say how.
Personal skills and the sounds they reuse stay out of Git; see
[docs/features/skills.md](docs/features/skills.md).
A track's `audio` lists audio clips, parts of a sample file placed on beats, and
`daw audio` adds, moves, cuts, splits, trims and crossfades them: an edit of a song is one
clip with ranges cut out. See [docs/features/audio-clips.md](docs/features/audio-clips.md).
In the Mac app an audio file dropped on the timeline is such a clip, which is moved,
trimmed, faded and split there. See [docs/features/audio-clips-in-app.md](docs/features/audio-clips-in-app.md).
A pad or an audio clip with `source_bpm` and `stretch: preserve_pitch` follows the session's tempo at
its own pitch, so an edit can be made a few percent shorter by raising the tempo.
See [docs/features/time-stretch.md](docs/features/time-stretch.md).
`daw timeline` gives a song's places in beats and in seconds, where its sound ends,
the beat a sound starts on to end on another, and fits the session's length. See
[docs/features/timeline.md](docs/features/timeline.md).
`daw joins` checks a rendered edit of a song: whether the beat carries across each
join, whether the splice shows as a step, the level either side and the file's
length, with an excerpt of each join to hear. See [docs/features/join-checks.md](docs/features/join-checks.md).
`daw export` writes the song's render as a named WAV, AAC or MP3 file and reports
the level policy it applied. See [docs/features/export.md](docs/features/export.md).
Audition exports a short WAV for listening; it does not start playback automatically.

Edits are commands: `set` for any value by path, and verbs for tracks, returns,
clips, patterns, pads, effects, sends, lanes and sections (`daw --help` lists them,
and [engine/README.md](engine/README.md) describes them). Each validates the whole
resulting song before it is written. While a host runs for the song, because it is
open in the Mac app or `daw host` or `daw play` is running, commands go to that host:
the edit is heard and shown as it lands, `daw undo` takes it back, and `daw changes`
lists what the person and the agent did. With no host, a command loads the file,
applies the edit and saves.

`inspect` returns the current `project_sha256`. To revise several fields at once,
write a JSON merge patch and run:

```sh
uv run daw apply projects/my-beat/song.yaml /tmp/revision.json --expect SHA_FROM_INSPECT
```

Objects merge, arrays replace, and null removes a field. The edit is validated before
an atomic write; stale expected revisions fail. Writers without a host use an advisory
project lock. Raw file editing is also supported, but outside that concurrency
contract; a running host loads it as an external edit. `fmt` produces stable compact
YAML. `check` validates schema, references and asset hashes; the renderer
additionally validates audio content and trim bounds.

## Authoring contract

See [docs/architecture.md](docs/architecture.md) for the implemented schema and timing/audio semantics.
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

Effects are listed under a track's or return's `effects` or under `master.effects`.
Tracks reach shared reverb and delay through `sends` to `returns`. `automation`
lanes change levels and effect parameters over time:

```yaml
  effects:
  - {type: filter, id: sweep, mode: lowpass, cutoff_hz: 18000, slope_db_per_octave: 24}
  - {type: compressor, threshold_db: -30, ratio: 8, release_ms: 150, sidechain: kick}
  sends:
  - {to: plate, gain_db: -12}
  automation:
  - param: effects.sweep.cutoff_hz
    points:
    - {at: 48, value: 300}
    - {at: 64, value: 18000}
returns:
- id: plate
  effects:
  - {type: reverb, decay_seconds: 1.8, predelay_ms: 20, lowcut_hz: 200}
master:
  effects:
  - {type: limiter, ceiling_db: -1}
```

## Audio output

Each render produces a stereo 48 kHz or 44.1 kHz PCM24 `mix.wav`, aligned float WAV
stems, a project snapshot, and `report.json`. Stems include track effects, master
gain and ending fade, but not master effects. Track stems are dry and each return
has its own stem. Without master effects track and return stems together
reconstruct the mix within quantization tolerance; `stems_sum_to_mix` in the report
says whether they do. Only the effects written in the project are applied: there is
no automatic normalization. Unsafe PCM clipping aborts export; lower
`master_gain_db` or add a master limiter.

`renders/latest.json` points to the latest full mix; previews have their own pointer.
Render directories are keyed by project, actual sample content, engine code,
dependencies and target. Re-rendering the same identity replaces its equivalent
artifacts. A different render cannot overwrite an occupied `--output` directory. Reports
record the engine build, asset hashes, audio hash, peak/RMS, a 4× oversampled peak
estimate, track event counts, effect gain reduction, and render time.
Bit-identical output is checked in the same environment, not promised across machines
or engine builds. Restore `song.snapshot.yaml` into the original project root
to use its project-relative sample paths. Renders made by the earlier Python engine
still verify with `daw listen` and `daw compare`.

Track and section previews evaluate the original timeline before slicing, so notes
and tails beginning before a section are retained. A render is many times faster
than real time: a three-minute song with thirty effects takes about nine seconds.

## Perception and comparison

`daw listen <render>` measures the mix, tracks and sections, then writes JSON and
an energy/spectrogram PNG. `daw compare <before> <after>` reports actual and
loudness-matched deltas plus a comparison chart. Both accept render directories,
render pointers, `report.json`, or standalone audio. Add `--no-images` for JSON only.
Outputs live under `analysis/` inside the render directory or beside standalone
audio. Reports use the saved project snapshot and do not change the song or audio.
See [docs/features/perception.md](docs/features/perception.md) for definitions and limitations.

`docs/concept.md` describes the broader product vision. What is implemented is
deliberately narrower; `docs/architecture.md` is authoritative for the current build,
and `docs/backlog.md` lists what is next.

## License

AAW is open source under the [MIT License](LICENSE).
