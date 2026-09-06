# Specification

Status: design draft, nothing built. This is the technical companion to [idea.md](idea.md). Where a choice is settled, [decisions.md](decisions.md) records why. Where it is not, it is listed under open questions in [plan.md](plan.md).

Contents

1. Scope
2. Time model and session globals
3. Object model
4. Project document format
5. Pattern notation
6. Library and assets
7. Devices
8. Rendering
9. Perception
10. Audio operations
11. MIDI operations and theory library
12. CLI surface
13. Intent and memory
14. Working rhythm and autonomous mode
15. Technology stack
16. Workspace UI
17. Version one boundaries

---

## 1. Scope

A macOS-native, offline, agent-operated DAW. The agent edits a text project, runs commands that render and analyze, and iterates. A person listens to rendered artifacts and steers. No recording, no realtime audio engine, no third-party plugin hosting, no clip launching in version one.

The end state adds a visual workspace (section 16) as a second client of the same project, so a person can adjust tracks, devices, and knobs by hand and hear the result while the agent works alongside. The agentic loop is built first. Version one is shaped so the workspace can be added without changing the core, and the specific requirements it imposes are called out where they land: canonical form (section 4), single device implementation (section 7), region rendering (section 8), the `fmt` and `serve` verbs (section 12), and the library layering (section 15).

---

## 2. Time model and session globals

**Time is musical, always.** Every position and duration the agent reads or writes is in bars and beats. Sample and second conversions happen inside the renderer and analysis tools only.

Position syntax: `bar.beat.fraction`, one-indexed, e.g. `9.1` is the downbeat of bar 9, `9.2.5` is halfway through beat 2 of bar 9. Durations: `2 bars`, `1/8`, `3 beats`, `1/16t` for triplets.

Session globals, set early and rarely changed:

| Global | Notes |
|---|---|
| tempo | BPM. Constant in version one. Tempo automation reserved for later. |
| time_signature | e.g. `4/4`, `6/8`, `7/8`. Constant in version one. |
| sample_rate | 44100 or 48000 for final renders. Draft renders may downsample. |
| swing | Global groove amount applied to the grid, e.g. `56%`. Clips may override. |
| length | Total length in bars. Derived from the arrangement if omitted. |
| notes | Free prose. This is where tonal intent lives, e.g. "mostly F minor, chorus borrows from the parallel major." Never a constraint. |

There is deliberately no `key` field. See [decisions.md](decisions.md) D6.

**Sections** are named ranges on the timeline: intro, verse, drop, bridge, outro. They are the unit the agent thinks in when arranging and the unit perception reports on.

---

## 3. Object model

Vocabulary follows Ableton. Every object has a stable, human-readable `id` unique within its parent, so any parameter is addressable by a dotted path such as `bass.sub.filter.cutoff` or `drums.kit.pads.kick.sample`.

**Session.** Globals, sections, tracks, returns, master.

**Track.** Types: `audio`, `midi`, `group`, `return`, `master`. Fields: volume in dB, pan from -100 to 100, mute, solo, color (cosmetic), devices, clips, sends, automation, `group` parent. Groups sum their children and can carry devices.

**Clip.** Placed on a track at a position with a length.
- Audio clip: references a sample, with `start` offset into the sample, `gain`, `transpose` in semitones, `fade_in`, `fade_out`, `reverse`, `stretch` mode (`preserve_pitch`, `repitch`, `off`) and a `warp` block that records the source tempo and downbeat so the renderer can align it to the session grid.
- MIDI clip: references a pattern file or holds notes inline. Fields: `pattern`, `at`, `repeat` (a duration, tiles the pattern), `transpose`, `velocity_scale`, `swing` override.

**Pattern.** A reusable MIDI phrase in text notation (section 5), with its own length and grid. Defined once, placed many times.

**Device.** An instrument or effect in a track's chain, in order. Fields: `id`, `device` type, either a `patch` reference or inline parameters, `enabled`, `mix` (wet/dry where meaningful), `sidechain` input path where the device supports it.

**Parameter.** A named value with a unit, range, and default, declared by the device. Values in the document are always written with units: `800 Hz`, `-18 dB`, `10 ms`, `+7 st`, `1/4` (tempo-synced time), `50%`.

**Automation.** A list of breakpoint envelopes. Each has a `target` parameter path, a list of `[position, value]` points, and a `curve` per segment (`linear`, `exp`, `hold`, `smooth`). Automation overrides the static value while active. Any parameter anywhere is automatable, including track volume, pan, sends, and device parameters.

**Modulation.** Declared inside patches rather than on the timeline: a source (`lfo`, `envelope`, `velocity`, `note`, `random`) with its own parameters, an `amount` in the target's units, and a `target` parameter. Distinct from automation: automation is explicit over song time, modulation is a running process.

**Routing.** Tracks output to master by default, or to a group. Sends are per-track dB amounts to named returns, pre or post fader. Sidechain inputs reference any track or a specific drum pad. Routing must form a directed acyclic graph; the checker rejects cycles.

**Master.** A track with a device chain and no clips.

---

## 4. Project document format

Principles: plain text, diffs cleanly, readable in a model's context, real units, stable ids, references to files for anything large or binary. The whole song minus patterns and patches should fit in a few hundred lines.

Concrete syntax is an open question between YAML and a small purpose-built format. The sketches below use YAML for readability.

### Directory layout

```
my-song/
  song.yaml          session globals, sections, tracks, routing, automation
  brief.md           creative intent for this song (see section 13)
  journal.md         the agent's running log
  patterns/          MIDI patterns in text notation
  patches/           instrument and effect presets as text
  samples/           audio copied or linked into the project
  renders/           listenable artifacts, organized by kind (not versioned)
    clips/  tracks/  sections/  stems/  mix/  analysis/
  .cache/            render cache keyed by content hash (not versioned)
  .git/              every meaningful step is a checkpoint
```

### song.yaml sketch

```yaml
session:
  tempo: 87
  time_signature: 4/4
  sample_rate: 48000
  swing: 56%
  notes: "Dusty, loosely F minor. Vocal chop from the provided song is the hook."

sections:
  - { id: intro, start: 1.1,  length: 8 bars }
  - { id: verse, start: 9.1,  length: 16 bars }
  - { id: drop,  start: 25.1, length: 16 bars }
  - { id: outro, start: 41.1, length: 8 bars }

tracks:
  - id: drums
    type: midi
    volume: -3 dB
    pan: 0
    devices:
      - { id: kit,  device: drum_rack,  patch: patches/kit-dusty.yaml }
      - { id: comp, device: compressor, threshold: -18 dB, ratio: 4, attack: 10 ms, release: 80 ms }
    clips:
      - { pattern: patterns/drums-main.pat, at: 9.1,  repeat: 32 bars }
      - { pattern: patterns/drums-fill.pat, at: 24.1 }
    sends: { verb: -14 dB }

  - id: bass
    type: midi
    volume: -6 dB
    devices:
      - { id: sub,  device: synth, patch: patches/sub-round.yaml }
      - { id: duck, device: compressor, sidechain: drums.kit.pads.kick, threshold: -20 dB, ratio: 6, attack: 1 ms, release: 120 ms }
    clips:
      - { pattern: patterns/bass-a.pat, at: 9.1, repeat: 16 bars }
      - { pattern: patterns/bass-b.pat, at: 25.1, repeat: 16 bars }
    automation:
      - target: sub.filter.cutoff
        points: [[25.1, 200 Hz], [33.1, 2400 Hz], [41.1, 400 Hz]]
        curve: exp

  - id: vox
    type: audio
    volume: -8 dB
    clips:
      - sample: samples/hook-chop.wav
        at: 25.1
        length: 2 bars
        transpose: +2 st
        stretch: preserve_pitch
        warp: { source_tempo: 92, downbeat: 0.412 s }
    devices:
      - { id: eq, device: eq, patch: patches/vox-cleanup.yaml }
    sends: { verb: -6 dB, dly: -10 dB }

returns:
  - { id: verb, devices: [{ id: r, device: reverb, patch: patches/hall.yaml }] }
  - { id: dly,  devices: [{ id: d, device: delay, time: 3/16, feedback: 35%, mix: 100% }] }

master:
  devices:
    - { id: glue, device: compressor, threshold: -10 dB, ratio: 2, attack: 30 ms, release: auto }
    - { id: lim,  device: limiter, ceiling: -1 dBTP }
```

### Patch sketch

```yaml
device: synth
name: sub-round
osc:
  - { wave: sine, level: 0 dB }
  - { wave: triangle, level: -12 dB, detune: +3 ct }
filter: { type: lowpass, cutoff: 400 Hz, resonance: 10% }
amp_env: { attack: 5 ms, decay: 200 ms, sustain: 80%, release: 120 ms }
modulation:
  - { source: envelope, attack: 0 ms, decay: 150 ms, target: filter.cutoff, amount: +800 Hz }
  - { source: lfo, rate: 1/4, shape: sine, target: osc[1].detune, amount: 4 ct }
```

Every rendered artifact and every promoted asset gets a `provenance` block: which project, which document revision, which command, which seed.

### Canonical form

The document has exactly one serialization for a given content. The agent and the workspace will both rewrite the file constantly, and if their formatting differs, every git diff fills with reordering and quoting noise that the agent has to read past and that hides real changes. So:

- `daw fmt` rewrites a project into canonical form, and `daw check` fails on a file that is not canonical.
- Every tool that writes the document writes canonical form, including the workspace daemon.
- Writes are atomic: write to a temporary file, then rename. Every writer re-reads the document before modifying it, so two writers editing different fields do not clobber each other. Git catches anything that slips through.

This weighs on the syntax choice in plan.md: a purpose-built format has one formatter by construction, while YAML needs ruamel with a fixed style, key order, and quoting policy to be canonical.

---

## 5. Pattern notation

A note list is verbose and hard to hold in context. Patterns use a compact text notation the agent can read at a glance and edit like code. This is a new format by design; nothing existing was made for this reader.

### Step notation for drums and rhythmic parts

```
# patterns/drums-main.pat
length: 2 bars
grid: 1/16

kick   |x...x...x..x....|x...x...x.......|
snare  |....x.......x...|....x.......x..x|
hat    |x.x.x.x.x.x.x.x.|x.x.x.x.x.x.x.xx|
hat.v  |7.5.7.5.7.5.7.5.|7.5.7.5.7.5.7.55|
ohat   |......x.........|......x.........|
```

Rows name a drum rack pad. `x` is a hit at default velocity, digits 1 to 9 are velocity levels, `.` is rest. A `.v` row overrides velocity for the pad above. Optional `.n` rows for nudge in ticks, for hand-placed swing.

### Note notation for melodic parts

```
# patterns/bass-a.pat
length: 4 bars
grid: 1/8

1.1   F1   3/8   v100
1.2.5 F1   1/8   v80
2.1   Ab1  1/4
2.3   C2   1/4
3.1   F1   1/2
4.1   Eb2  1/8   v90
4.1.5 Db2  1/8
4.2   C2   1/4
```

Columns: position, pitch, length, optional velocity. Chords are several lines at the same position, or a chord symbol expanded by the theory library: `1.1  Fm7  1 bar  voicing: close`.

Both forms compile to the same internal note list. Import from and export to standard MIDI files is supported for interoperability, but the text form is canonical.

---

## 6. Library and assets

The library is the persistent, cross-project home of everything reusable. It lives outside any one project, at a path recorded in the profile.

### Samples

A sample is a first-class analyzed object, not just a file. On import, once, each sample gets:

| Field | Purpose |
|---|---|
| duration, channels, sample rate | basics |
| loudness, peak, crest factor | gain staging and selection |
| root pitch and confidence | pitched samples map to the keyboard correctly |
| tempo and downbeat, if a loop | warping to session tempo |
| transient positions | slicing, and "is this one-shot or loop" |
| spectral profile and centroid | search by brightness, weight |
| category guess | kick, snare, hat, vocal, bass, pad, fx, loop, full song |
| spectrogram thumbnail | visual inspection |
| embedding | text and similarity search, e.g. "dark punchy kick" |
| tags and notes | user or agent annotations |
| provenance | where it came from, or the recipe that generated it |

Search is a command that combines text-to-embedding similarity with filters on the fields above, returning a short ranked list the agent can act on.

### Patches, racks, patterns

Text files, same format as inside a project, stored in the library with tags and provenance. A rack is a saved device chain. A drum rack patch maps pads to samples with per-pad tuning, envelope, and choke groups.

### Promotion

`daw promote <artifact>` copies a render, a slice, a patch, or a pattern from a project into the library with its provenance and any tags. This is how "keep that sound" works. A generated sound carries the patch and seed that made it, so it can be regenerated or varied later.

### Tempo policy on import

- Session has no tempo yet: adopt the detected tempo of the imported material and ask the person to confirm when confidence is low or a half or double time ambiguity exists.
- Session has a tempo: warp the imported material to it, preserving pitch by default. Record the source tempo and downbeat on the clip so the choice can be changed later.
- One-shots are never stretched.

---

## 7. Devices

All devices are in-house. Each device ships with a self-description the agent reads before use: what it does, every parameter with unit, range, default, and guidance on typical settings. `daw describe compressor` prints it.

### Version one instruments, in priority order

1. **Sampler.** One sample per instance. Root note, transpose, fine tune, start and end, loop points and mode, amp envelope, filter with envelope, velocity to amp and filter, glide. The workhorse for chops and pitched samples.
2. **Drum rack.** Many pads, each a sampler with its own sample, tuning, envelope, gain, pan, choke group, and output routing so pads can be processed separately or sidechained from. Pads are named, e.g. `kick`, `snare`, `hat`, or `s01` to `s16` after slicing.
3. **Synth.** Subtractive, multi-oscillator, with noise, filter types, envelopes, and the modulation system. Enough to design kicks, basses, pads, leads, and plucks from scratch. FM and wavetable are later additions.

### Version one effects

utility (gain, pan, width, phase, mono below), eq (parametric, several bands, with shelves and high and low pass), compressor (with sidechain input and optional filter on the key), limiter (true peak), reverb, delay (tempo-synced, feedback, filter in loop, ping pong), filter (as an effect, for automation sweeps), saturation, chorus, phaser. Multiband compressor, transient shaper, gate, and pitch shift follow.

### Modulation sources

lfo (rate free or tempo-synced, shapes, phase, retrigger), envelope (ADSR, per note), velocity, note number, random (seeded, per note or sample and hold). Any parameter of the device that owns the patch can be a target.

### Determinism

Every device is deterministic given its inputs, parameters, and a seed. No free-running state that would differ between renders.

### Single implementation

Each device exists exactly once. The code that renders offline is the code any future responsive-preview path or realtime engine runs, so a person adjusting a knob in the workspace and the agent analyzing a render always hear identical audio. If they ever hear different things, the perception loop cannot be trusted. A second implementation of a device for the UI is forbidden, including approximations such as browser audio nodes standing in for the real effect while a knob is dragged.

To keep the realtime door open without paying for it now, devices are written in a block-processing shape: explicit state carried between calls, one buffer of samples processed per call, and no operation that assumes the whole track is in memory at once. This is somewhat slower offline than whole-array vectorization, and it is the price of one implementation. The language the devices are written in is a separate decision, recorded in section 15.

---

## 8. Rendering

Offline, deterministic, cached.

- **Graph evaluation.** The session routing is a DAG. Tracks render in dependency order so sidechain sources and groups are available when needed.
- **Targets.** A clip, a pattern in isolation, a track, a section, a section of one track, a region between two positions, all stems, the mix. `daw render drums`, `daw render section drop`, `daw render mix`, `daw render stems`, `daw render mix --region 25.1..29.1`.
- **Region rendering.** A region render evaluates only the bars between two positions, on one track or the mix, and reuses the track cache for anything unaffected. The workspace daemon uses it to re-render a few bars around the playhead when the person changes something, which is what makes editing feel responsive without a realtime engine. The agent uses it to audition a change cheaply before committing to a full render.
- **Caching and freezing.** Each track render is keyed by a hash of everything that affects it. Unchanged tracks are never re-rendered. This is what makes a full song cheap to iterate on.
- **Draft and final.** Draft mode uses a lower sample rate, no oversampling, and cheap reverb settings for speed. Final mode renders at full quality. Perception reports state which mode produced them.
- **Seeds.** Anything random (humanize, random modulation, round robin) takes a seed from the document. Same document, same audio.
- **Performance target.** A four-bar loop in draft mode renders in well under one second. A full song in final mode in under a minute on an Apple Silicon laptop. This target constrains every device implementation.
- **Output conventions.** `renders/<kind>/<name>.wav`, with the mix versioned by document revision so the person can compare `mix/v12.wav` and `mix/v13.wav` by ear.

---

## 9. Perception

The agent cannot hear. Perception is a set of tools that turn audio into compact reports and images designed for a language model to reason about. Cheapest first.

### Descriptors

Per render, per section, and per track: integrated and short-term loudness, true peak, crest factor, spectral balance in named bands (sub, low, low mid, mid, high mid, high, air) in dB, spectral centroid, stereo width and correlation, whether the low end is mono, transient density and sharpness, silence and clipping detection, detected tempo and downbeat, chord and key inference as a hint.

### Relational analysis

Analyses that humans do by ear and an agent does better by computation:

- **Masking** between pairs of tracks in named bands, with the bars where it is worst.
- **Diff** between two renders: what changed in every descriptor, and where.
- **Reference comparison**: every descriptor as a delta against a reference track from the profile or the brief, section-aligned where possible.
- **Structure**: energy and spectral contour per section, so the agent can confirm the drop is louder and brighter than the verse.

### Images

Spectrograms drawn for reading, not for decoration: log frequency, bar and beat gridlines, bar numbers on the axis, one image per section or per few bars, optional overlay of two tracks for masking inspection, and waveform strips with section markers for a whole-song overview. Written to `renders/analysis/` and referenced from the report so a multimodal agent can open them.

### The report

`daw listen <render>` prints a report shaped like this, short enough to read in one glance:

```
mix/v13.wav   final   64 bars @ 87 BPM   2:56
loudness   -11.2 LUFS   peak -0.4 dBTP   crest 9.1 dB
balance    sub +1.8  low +2.1  lowmid -0.4  mid -1.0  himid -1.6  high -2.8  air -4.0   (dB vs reference)
stereo     width 0.61   corr 0.92   mono below 120 Hz: yes
masking    kick/bass 60-120 Hz heavy in drop (bars 25-40)
sections   intro -9.4 dB vs drop   verse/drop contrast 6.2 dB   outro tails to silence at 48.3
issues     none clipping   bass silent bars 1-8 (intentional?)
images     analysis/mix-v13-drop.png  analysis/mix-v13-overview.png
```

Perception never sends audio to a model. Optional audio-input model critique is a later, separate command.

---

## 10. Audio operations

Commands that compute on audio. These adopt existing open source where it is clearly best.

- **Tempo and downbeat detection.** Returns candidates with confidence, including half and double time alternatives.
- **Time stretch** preserving pitch, and **pitch shift** preserving duration. Rubber Band.
- **Repitch** as a deliberate creative choice, changing speed and pitch together.
- **Stem separation** of a provided song into vocals, drums, bass, other. Demucs. Output stems become library samples with provenance pointing at the source.
- **Slicing** at transients or on a grid, producing a set of samples and a drum rack patch mapping them to pads, plus a pattern that replays the original in order.
- **Clip editing**: trim, split, fade, crossfade, reverse, gain, normalize, consolidate.
- **Pitch detection** of a sample's root note.
- **Warp markers** for drifting-tempo material are a later feature. Version one handles constant tempo with a global ratio and downbeat alignment.

---

## 11. MIDI operations and theory library

**Transformations** on patterns or clips: transpose, quantize with strength, humanize timing and velocity with a seed, velocity curves, note length scaling, legato, arpeggiate, strum, invert, retrograde, and pattern variation (drop hits, add ghost notes, shift accents).

**Theory library** as a tool, never a constraint: scale and mode lookup, chord construction and voicing, chord symbol expansion, progression suggestions given a mood, key and chord inference over a pattern or an audio clip, and interval and tension queries. The agent may write anything it likes; the library is there when it wants help.

---

## 12. CLI surface

The rule: the agent edits the document directly for anything that is state, and runs the CLI for anything that computes. This keeps the tool surface small and plays to what a coding agent does best.

| Verb | Purpose |
|---|---|
| `daw check` | Lint the project: missing files, out-of-range parameters, routing cycles, clips past song end, unknown ids. Fast, before every render. |
| `daw render <target>` | Render a clip, pattern, track, section, stems, or mix. Draft by default, `--final` for full quality. |
| `daw listen <render>` | Perception report for a render, optionally against a reference. |
| `daw compare <a> <b>` | Diff two renders. |
| `daw describe <device>` | Print a device's self-description. |
| `daw search <query>` | Search the library with text and filters. |
| `daw import <file>` | Bring a sample or song into the library or project, run analysis, apply the tempo policy. |
| `daw analyze <file>` | Tempo, downbeat, pitch, structure of any audio. |
| `daw separate <file>` | Stem separation. |
| `daw slice <sample>` | Slice into a drum rack and pattern. |
| `daw stretch <sample>` | Time stretch or repitch to a tempo or ratio. |
| `daw midi <op>` | Pattern transformations and theory queries. |
| `daw promote <artifact>` | Copy something worth keeping into the library with provenance. |
| `daw export <preset>` | Mixdown, stems, MIDI, with named recipes such as a streaming loudness target. |
| `daw log <text>` | Append to the project journal. |
| `daw checkpoint <msg>` | Git commit the project with a message. |
| `daw fmt` | Rewrite the project document in canonical form. |
| `daw serve` | Start the long-lived project service the workspace UI talks to: watches the project directory, keeps the render cache warm, streams render progress, serves the document and waveform peaks. |

The CLI is machine-first: terse, structured output, non-zero exit on problems, no interactive prompts. A `--json` flag on every verb, and long-running verbs stream progress as newline-delimited JSON events under `--json`.

The daemon is never required. When one is running for the project, the CLI hands renders to it so both clients share one cache and the workspace sees progress. When none is running, the CLI does the work itself. The agent should not notice the difference.

---

## 13. Intent and memory

Three tiers, distinguished by who may write them.

| Tier | Written by | Scope | Purpose |
|---|---|---|---|
| Rules | the person only | global | Hard constraints the agent never edits. "Never use sidechain pumping." "Ask before changing tempo mid-song." |
| Preferences | the agent, editable by the person | global, optionally scoped by genre or context | Learned taste and workflow, each with evidence and a date. |
| Brief | both | per project | The idea, references, mood, and a running record of what the person said they liked and disliked about this song. |

Precedence: rules, then brief, then preferences, then defaults. The agent must be able to say which tier drove a decision.

### The profile

Lives in a user-level directory the DAW owns. Contains rules, preferences, a reference set (songs the person points to as "this is what good sounds like," analyzed once, feeding the perception layer's deltas), a house kit (favorite patches, racks, a default session template with usual routing and mastering chain), workflow preferences (how often to check in, how much to render before pausing, checkpoint cadence), and library paths.

### Learning guardrails

- The agent records preferences as it goes from explicit signals: promotions, rejections, direct feedback, hand edits the person makes to the document. It writes only to the preferences file, never to rules.
- Every preference carries evidence and a date. Preferences decay unless reconfirmed.
- At the end of a session the agent states what it learned in a sentence or two. No silent adaptation.

### Harness boundary

The DAW owns its own profile and brief files in its own format, so the same intent works under Claude Code, Codex, or anything else. The harness instruction file points at them. Anything about music belongs to the DAW; anything about how the person likes to work with an agent in general belongs to the harness. Keeping this line clean avoids two memories learning contradictory things.

---

## 14. Working rhythm and autonomous mode

**Every step leaves a listenable artifact.** Renders land in `renders/` as they are made, named by kind and id, so the person can listen while the agent keeps working.

**Journal.** `journal.md` holds a short running log of what the agent is doing and why. The person reads the last few lines instead of reconstructing the agent's thinking.

**Checkpoints.** Git commit after each meaningful step. Song versions are branches. The document and the cache are always consistent at a checkpoint, so interruption at any moment is safe.

**Interruption.** The harness delivers mid-task messages. The agent treats them as brief updates: "keep that sound" becomes a promotion, "I don't like this part" is recorded in the brief with the section and the reason, and work continues.

**Autonomous mode.** "Make a song and don't stop until done" runs a producer's loop: read rules, profile, and brief; plan sections and instrumentation; build one section at a time; render; listen; critique against the definition of done; revise; move on; master; export. Skills encode the reusable craft at each step.

**Definition of done.** A checklist the agent grades a song against, kept in a skill and adjustable per brief: arrangement complete with all sections filled, no clipping, loudness within target, spectral balance within tolerance of the reference, sections audibly distinct by energy and content, no unresolved notes in the brief, stems and mix exported.

---

## 15. Technology stack

Version one in Python for speed of building. The document format and CLI are the stable contract; the engine behind them can be ported later.

### Layering

One core library, `daw`, holds the document model, the renderer, the devices, the perception tools, and the library index. Two thin shells sit over it: the CLI the agent uses, and the `daw serve` daemon the workspace uses. Neither shell contains logic the other would need. This is what keeps the workspace from becoming a second implementation of anything.

| Need | Choice | Why |
|---|---|---|
| numeric DSP | numpy, scipy | vectorized offline rendering is fast enough for the target |
| audio I/O | soundfile | WAV, FLAC, AIFF |
| time stretch, pitch shift | Rubber Band via pyrubberband | near state of the art, not worth rebuilding |
| stem separation | Demucs | solved by deep learning, clearly optimal |
| analysis | librosa, essentia, pyloudnorm | tempo, onsets, pitch, spectral features, loudness |
| embeddings for sample search | a CLAP-family audio-text model, run locally | text search over samples |
| MIDI import and export | mido | interoperability only; text notation is canonical |
| images | matplotlib | spectrograms with gridlines |
| project format | YAML via ruamel, or a custom parser | open question; must be canonical either way |
| versioning | git | checkpoints and branches |
| project daemon | Python, websockets, watchfiles | same process as the core library, shares the cache |
| workspace frontend | TypeScript, canvas timeline, Web Audio for playback | web is the best cross-platform UI toolkit; wraps into a desktop app later |
| workspace shell | browser tab served by the daemon first, Tauri later | Tauri is a Rust shell, so a Rust engine could later live in-process |

### Device language

Python remains in the project regardless. Demucs, CLAP, librosa, and the analysis layer are never leaving it. The open decision is only where device DSP lives.

Python devices in the block-processing shape serve the offline renderer and the responsive-preview path. They cannot serve a true realtime engine: a callback thread with a few milliseconds of budget cannot tolerate the interpreter or the GIL. If the end state ever needs live instrument playing, devices must be in a language that runs both offline and in a callback, and Rust with Python bindings is the candidate D18 already names. Because devices must exist exactly once, this decision is made before phase 3, where effects, the bulk of device code, get written. Until then the block-processing shape is the hedge.

Later: port the render hot path to Rust if the performance target is missed, keeping the document and CLI unchanged.

All in-house devices are written from scratch in the project, with unit tests that assert deterministic output for fixed inputs and seeds.

---

## 16. Workspace UI

The workspace is the end state: a visual DAW in the style of Ableton, Logic, or Pro Tools where a person adjusts tracks, volume, panning, devices, and knobs by hand and hears the result, while the agent works in the same project. The person watches the agent build in real time, plays back what it has made as it goes, and steps in for the small things. It is built after the agentic loop and in parallel with the numbered phases, never ahead of them.

### Principles

- **A second client of the document, never a second source of truth.** The workspace reads `song.yaml` and writes `song.yaml`, through the same canonical serializer the agent's tools use. It holds no state the file does not hold. Every edit made in the workspace is visible to the agent as a file change and a git diff.
- **Watching the agent is file watching.** The agent edits the document, the workspace redraws. The agent renders a track, the workspace draws the waveform. There is no channel between the agent and the workspace other than the project directory.
- **Hand edits are signals.** Section 13 already treats hand edits to the document as evidence for preferences. The workspace makes those edits easier to make. Nothing else changes.
- **Same sound for everyone.** The workspace never runs an approximation of a device. Playback is rendered audio, and any audible change goes through the real renderer. See section 7.
- **The agent lives in the terminal.** The workspace is a window into the project, not a chat client. Embedding the agent in the app is possible later through the agent SDK and is not required.

### What it shows

The arrangement: tracks, clips with waveforms from the render cache, sections, automation lanes, the playhead. Track strips with volume, pan, mute, solo, and sends. Device chains with panels generated from each device's self-description, so every knob has the right unit, range, and default without a hand-built UI per device. A step grid and piano roll that read and write the pattern notation. The journal, the brief, and the git history. The perception layer's reports and images for the current render.

### Playback model

"Realtime" is three different things, and the workspace needs the first two.

1. **Playback of rendered audio.** Transport, playhead, scrubbing, looping a section. The player loads the track cache, which is the stem export, and applies mute, solo, volume, and pan in Web Audio instantly with no re-render. Those four cover most of what a person reaches in to change during a mix. This path skips the master chain, so it is a pre-master monitor; the daemon re-renders the true mix in the background and the player swaps to it when it lands.
2. **Responsive editing.** Any other change, a filter cutoff, a compressor threshold, a moved clip, writes the document. The daemon region-renders the affected track for a few bars around the playhead and the player swaps the audio in. With the draft performance target this is a few hundred milliseconds of latency: not realtime in the audio-engineer sense, but responsive enough to mix by ear, and the same audio the agent will analyze.
3. **Live instrument playing.** A keyboard into a synth with no perceptible latency needs a callback-driven engine and a device language that can run in it. Recording and live performance are out of scope for version one, so this is an end-state question, decided with the device language before phase 3.

### Stages

Each stage is small enough to stop after, and lower priority than any numbered phase.

- **U0, read-only.** Arrangement view, waveforms, transport, playhead, journal and brief panes, live updating as the agent works. Available once phase 0 exists.
- **U1, simple edits.** Mute, solo, volume, pan, sends, move and trim clips, delete. Writes the document, triggers a re-render.
- **U2, panels and editors.** Generated device panels, step grid, piano roll, automation editing.
- **U3, responsive editing.** Region rendering around the playhead, audio swap in the player.
- **U4, realtime engine.** Only if live instrument playing becomes a goal. The only stage that touches the core.

### What it is not

Not a place to build whole songs by hand. Ableton exists for that. If the workspace starts growing features that only make sense for a person composing from scratch, the design has drifted from the product, which is the agent.

---

## 17. Version one boundaries

In: everything above marked version one. Out until later: tempo and time signature changes, warp markers, third-party plugin hosting, the workspace UI and its daemon (designed for from the start, built as a parallel track after phase 0), a realtime audio engine, FM and wavetable synthesis, audio-input model critique, Ableton project import or export, recording.
