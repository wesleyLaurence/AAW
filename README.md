# Agentic Audio Workspace (AAW)

An AI-native digital audio workstation. A next-generation DAW.

Open this with Codex or Claude and start creating music.

For agent tasks, [AGENTS.md](AGENTS.md) routes to [making music](docs/music.md)
or [developing AAW](docs/development.md); read the guide for the current task.
Start the agent at this root, or in a project's folder under it for a session
about that one song: either has the project's `SONG.md` and your own
instructions ([what every session is given](docs/features/agent-context.md)).
Claude Code reads `AGENTS.md` from version 2.1.277.

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

AAW today is a workstation that an agent and a person operate together on one
song: samples, a synthesizer and instruments of your own, MIDI and audio clips,
recording, a mixer with groups, returns and effects, automation, rendering and
measurement of what was rendered.

One native engine, written in Rust, plays the song in real time and renders it
offline, so what is heard while editing is what is exported. A session host holds
the open song, applies each edit as a command with undo, and plays it as it
lands. The agent drives it with the `daw` command; the person drives the same
host from a native Mac app and sees the agent's changes as they happen. See
[engine/README.md](engine/README.md) and [apps/mac/README.md](apps/mac/README.md).

**The Mac app** has an arrangement that shows each clip's waveform, a mixer,
device panels, automation lanes for every row (A shows them all), a pattern
editor, a piano roll with a velocity lane, a browser of favorites, samples,
instruments, effects, racks and plugins, a live analyzer, and File › Export
Audio…. Each project's window has an agent panel: a conversation with your own
Claude Code started in the project's folder, with your skills under `/`, the
song's tracks, clips and samples under `@`, files and clips dropped in, a sound
recorded from the input and words spoken into the field. The app opens on a
blank project called Untitled, which Save As… gives a name and a place. A
project is a folder with `song.yaml`, its samples and renders, `SONG.md` for
what the song is for, and an `.aaw` file named for the folder that the Finder
opens in the app ([the `.aaw` project file](docs/features/project-file.md)).

**Sound sources.** Pattern tracks play samples from pads on a step grid. MIDI
tracks hold note clips that own their notes, played by a Sampler, by the Synth,
a polyphonic synthesizer whose sound is a patch in the song (with factory
patches, unison, wavetables and a modulation matrix; `daw describe synth`), or
by an instrument of your own. MIDI effects before the instrument change the
notes as they play: an arpeggiator, a chord, a scale and a velocity. A track's
`audio` holds audio clips, parts of a sample file placed on beats, moved,
trimmed, faded, split, crossfaded and looped; a pad or an audio clip can follow
the tempo at its own pitch. An armed audio track records from an input device
or the song's own sound, and a track can be frozen to a file. `daw separate`
takes a song apart on this Mac into vocals and instrumental, or drums, bass,
vocals and the rest, and `daw generate sound` makes a sound from a description
through ElevenLabs at your own key. Standard MIDI files of one part go in and
out (`daw describe midi`).

**Mixing.** Tracks, groups, returns and the master take insert effects:
filter, parametric EQ, compressor with sidechain, look-ahead limiter that can
hold a true peak, clipper, saturation, overdrive, chorus, tempo-synced delay,
seeded convolution reverb, utility and an analyzer that only shows the sound.
A chain can be saved as a rack and loaded into any song, or kept in the chain
as one rack device; a MIDI track's instrument with its chain is an instrument
rack. Groups sum tracks and other groups, to any depth, and tracks and groups
send pre- or post-fader to returns. Automation lanes move levels, pans, sends
and effect parameters over time, each segment straight or bent. See
[docs/features/effects.md](docs/features/effects.md) and
[docs/features/automation.md](docs/features/automation.md).

**Devices of your own.** A plugin is a folder with a manifest of its fields and
its code, written to AAW's device spec: an effect, an instrument or a MIDI
effect, run offline in Python or, for an effect or an instrument, in real time
as WebAssembly built from Rust. `daw plugin new` scaffolds a working one, the
app draws its panel from the manifest, and lanes move its fields. See
[docs/features/custom-devices.md](docs/features/custom-devices.md).

**Listening and checking.** Samples are measured from their audio for pitch,
loop tempo, kind and what they sound like, and search sorts and filters by
them. `daw listen` and `daw compare` measure renders: loudness, spectrum,
overlap between stems, what a mix loses in mono and on a small speaker,
dynamics, and the difference from a reference song. `daw map` draws the song as
a grid of tracks by bars, `daw check` warns of musical problems, and markers
the person leaves with M while the song plays tell the agent where to look.

The broader workspace described above is the vision. A tempo or a meter that
changes within a song, a MIDI keyboard, a time selection in the app and an app
that runs without this checkout's Python are not built yet;
[docs/backlog.md](docs/backlog.md) lists what is next. Third-party plugins are
not hosted, by design: devices are AAW's own or written to its spec.
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
The runtime is local and does not call a model or require a Splice connection;
`daw generate`, which calls ElevenLabs at a key you keep with `daw keys set`, is
the one exception, and only when asked.

Three parts are set up only when wanted: `daw separate setup` installs the
stem-separation models in an environment of their own, `rustup target add
wasm32-unknown-unknown` lets `daw plugin build` compile a real-time plugin, the
FAUST compiler (`brew install faust`) lets it compile one written in FAUST, and
the Mac app needs Xcode 16.2 ([apps/mac/README.md](apps/mac/README.md)).

The engine, the song model, the session host and every command that reads, edits,
plays or renders a song are Rust (`engine/`). The sample library, sample analysis
and perception are Python (`src/agent_daw`), and read songs through the Rust model,
which `uv sync` builds into the package as `agent_daw.aaw_py`. If uv's Python is an
Intel build on Apple silicon, add its Rust target first: `rustup target add
x86_64-apple-darwin`.

`daw` is one command. `uv run daw` and `engine/target/release/daw` each pass the
other the commands it does not own (`samples`, `reference`, `listen`, `compare`,
`check`, `timeline`, `joins`, `export` and `separate` run in Python), so either works for everything. `uv run daw` runs the release build, or
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
uv run daw samples search kick --sort punch --limit 5
uv run daw samples search snare --short --bright
uv run daw samples like SAMPLE_ID --limit 5
uv run daw samples analyze SAMPLE_ID
uv run daw samples audition SAMPLE_ID --output /tmp/candidate.wav
uv run daw samples beats SONG.wav --near 0:41

uv run daw init projects/my-beat --tempo 160             # song.yaml, and SONG.md for what the song is for
uv run daw samples import SAMPLE_ID --project projects/my-beat/song.yaml --id kick
uv run daw samples import BASS_ID --project projects/my-beat/song.yaml --id sub --root-note auto
uv run daw samples import ~/Music/song.m4a --project projects/my-beat/song.yaml --id song
uv run daw describe
uv run daw describe start
uv run daw describe recipes.drum-bus
uv run daw describe sampler
uv run daw describe effects
uv run daw describe automation
uv run daw describe project --schema

uv run daw synth add projects/my-beat keys --patch 'Soft Pad'   # a MIDI track with the Synth
uv run daw clip add projects/my-beat keys --id verse --length-beats 8
uv run daw note add projects/my-beat tracks.keys.clips.verse --pitch C4 --duration 1
uv run daw group add projects/my-beat drum-bus --tracks drums,perc
uv run daw effect add projects/my-beat groups.drum-bus --type clipper
uv run daw plugin new 'Tape Warmth'                          # a device of your own to edit
uv run daw separate projects/my-beat ~/Music/song.m4a --split stems

uv run daw check projects/my-beat/song.yaml
uv run daw inspect projects/my-beat/song.yaml
uv run daw map projects/my-beat/song.yaml
uv run daw marker list projects/my-beat/song.yaml
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
The shared sample index is `library.sqlite` in the app data folder
(`~/Library/Application Support/AAW`, or `AAW_DATA_DIR`). `AAW_LIBRARY` overrides
it for both app and agent; `daw samples --db /path/index.sqlite ...` overrides
one command. `daw samples folders add DIRECTORY` registers and scans a source;
`folders list`, `folders refresh` and `folders remove DIRECTORY` manage those
sources. Repeat `samples search --folder DIRECTORY` to search several sources.
Existing `.daw/library.sqlite` indexes remain usable through an explicit override,
or add their source folders to the shared library.
Filename BPM/key/category are **hints**, not audio-derived facts. Names with C/F/etc.
do not establish an octave. `daw samples analyze` measures pitch with octave and
cents, onsets, loop tempo and one-shot/loop kind from the audio. `--root-note auto`
uses the measured note, and `daw check` warns when a declared root disagrees with
the audio. See [docs/features/sample-analysis.md](docs/features/sample-analysis.md).
It also measures what the sample sounds like in seven numbers, with words such
as bright, short and noisy that say where it lies among samples of its
category; `daw samples search --sort punch`, `--short --bright` and
`--max-decay 300` choose by them, and `daw samples like SAMPLE` lists the
nearest to one. A sample named for no category is found under the one it
measures as, a hat or a kick. See
[docs/features/sound-descriptors.md](docs/features/sound-descriptors.md) and
`daw describe samples`.
`daw samples import` takes a file's path as well as an index ID. An `.m4a` or
`.mp3` file is decoded once into the project as WAV, and a file the engine cannot
play is refused; see "Format" in [docs/architecture.md](docs/architecture.md).
`daw samples beats` maps a whole song: its tempo, every beat, the downbeats with
their alternatives, where the arrangement changes, the beats near a timecode and a
click audition to check the grid by ear. See [docs/features/beat-map.md](docs/features/beat-map.md).
To edit a finished song from timecodes, `daw describe edit`, `beats`, `joins` and
`export` say how. A skill for a kind of edit is personal and stays out of Git, with
the sounds it reuses; see [docs/features/skills.md](docs/features/skills.md).
A track's `audio` lists audio clips, parts of a sample file placed on beats, and
`daw audio` adds, moves, cuts, splits, trims and crossfades them: an edit of a song is one
clip with ranges cut out. See [docs/features/audio-clips.md](docs/features/audio-clips.md).
In the Mac app an audio file dropped on the timeline is such a clip, which is moved,
trimmed, faded and split there. See [docs/features/audio-clips-in-app.md](docs/features/audio-clips-in-app.md).
A pad or an audio clip with `source_bpm` and `stretch: preserve_pitch` follows the session's tempo at
its own pitch, so an edit can be made a few percent shorter by raising the tempo.
See [docs/features/time-stretch.md](docs/features/time-stretch.md).
`daw timeline` gives a song's places in beats and in seconds, where its sound ends,
and the beat a sound starts on to end on another. A song is as long as what is
in it: the host sets its length after every edit. See
[docs/features/timeline.md](docs/features/timeline.md).
`daw joins` checks a rendered edit of a song: whether the beat carries across each
join, whether the splice shows as a step, the level either side and the file's
length, with an excerpt of each join to hear. See [docs/features/join-checks.md](docs/features/join-checks.md).
`daw export` writes the song's render as a named WAV, AAC or MP3 file and reports
the level policy it applied. See [docs/features/export.md](docs/features/export.md).
Audition exports a short WAV for listening; it does not start playback automatically.

Edits are commands: `set` for any value by path, and verbs for tracks, groups,
returns, clips, audio clips, notes, instruments, the Synth, patterns, pads,
effects, racks, sends, lanes, sections, markers and ranges of bars (`daw --help` lists them,
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

The song has no length to write: it ends at the bar after its last sound. A
quarter beat grid means sixteenth notes. `x` triggers velocity 100, digits 1–9
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
gain and ending fade, but not master effects. Track stems are dry, and each group
and each return has its own stem; a grouped track's stem is its sound before its
group. Without master effects the stems of the ungrouped tracks, the outermost
groups and the returns together reconstruct the mix within quantization tolerance; `stems_sum_to_mix` in the report
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
A render's report also names the pairs of stems that overlap: the band, how much
of the time and where, with `daw listen <render> --overlap <a> <b>` for one pair
by band and section. See [docs/features/masking.md](docs/features/masking.md).
The mix and each stem are also measured in third octaves with a tilt, and each
stem's resonances are listed as a frequency, a height and a `q`, the numbers an
equalizer's bell takes; `daw listen <render> --section <id>` prints one
section's. See [docs/features/spectrum-detail.md](docs/features/spectrum-detail.md).
The report's `translation` says what the mix, each band and each stem loses
summed to mono and through the band a small speaker plays, and
`daw listen <render> --write-translation` writes the two as files to hear. See
[docs/features/translation-checks.md](docs/features/translation-checks.md).
The mix and each section have their loudness range and their loudest 3 s and
400 ms, each track's `hits` say how hard its hits land at its stem, through its
group and in the mix, and `effects` lists what each compressor, limiter and
clipper took off, over the song and in each section. See
[docs/features/dynamics-detail.md](docs/features/dynamics-detail.md).
See [docs/features/perception.md](docs/features/perception.md) for definitions and limitations.

`daw reference add <song> --name <name>` measures a song you point to as what
good sounds like, an `.m4a` or `.mp3` too, and keeps the numbers in the workspace
library; the audio stays where it is. `daw compare <render> --reference <name>`
then reports how the mix differs from it, whole and section by section: loudness,
tonal balance by band, crest, width, and how far each section sits over the
song's loudness. See
[docs/features/reference-comparison.md](docs/features/reference-comparison.md).

`docs/concept.md` describes the broader product vision. What is implemented is
deliberately narrower; `docs/architecture.md` is authoritative for the current build,
and `docs/backlog.md` lists what is next.

## License

AAW is open source under the [MIT License](LICENSE).
