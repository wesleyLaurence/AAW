# Rust + Swift update: build plan for a native real-time DAW

Status: adopted September 30, 2026; M0 to M3 done, and the M2 spike and M3 exit tests passed (see [Progress](#progress)). It replaces the workspace track (U0–U4) in [plan.md](plan.md), and decisions D33 to D40 in [decisions.md](decisions.md) record the changes listed in [Decisions to record](#decisions-to-record); D41 records M3's choices. [mvp.md](mvp.md) remains authoritative for what exists today.

## Progress

- **M0:** Cargo workspace under `engine/`; decisions D33 to D40 recorded. Xcode 16.2 is still to be installed, before M4.
- **M1:** `aaw-model` matches `model.py` on 12,532 generated documents and on every local song: the same documents are accepted, canonical YAML is byte-identical, and `project_sha256` and the legacy fingerprints are equal. Rejections carry pydantic's locations, types and messages. Known differences, all in input no tool writes, are listed in [engine/README.md](../engine/README.md).
- **M2:** The Rust sampler engine renders effect-free songs byte-identically to the Python engine: 199 of 199 files over 60 generated songs, and 52 of 53 files over four local 10–15 track songs with their effects removed (the other within 1.8e-12). Playback of a 15-track song at 128-frame buffers on the built-in speakers used at most 0.05 ms of each 2.67 ms callback, with no xruns; the headless benchmark runs about 500 times faster than real time. Playing from beat 32 picks up sounding samples and equals the same frames of a full render. The plan proceeds as written.
- **M3:** The session host (`aaw-host`) holds the song in memory, applies the command catalog below (plus `remove`, `get` and `return rename`), keeps undo, redo and a change log with origins, gives clips, effects, events and points `@N` handles, saves `song.yaml` after each command, loads external edits as undoable steps and registers a per-user socket. With no host running, commands run headless. Exit test on the same 15-track song at 128-frame buffers: while `daw play` played from beat 32, a second process ran `daw set tracks.drums.gain_db -6` and later `daw undo`. Each took about 20 ms end to end, including recompiling and saving; both new versions reached the audio thread; `song.yaml` changed and was restored; and `daw changes` listed both with origin `agent`. The 25 s of playback had no xruns, and the slowest callback took 0.05 ms. Whether the 2 dB drop was audible is left to the person to judge. A stress test swaps in 140 random edits while 1,127 blocks render on schedule, with no allocation on the audio thread. Saving before each reply, the 5 ms crossfades and headless limits are recorded in D41. A debounce, gesture coalescing and parameter messages that skip recompiling wait for drags in M5.

## Goal

A native macOS DAW in the spirit of Ableton Live's Arrangement View, which a person and an agent operate together:

- Tracks are stacked vertically with clips laid out left to right on a timeline. Clicking a track opens its device chain, as Ableton's Device View does.
- Click a position, press space, and the song plays from there. Move a clip or turn a knob and hear the result immediately, during playback or on the next play.
- The agent operates the same machinery through its own tools: it adds tracks, moves clips, sets values and starts playback. With the window open, the person sees each change happen as the agent makes it.
- Work alternates freely: the person edits, asks the agent for something, edits more, asks again. Both share one song, one undo history and one sound.

The agent's ability to operate the DAW natively is the differentiator. Parity with Ableton's full feature set is not a near-term goal; see [Scope](#scope).

## Why the current architecture cannot get there

The MVP renders offline. `daw render` computes the whole song into WAV files, which took 4–6 s for 14-track local projects and 27–34 s for a 15-track project with reverb and automation. A cold `daw inspect` takes about 3 s, most of it interpreter and library start-up.

Pressing space and hearing edits right away needs a real-time engine. The audio hardware requests a small buffer every few milliseconds (128 frames at 48 kHz is 2.67 ms) and a late buffer is an audible dropout. Python cannot meet that deadline reliably: the interpreter, the GIL and garbage collection all pause unpredictably, and per-sample loops are slow. D25 already recorded that a real-time engine would require rewriting the devices in another language.

## Summary of the approach

1. Rewrite the song model, sampler, effects, automation and mixer in Rust as one engine that serves both real-time playback and offline export.
2. Put a session host in front of it: an in-memory authority for the song that applies edit commands one at a time, keeps undo history and saves `song.yaml` continuously.
3. Build a native Swift app (SwiftUI with AppKit) that embeds the Rust core in-process and draws the arrangement, mixer and devices.
4. Give the agent a `daw` CLI that sends the same commands to the running app, or runs the core headless when the app is closed.
5. Keep Python for sample search, sample analysis and the perception tools (`daw listen`, `daw compare`).
6. Port against the existing Python engine as the reference: parity tests must pass before the Python renderer is retired.

## Why Rust

Rust compiles to native code with speed comparable to C++ and has no garbage collector, so nothing pauses the audio thread. For audio, never missing a deadline matters more than raw speed. The compiler rejects most memory-safety errors and data races, the bugs that crash audio applications, which matters when an agent writes much of the code. Cargo gives one build and test tool. The DSP ecosystem covers what the port needs: FFTs, lock-free queues, audio I/O and bindings for both Swift and Python.

The alternative considered is C++ with JUCE, the industry standard; Ableton and Bitwig are C++. JUCE's clearest advantage is mature hosting of third-party AU and VST3 plugins. AAW builds its devices in-house (D3) and D18 already named Rust as the escape hatch, so Rust is recommended. Revisit if third-party plugin hosting becomes a requirement; on macOS, AU hosting remains possible from the Swift side through AVFoundation.

## Target architecture

```
  Mac app (Swift, SwiftUI + AppKit)            agent in a terminal
  arrangement, mixer, device view,             daw set / move / add / play
  transport, space bar, drag edits                     │
         │ in-process calls (UniFFI)                   │ local socket
         ▼                                             ▼
  ┌──────────────────── session host (Rust) ─────────────────────┐
  │ in-memory song · command log · undo/redo · change feed       │
  │ revision + project_sha256 · autosave · external-edit reload  │
  └───────────┬───────────────────────────────┬─────────────────┘
              │ compiled program              │ canonical YAML, atomic write
              ▼                               ▼
  ┌──────── engine (Rust) ─────────┐     song.yaml, samples/, renders/
  │ scheduler · sampler · devices  │
  │ automation · routing · mixer   │     Python (unchanged role):
  │ real-time driver (CoreAudio)   │     samples scan/search/analyze,
  │ offline driver (WAV, stems)    │     listen, compare
  └────────────────────────────────┘
```

When the app is closed, the `daw` CLI links the same host and engine and works headless against the file, as the CLI does today.

### Components

| Component | Language | Responsibility |
|---|---|---|
| `aaw-model` | Rust | Schema v1 types, validation, exact beat arithmetic, canonical YAML, `project_sha256` and legacy fingerprints, JSON merge patch |
| `aaw-dsp` | Rust | Sampler voices, resampler, filter, EQ, automated SVF, compressor, limiter, delay, reverb, envelopes |
| `aaw-engine` | Rust | Compiles a song into a program; scheduling, routing, sidechain order, sends and returns, master chain; real-time and offline drivers |
| `aaw-host` | Rust | Session authority: command application, undo/redo, change feed, autosave, socket server, headless mode |
| `aaw-cli` | Rust | The `daw` binary: JSON in and out, talks to a running host or runs one headless; forwards Python-only subcommands |
| `aaw-ffi` | Rust | UniFFI bindings the Swift app calls |
| `aaw-py` | Rust | PyO3 module so Python analysis code loads and validates projects through the Rust model |
| `apps/mac` | Swift | The app: windows, views, controls, keyboard handling |
| `src/agent_daw` | Python | Library index, sample analysis, perception; the engine modules are deleted after cutover |

Swift holds no model logic of its own: it displays what the host reports and sends commands. There is still exactly one implementation of the document (D23, D24) and of each device (D22).

## The shared song: session host and commands

### Who owns the song

When the app has a project open, the session host inside the app is the authority, as Ableton's in-memory set is. Every edit, from the UI or the agent, is a command applied by the host on one thread in arrival order. The host writes canonical `song.yaml` shortly after each change, so the file, `git diff` and anything reading the file stay current.

When no app is running, `daw` starts a headless host in its own process, applies the command, saves and exits, under the existing `.daw.lock` advisory lock. The agent's commands behave the same in both modes.

A running host registers the project it owns in a per-user registry under `~/Library/Application Support/AAW/`, with the socket beside it. Sockets and registry stay out of project directories because projects may live in synced folders. The CLI checks the registry, connects if a live host owns the project and otherwise runs headless. A stale registration, whose socket refuses connections, is removed.

### What is and is not saved

All musical state lives in `song.yaml`: nothing about the sound exists only in the app. Ephemeral state lives in the host and is never saved: playhead, play state, loop brace, selection, zoom, scroll, undo history and the in-memory command log. The agent can still read ephemeral state through `daw status`, so a request such as "make the selected clip quieter" works.

### Commands

Every change is a command. Each command validates against the whole resulting song before it applies, so the song is always valid. Each carries an origin (`user`, `agent` or `external`) and produces a new revision number and `project_sha256`.

Addressing follows the automation parameter syntax the agent already knows:

- `session.tempo`, `session.length_beats`, `session.master_gain_db`
- `tracks.drums.gain_db`, `tracks.drums.pan`, `tracks.drums.mute`
- `tracks.drums.sends.plate.gain_db`
- `tracks.drums.effects.sweep.cutoff_hz`, `returns.plate.effects.0.decay_seconds`
- `tracks.bass.pads.sub.release_ms`

Schema v1 has objects without IDs: clips, effects without `id`, pattern events and automation points. The host assigns each one a handle that stays stable for the session and appears in `daw inspect` output while a host runs. Commands can address a clip by handle, so the agent's "move clip 7" still means the same clip after the person deletes an earlier one. The file format does not change.

Initial command catalog:

| Area | Commands |
|---|---|
| Values | `set PATH VALUE` for any scalar field; `toggle` for booleans |
| Tracks and returns | `track add`, `track remove`, `track rename` (updates sends, sidechains and lanes that reference it), `track move`, `return add`, `return remove` |
| Clips | `clip add`, `clip move` (track and/or beat), `clip repeats`, `clip duplicate`, `clip remove` |
| Patterns | `pattern add`, `pattern steps`, `pattern event add/remove/set`, `pattern duplicate` |
| Pads and samples | `pad add`, `pad set`, `pad remove`; sample import stays in Python and registers through the host |
| Effects | `effect add` (owner, type, position, parameters), `effect remove`, `effect move`, `effect bypass` |
| Sends | `send set`, `send remove` |
| Automation | `lane set` (whole lane), `lane point add/move/remove`, `lane remove` |
| Sections | `section add`, `section move`, `section remove` |
| Transport | `play [--from BEAT]`, `stop`, `locate BEAT`, `loop BEAT LENGTH`, `loop off` |
| History | `undo`, `redo`, `batch FILE` (several commands as one atomic, single-undo step) |
| Reading | `inspect`, `status`, `changes --since REVISION` |

`daw apply PATCH --expect SHA` remains for whole-document edits and keeps its stale-revision check. `daw changes --since REVISION` returns the command log with origins, so the agent sees what the person did since its last look as a list of operations rather than inferring it from a file diff.

### Concurrent edits

The host serializes commands, so no write can be lost. Commands addressed by ID or handle commute: an agent edit to the bass and a person's fader move on the drums both land. When both change the same field, the later command wins and both appear in the undo history. `--expect` stays available for agent edits that must not apply over an unseen change.

A drag in the UI sends throttled `set` commands (about 30 per second) while the mouse moves and a final one on release. The host merges consecutive `set`s from one gesture into one undo step.

### External edits

The host watches `song.yaml`. When the file changes and differs from the host's last write, for example from a raw text edit or `git checkout`, the host reloads it as a single `external` command that can be undone. An invalid file is not loaded; the app shows the validation error and keeps the last valid song until the file is fixed. Raw editing stays supported, but commands are the recommended path while the app is open, and AGENTS.md will say so.

### Canonical form and fingerprints

The Rust model must reproduce today's outputs exactly so that existing projects, render reports and the agent's `--expect` SHAs keep working:

- **YAML:** byte-identical to the current PyYAML emitter: key order, flow-style rules, quoting and 110-column wrapping. Beat values keep their written form (integer, decimal or fraction string).
- **`project_sha256`:** SHA-256 of Python's `json.dumps(..., sort_keys=True)` of the model with defaults and nulls omitted. This needs Python's separators, ASCII escaping and float `repr` rules.
- **Legacy fingerprints:** the `LEGACY_FIELDS` forms, so reports from earlier engines still verify.
- **Exact time:** decimals are converted through their shortest decimal representation, as `Fraction(str(value))` does, and frames round half up from absolute time.

A corpus of generated fixture projects pins all four: Python and Rust must produce identical YAML and hashes for every fixture. If byte-identical YAML proves impractical, the fallback is a one-time reformat of each project, recorded as a decision. The hash must match regardless.

## The engine

### Threads

- **Main thread (Swift):** drawing and input.
- **Host thread:** applies commands, publishes changes, schedules compiles and saves.
- **Worker pool:** decodes samples, prepares repitched audio, generates reverb impulse responses, compiles programs, runs offline renders and computes waveform peaks.
- **Audio thread:** CoreAudio's callback. It only reads the current program and processes one buffer.

### Real-time rules

The audio thread never allocates, frees, locks, blocks, performs I/O or logs. Debug builds enforce the allocation rule with `assert_no_alloc`. Voices, buffers and device state are preallocated. Messages to and from the audio thread go through lock-free ring buffers such as `rtrb`. Replaced programs are returned to a worker thread to be freed.

### Program compilation

A song is compiled off the audio thread into an immutable program: scheduled triggers per track, prepared sample buffers, device chains with their parameters, automation envelopes and the processing order (sidechain sources first, then the other tracks, returns and master). The audio thread swaps to a new program between buffers.

Compilation is incremental. An edit rebuilds only the tracks it affects, and devices whose type and position are unchanged keep their state, so moving a clip does not cut off a reverb tail. Parameter-only changes, such as a fader or a knob, skip recompilation and go to the audio thread as parameter messages.

Live parameter changes are smoothed over a few milliseconds to avoid zipper noise. Automation written in the document keeps its current exact semantics, without smoothing. An offline render reflects the saved values, not how a knob moved while it was dragged.

### Transport

The transport supports play from any beat, stop, locate while playing, and a loop region. The playhead position is published through an atomic value and displayed after compensating for output latency.

Starting mid-song chases sample voices: a hit whose trigger precedes the playhead but whose sound continues past it starts partway through the sample. Effect state starts empty at the playhead, so reverb and delay tails from before it are absent, as in other DAWs. An offline render always processes from the beginning.

### One engine for playback and export

The offline driver runs the same program with fixed blocks as fast as the machine allows and writes the mix, stems, snapshot and `report.json` in today's formats, so `daw listen` and `daw compare` keep working. The real-time driver calls the same processing with whatever buffer size CoreAudio requests. The Python engine is already block-partition invariant and tests prove it; the Rust engine keeps that property and those tests. That is what makes a performance played from the start equal to its offline render.

### Latency compensation

Each chain reports its latency. The engine delays shorter paths so all tracks and returns align, as the offline chain compensates today. In real time, total latency delays what the person hears after pressing play, so it must stay small:

- **Reverb:** today's uniformly partitioned convolution uses partitions of 4096–65536 frames, which is up to 1.4 s of latency at 48 kHz. The Rust reverb uses non-uniformly partitioned convolution: a short first partition, processed directly or with a small FFT, then growing partitions. Latency drops to at most one small block, and the result is the same convolution.
- **Limiter:** look-ahead latency remains and is compensated.

### Audio I/O and sample rates

Audio output uses `cpal` over CoreAudio, with device and buffer-size selection. The engine opens the device at the session rate (44.1 or 48 kHz) when the device allows. Otherwise the real-time output is resampled to the device rate, for example with `rubato`. Offline renders always use the session rate.

### Sample loading

Samples are preloaded and prepared in memory, as today. Decoding uses libsndfile, the library behind Python's `soundfile`, so decoded audio is identical. `symphonia` is the pure-Rust alternative if avoiding the C dependency matters more. Streaming from disk is deferred; sample-based projects fit in memory.

### Numerics

Processing is 64-bit float, as in the Python engine, so parity can be checked closely; output to CoreAudio is converted to 32-bit. Switching to 32-bit processing is an optimization to measure later, not an assumption.

### What must be ported exactly

- **Scheduler:** clip repeats, step-row expansion, swing, gate cutoffs, per-track choke groups, trigger ordering and exact frame rounding.
- **Sampler:** trim, reverse, mono downmix, attack, gate and natural-end release, equal-power mono pan and stereo balance, velocity scaling.
- **Repitch:** a port of `scipy.signal.resample_poly`, with the same Kaiser-window filter design and the same ratio from `limit_denominator(8192)`, so repitched samples match.
- **Filter and EQ:** Butterworth sections and RBJ biquads.
- **Automated filters:** the trapezoidal state-variable path with coefficient updates every 64 frames (decision in [decisions.md](decisions.md)).
- **Compressor:** with sidechain keys delayed by upstream latency.
- **Limiter:** look-ahead with fixed-point dB smoothing that stays partition-exact.
- **Delay:** tempo-synced.
- **Automation:** envelopes with log-domain frequencies, `hold` curves, jumps at coincident points, and constant lanes that render exactly as static values.
- **Routing:** pre- and post-fader sends, returns that are never solo-muted, mute and solo precedence, master gain lane, master chain and the end fade.

The reverb impulse response is generated from numpy's seeded Gaussian noise and scipy's STFT. Matching numpy's generator bit for bit in Rust is possible but fiddly. The recommendation is a documented Rust generator, which makes reverb impulse responses statistically equivalent but not sample-identical to the Python ones. Reverb parity is then tested on decay time, spectrum and energy rather than sample by sample. Record this as a decision when it lands.

## The Mac app

### Toolchain

- **Xcode 16.2** (Swift 6), the last release that runs on macOS 14. The installed Xcode 13.4 is too old for the Observation framework and current SwiftUI.
- **Rust** stable via rustup, targeting `aarch64-apple-darwin`.
- **Build:** the Rust crates build as a static library and XCFramework through a Cargo build step invoked from Xcode. UniFFI generates the Swift bindings, including callback interfaces for the change feed.
- **Deployment target:** macOS 14.

### Structure

SwiftUI covers the window, toolbar, inspectors and panels. AppKit views, bridged into SwiftUI, cover the timeline, faders and knobs, where precise mouse handling and drawing performance matter. Drawing uses Core Graphics and Core Animation first; Metal only if profiling shows the timeline needs it.

View models subscribe to the host's change feed. Each change names the revision and the affected objects, and the app re-reads only those through the bindings. During a drag the control shows its local value immediately and reconciles with the host's when the change comes back.

### Layout (Arrangement View)

- **Transport bar:** play, stop, loop, position in bars and beats, tempo, CPU load and an indicator when the agent is making changes.
- **Ruler:** bars and beats, section markers and the loop brace. Clicking sets the start position; space plays or stops from there.
- **Track headers:** name, color, mute, solo, volume, pan and collapsed sends.
- **Clip lanes:** clips as blocks at their beat positions, labeled with their pattern, with waveforms once available. Repeats are shown as divisions.
- **Automation lanes:** fold out under a track.
- **Returns and master:** at the bottom.
- **Detail panel:** below the arrangement, for the selected track. It shows the effect chain as device panels, generated from the effect schemas (units, ranges, defaults) with no hand-built panel per effect. Sends and pads are listed there too; the pattern step grid comes later.

### Waveforms

Clips are patterns triggering samples, so a clip has no single audio file. Waveforms come from a background offline render of each affected track, reduced to peak data (minimum and maximum per display bucket) and cached by the track's compiled identity. Clips whose audio is being recomputed show a placeholder, then redraw. A native engine renders a track many times faster than real time, so waveforms should trail an edit by a moment rather than seconds; measure this in M8.

### Seeing the agent work

- Controls the agent changes move to their new value with a short animation rather than jumping.
- Objects the agent changed are briefly highlighted.
- An activity panel lists recent commands with their origin, for example "agent: tracks.drums.gain_db −6 → −4.5".
- Undo and redo menu items name the command and its origin ("Undo Agent: Move Clip").

### Keyboard

| Key | Action |
|---|---|
| Space | Play or stop |
| Return | Return to start position |
| ⌘Z, ⇧⌘Z | Undo, redo |
| Delete | Delete selection |
| ⌘D | Duplicate |
| L | Loop selection |
| Arrow keys | Move selection by grid |

## The agent's interface

The agent keeps a JSON-first CLI named `daw`, which works for any agent harness. The Rust binary provides the model, command, transport and render verbs. Python-only subcommands (`samples ...`, `listen`, `compare`) are forwarded to the Python package, so the agent keeps one command name.

Examples:

```sh
daw status projects/my-beat/song.yaml
daw set projects/my-beat/song.yaml tracks.drums.gain_db -4.5
daw clip move projects/my-beat/song.yaml CLIP_HANDLE --at 32
daw effect add projects/my-beat/song.yaml tracks.bass --type filter --id tone --cutoff-hz 800
daw play projects/my-beat/song.yaml --from 32
daw changes projects/my-beat/song.yaml --since 118
daw render projects/my-beat/song.yaml
```

An MCP server exposing the same commands as tools is optional and deferred; add it only if it proves more reliable for agents than the CLI.

## Python after the update

Python keeps what it is good at and where no real-time path is needed: the SQLite sample index (`samples scan/search/inspect/audition/import`), sample analysis (pitch, onsets, loop tempo, kind) and perception (`listen`, `compare` and their images).

Python stops rendering. `model.py` becomes a thin wrapper over the `aaw-py` module, so Python validates and reads projects through the Rust model instead of a second schema. `engine.py`, `effects.py` and `automation.py` are deleted after parity (M7).

## Testing and parity

- **Rust unit tests:** each device, the scheduler, envelopes, the model and commands, run with `cargo test`.
- **Parity harness:** generated fixture projects, using generated audio (tones, noise bursts, clicks) and no personal material, rendered by both engines. Trigger frames must be identical. Audio must match within about −120 dBFS maximum absolute difference, except reverb, which is compared on its statistics. Stems-sum and report fields are compared as well.
- **Model parity:** YAML and `project_sha256` identical for every fixture, including legacy forms; invalid fixtures rejected by both.
- **Partition invariance:** offline renders with random block sizes must match fixed-block renders. This covers the real-time driver's variable buffers.
- **Real-time stress:** playback while a script applies random commands, run with allocation checking, counting dropouts and measuring callback time against the buffer budget.
- **Host tests:** command validation, undo and redo, handle stability, concurrent origins, external-edit reload, autosave atomicity and socket and headless equivalence.
- **Local parity over real projects:** a generic `daw parity PATH` tool, with no paths or project names built in, compares both engines on the person's own projects locally. Results stay in the ignored project directories.
- **Existing tests:** `uv run pytest -q` keeps passing throughout. Tests move to Rust as their subjects move.

## Milestones

Each milestone ends in something usable, and each has an exit test. M2 retires the largest technical risk, so it comes before any app work.

### M0 — Setup and decisions

- Install rustup and Xcode 16.2.
- Create the Cargo workspace under `engine/` and an empty app under `apps/mac/`.
- Update [decisions.md](decisions.md) as listed below, close open question 8 in [plan.md](plan.md), and point spec section 16 at this document.
- Exit: `cargo test` and `uv run pytest -q` pass, and the decision log reflects the new direction.

### M1 — The model in Rust

- Schema v1 types with every current validation rule, exact beats, merge patch, canonical YAML emitter, `project_sha256` and legacy fingerprints.
- A generated fixture corpus with a pytest harness comparing Python and Rust.
- Exit: identical YAML and hashes for every fixture; the same projects are rejected, with equivalent messages.

### M2 — Engine spike: sampler, mixer and real-time playback

- Scheduler, sampler with the ported repitch, voices, choke groups, gate, pan laws, track gain, pan, mute and solo, master gain and end fade. No effects, sends or automation yet.
- Offline driver writing mix and stems.
- Real-time driver through CoreAudio with a terminal command: `daw play song.yaml --from 32`.
- Exit: effect-free fixtures match Python within tolerance; a 15-track project plays at 128-frame buffers without dropouts; playing from the middle chases sounding samples.

### M3 — Session host, commands and agent control

- Host with the command catalog, handles, undo and redo, revisions, change feed, `changes --since`, debounced autosave, external-edit reload, per-user registry and socket.
- Headless CLI mode, with the transport verbs driving `daw play`.
- Update AGENTS.md with the new commands and when to prefer them over `apply`.
- Exit: with one terminal playing, a second runs `daw set tracks.drums.gain_db -6` and the change is heard during playback; `song.yaml` updates, `daw undo` restores it, and `changes` shows both commands with their origin.

### M4 — Mac app, live view and transport

- App embedding the host and engine: ruler, track headers (display only), clip lanes with pattern blocks, section markers, returns, master and playhead.
- Click to set position, space to play and stop, loop brace.
- Live updates from the change feed, with agent-change animation and highlight, and the activity panel.
- Exit: the agent works in a terminal while the window shows each change as it happens, and space plays from the clicked point at any moment.

### M5 — Editing in the app

- Volume, pan, mute, solo and sends in track headers.
- Clip select, drag with grid snap, duplicate, delete and repeats resize.
- Add, remove, rename and reorder tracks and returns.
- Undo and redo labeled with origin; gesture coalescing.
- Exit, the collaboration loop: the person edits, asks the agent for a change, sees it happen, edits again and plays throughout. Everything lands in one undo history and one file diff.

### M6 — Effects, automation and Device View

- All six effects in Rust, including the low-latency reverb.
- Sidechains, sends and returns, master chain, automation lanes and latency compensation.
- Device View with generated panels; add, remove, reorder and bypass effects from the UI.
- Automation lanes displayed under tracks, with point editing.
- Exit: effect and automation fixtures reach parity (reverb statistically); the largest local project plays in real time using at most half the callback budget; knob drags are free of clicks and dropouts.

M6 is the largest milestone and can be split per device.

### M7 — Cutover

- `daw render` runs on Rust, with reports in today's schema and a Rust build identity in place of the Python source hash.
- Python reads projects through `aaw-py`; the Python engine modules are deleted.
- README, mvp.md, effects.md and automation.md updated to describe the new build.
- Exit: the full test suites pass, renders made before the cutover still verify, and `daw listen` and `daw compare` work on new renders.

### M8 — Waveforms and editors

- Background track renders feeding waveform peaks.
- Pattern step grid and event editor, in the detail panel.
- Zoom and scroll performance work, moving to Metal only if measurements require it.
- A sample browser over the Python library index, with drag-to-create tracks and pads.

### M9 — Packaging

- The app bundles the Rust core and the `daw` binary, with a menu item to install the CLI on the PATH.
- The Python analysis tools remain a `uv` environment during development; bundling them waits for a need to distribute.
- Developer ID signing and notarization when the app is shared. Distribution outside the Mac App Store, because the sandbox would block the CLI socket and arbitrary project paths.

## Performance targets

These targets are to be measured and confirmed in M2 and M6, not promises:

- **Callback:** a 16-track project with effects uses at most 50% of the buffer budget at 128 frames and 48 kHz.
- **Parameter edits:** a fader or knob change is audible within one buffer of the host receiving it.
- **Structural edits:** a moved clip or an added effect is audible within about 50 ms.
- **Offline render:** at least 20× faster than real time for a 16-track project with effects.
- **Timeline:** scrolling and zooming at 60 frames per second with a few hundred clips.

## Scope

In scope for this plan: Arrangement View, sampler tracks and patterns, the six built-in effects, sends and returns, automation, real-time playback with a transport, undo, and live co-editing between person and agent.

Deferred, each a substantial project of its own:

- MIDI instruments and synthesizers.
- Time-stretching and warp markers.
- Third-party AU/VST3 plugin hosting.
- Audio recording and input monitoring.
- Session View clip launching.
- MIDI import and export.
- Groups and return-to-return sends.
- Tempo and meter changes.

Existing deferrals in [mvp.md](mvp.md) stand unless this plan names them.

The risk in plan.md about scope creep toward a full DAW changes meaning. A hand-editing UI is now a goal, but each feature should still serve the shared person-and-agent workflow before it earns a place.

## Risks

| Risk | Mitigation |
|---|---|
| Dropouts from real-time rule violations | Allocation checking in debug builds; the stress test runs random commands during playback; review every audio-thread code path |
| Parity drift during the port | Python stays the reference until M7; generated fixtures and local parity runs; nothing is deleted before its parity test passes |
| Canonical YAML mismatch | Fixture corpus; fallback of a one-time reformat recorded as a decision; the hash must match regardless |
| Reverb differs from Python output | Intentional, documented generator; statistical parity tests; decision recorded |
| Latency from long reverb partitions | Non-uniform partitioning is part of the port, not a later optimization |
| Two languages and a binding layer | Swift stays a thin client; UniFFI generates the bindings; no model logic in Swift |
| Host and file disagreeing | Autosave after every change; external edits reload as commands; one writer at a time |
| Agent and person editing the same field | Serialized commands; later wins; both in history; agent edits visibly highlighted |
| Old toolchain on this macOS version | Xcode 16.2 on macOS 14; newer Swift requires a macOS upgrade later |
| Scope growing toward Ableton parity | The milestone gates above; deferred features require a decision to start |

## Decisions to record

Recorded in [decisions.md](decisions.md) in M0:

- **D1 (revised):** real-time playback is in scope. Offline rendering stays deterministic and uses the same engine. Recorded as D33.
- **D5 (unchanged):** Arrangement View model; no Session View.
- **D18 and D25 (superseded):** the engine and devices move to Rust. Python remains for the library, analysis and perception. Recorded as D34.
- **D21 (revised):** when the app is open, its session host is the authority for the song and saves `song.yaml` continuously. The agent reaches it through the `daw` CLI. All musical state is in the file; playhead, selection and undo history are ephemeral host state. Recorded as D36.
- **D22 (kept, strengthened):** one engine for playback and export. Live smoothing applies only to live parameter moves. Recorded as D37.
- **D24 (revised):** the core library is Rust. The CLI, the app and the Python bindings are shells over it. Recorded as D35.
- **New:** edits are commands with origins, handles and undo. Recorded as D38.
- **New:** Swift and Rust are connected through UniFFI. Recorded as D39.
- **New:** reverb impulse responses come from a documented Rust generator. Recorded as D40.
- **plan.md open question 8 (closed):** native macOS app with SwiftUI and AppKit, in D39.

## Open questions

1. Continuous autosave only, or autosave plus an explicit save that marks a checkpoint? The recommendation is continuous autosave, with git or a future checkpoint command for history.
2. Should clips gain optional IDs in the schema later, so handles survive between sessions?
3. Is an MCP server worth adding beside the CLI once agents use the command set?
4. Should the undo history persist across app restarts, for example as a command journal under `.daw/`?
5. Should sample import move into Rust eventually, leaving Python with analysis and perception only?

## First steps

1. The person installs Rust (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`) and Xcode 16.2.
2. Write the decision-log changes and the Cargo workspace skeleton (M0).
3. Build the fixture generator and the model port (M1), then the engine spike (M2). The spike's exit test decides whether the rest of the plan proceeds as written.
