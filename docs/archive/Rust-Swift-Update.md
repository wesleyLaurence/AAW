# Rust + Swift update: build plan for a native real-time DAW

> Archived October 2, 2026, and kept for history; it is not maintained. The plan of the Rust and Swift rebuild, finished October 1, 2026. Its milestones are in [completed.md](../completed.md), what it left unverified or open is in [backlog.md](../backlog.md), and [architecture.md](../architecture.md) describes the result. The Progress section is the detailed record of each milestone's exit test.

Status: adopted September 30, 2026; M0 to M9 done, and the M2 spike and the M3 to M8 exit tests passed (see [Progress](#progress)). The Python engine is retired. Of packaging (M9), signing with a Developer ID and notarizing are scripted and have not been run. It replaces the workspace track (U0–U4) in [plan.md](plan.md), and decisions D33 to D40 in [decisions.md](../decisions.md) record the changes listed in [Decisions to record](#decisions-to-record); D41 records M3's choices, D42 M4's, D43 M5's, D44 M6's, D45 M7's, D46 M8's and D47 M9's. [mvp.md](../architecture.md) remains authoritative for what exists today.

## Progress

- **M0:** Cargo workspace under `engine/`; decisions D33 to D40 recorded. Xcode 16.2 (Swift 6.0.3, macOS 15.2 SDK) installed September 30, 2026; the engine tests pass with it.
- **M1:** `aaw-model` matches `model.py` on 12,532 generated documents and on every local song: the same documents are accepted, canonical YAML is byte-identical, and `project_sha256` and the legacy fingerprints are equal. Rejections carry pydantic's locations, types and messages. Known differences, all in input no tool writes, are listed in [engine/README.md](../../engine/README.md).
- **M2:** The Rust sampler engine renders effect-free songs byte-identically to the Python engine: 199 of 199 files over 60 generated songs, and 52 of 53 files over four local 10–15 track songs with their effects removed (the other within 1.8e-12). Playback of a 15-track song at 128-frame buffers on the built-in speakers used at most 0.05 ms of each 2.67 ms callback, with no xruns; the headless benchmark runs about 500 times faster than real time. Playing from beat 32 picks up sounding samples and equals the same frames of a full render. The plan proceeds as written.
- **M3:** The session host (`aaw-host`) holds the song in memory, applies the command catalog below (plus `remove`, `get` and `return rename`), keeps undo, redo and a change log with origins, gives clips, effects, events and points `@N` handles, saves `song.yaml` after each command, loads external edits as undoable steps and registers a per-user socket. With no host running, commands run headless. Exit test on the same 15-track song at 128-frame buffers: while `daw play` played from beat 32, a second process ran `daw set tracks.drums.gain_db -6` and later `daw undo`. Each took about 20 ms end to end, including recompiling and saving; both new versions reached the audio thread; `song.yaml` changed and was restored; and `daw changes` listed both with origin `agent`. The 25 s of playback had no xruns, and the slowest callback took 0.05 ms. Whether the 2 dB drop was audible is left to the person to judge. A stress test swaps in 140 random edits while 1,127 blocks render on schedule, with no allocation on the audio thread. Saving before each reply, the 5 ms crossfades and headless limits are recorded in D41. A debounce, gesture coalescing and parameter messages that skip recompiling wait for drags in M5.
- **M4:** The Mac app (`apps/mac`, built by `build.sh` as a Swift package around the Rust core; see [apps/mac/README.md](../../apps/mac/README.md)) opens a song and becomes its session host, on a thread of its own (`host::spawn`), while `daw` commands from other processes reach the same host through its socket. `aaw-ffi` gives the app each revision's arrangement and the objects the change touched, the transport and the playhead read from the audio thread. The window shows a ruler with bars, section markers and the loop brace, a header for each track, return and the master (display only), the clips as pattern blocks with their repeats, the start position and the playhead, and an activity panel listing each change with its origin. Rows and clips ease to each new revision, and what an agent's or an external change touched lights up in that origin's color. Clicking sets the start position, space plays and stops, Return jumps back to the start position, a drag in the ruler's top strip sets the loop and L toggles it. Exit test on a generated five-track song, with the app's input sent as scripted events through its normal event path (`--click`, `--key`, `--snapshot`): a click at beat 50 and a space press started playback there on the built-in speakers; while it played, `daw set tracks.hats.gain_db -14` and `daw clip move` ran from a terminal, and a picture of the window taken a second later showed the fader at its new value, the clip at its new beat, both lit, and both commands in the activity panel with origin `agent`. `daw status` reported 128-frame buffers, a slowest callback of 0.02 ms and no xruns. A 15-track local song opened in under a second and drew correctly. Nobody watched the window or listened during these runs; the person then tried the app and found it working. The choices made are recorded in D42; the playhead is not yet compensated for output latency.
- **M5:** The person edits in the window, and each edit is a host command with origin `user` in the same history as the agent's. Track headers take drags on volume and pan and clicks on mute and solo, and unfold to a level for each return; returns take volume and mute, and the master volume. Clips are selected by click and Shift-click, dragged by grid steps (off the grid with Option) and across tracks, resized at their end by whole repeats, duplicated with ⌘D, deleted, and moved with the arrow keys; L loops the selection. Tracks and returns are added (⌘T, ⌥⌘T), renamed in place, reordered by dragging their headers and deleted. Undo and Redo in the Edit menu name the step and whose it is, such as "Undo Agent: Move clip eighths at 0". A drag shows its value at once and sends the host at most 30 edits a second, one at a time, under one gesture ID; the host merges a gesture's edits into one undo step and one log entry, and saves when they pause for 250 ms instead of after each. A change of mixer values alone gives the playing program its new gains without compiling again. `aaw-ffi` turns the app's edits into commands and works out their beats exactly, so a clip on a third of a beat stays on thirds when dragged. The host gained `return move`, a `label` for a batch and `select`: `daw status` lists what the person has selected. Exit test on a generated five-track song, with the app's input scripted as in M4: while the app played, a drag moved a clip two bars; `daw set tracks.bass.gain_db -10` and `daw clip move` ran from a terminal; then came a drag of the master volume, ⌘Z twice (undoing that drag as one step, then the agent's clip move), a mute click, an arrow-key move and a volume drag. The eleven revisions were in one change log with their origins, each drag as one entry; `daw status` named the selected clip and reported 128-frame buffers, a slowest callback of 0.01 ms and no xruns; `song.yaml` differed from its start by exactly the surviving edits; and a picture of the window showed them. Other scripted runs covered sends, renaming, adding, reordering and deleting, resizing, duplicating, and an edit the host refuses, which is reported and put back. On the largest local song (12 tracks, 1,351 hits, 110 KB of YAML) a fader step takes the host about 23 ms while playing, against about 40 ms for an edit before M5. Nobody watched the window or listened during these runs. The choices made are recorded in D43; parameter messages to the audio thread and compiles of only the tracks an edit affects wait for M6, whose devices need them.
- **M6:** The Rust engine plays and renders the whole song: the six effects on tracks, returns and the master, sidechains, pre- and post-fader sends, automation lanes and latency alignment (`aaw-dsp`, `aaw-engine`). The reverb adds no latency. Parity with the Python engine: over 120 generated songs with effects, returns and automation the largest difference in any mix or stem was 3.7e-9, most files were identical, and the reports agree on each effect's latency, lanes and gain reduction; on copies of four local songs every stem without reverb was within 6e-8, reverb returns within 0.5 dB and mixes within 0.05 dB in level. Reverb tails differ sample by sample by design (D40): over sixteen seeds the engines' decay times agree within 3 ms and their octave levels within about half a dB. Exit test on the largest local song (12 tracks, 3 returns, 32 effects, 10 lanes, 1,351 hits): 20 s through the built-in speakers at 128-frame buffers used 0.09 ms of each 2.67 ms callback on average and 0.52 ms at most, with no xruns; the headless benchmark over the whole song had a slowest block of 0.58 ms. `daw render` takes 8.0 s for its 179 s, 22 times faster than real time, where the Python engine took 31.5 s; two other local songs with effects render 22 and 24 times faster than real time. An edit during playback is a new program that takes over in place: devices keep their state, so tails ring through edits, locates and stops, levels and knobs glide over 5 ms, and a change of structure fades through a 10 ms dip (D44). A compile after a level or knob edit takes under 1 ms, and a first compile of the largest song 0.4 s. A test steps every level and knob of a sustained tone thirty times a second: the steepest step in the output was 6% larger than the tone's own, where a level stepped without a glide is over eight times larger. In the app, the row last selected shows its effect chain as device panels drawn from each effect's fields, with add, remove, reorder and bypass, and a row's automation lanes fold out under it with points to add, drag and remove. Exit test on a generated five-track song with eight effects, the app's input scripted as before: while the app played, three knob drags and a point drag were made in the window, and `daw set` on a delay's feedback, `daw effect add`, `daw lane point add` and `daw set` on a reverb's decay ran from a terminal. The changes were in one log with their origins, each drag as one entry; `daw status` reported 128-frame buffers, a slowest callback of 0.37 ms and no xruns; `song.yaml` differed from its start by exactly those edits; and a picture of the window showed them. Other scripted runs covered a lane added from a device, a point deleted, an equalizer's bands added and removed, an effect moved, removed and brought back by undo, and the largest local song opened and drawn in about a second. Nobody watched the window or listened during these runs, so whether a glide or a dip is heard is left to the person to judge. The playhead now trails the audio thread by the song's latency and the delay the output reports. The choices made are recorded in D44.
- **M7:** The Python engine is retired. `daw` is the Rust binary: `init`, `describe`, every edit, `play` and `render` run there, and it passes `samples`, `listen`, `compare` and `check` to Python, while `uv run daw` passes every other command to the binary, so either is the one command. Python reads, validates and saves songs through `aaw-py`, a PyO3 module over `aaw-model` that the package builds for its own interpreter; a song there is plain data. `engine.py`, `effects.py`, `automation.py` and the pydantic model are deleted; pydantic is no longer a dependency, and PyYAML only of the tests. A sample import copies the file and adds it with `daw apply`, which took a label, so a running host records "Import sample kick" as an undoable edit. `daw describe` prints the schema from Rust, where a test holds it to validation field by field, and a render report names the Rust build where it had the Python source hash. Before the deletion the two engines were compared a last time: the models agreed on 6,133 generated documents and on generated `fmt` and `apply` edits; 60 generated songs without effects rendered byte-identically; over 60 with effects, returns and automation the largest difference in 257 mixes and stems was 5.8e-11; and on copies of five local songs one was refused by both for clipping, one was identical in all 11 files, and the others had every stem without reverb within 6e-8, reverb returns within 0.45 dB and mixes within 0.02 dB in level. What the Python model did with 533 documents and 120 edits is pinned in `tests/golden_model.json`, which the Rust model is tested against from now on; the engine's tests no longer have a reference and hold it to its own promises instead, among them generated songs rendered whole, in other block sizes, as sections and as single channels. The Python suite's device, routing and automation tests run through `daw render`. Exit test: `uv run pytest -q` passes its 170 tests against the release binary in 69 s, where the suite took 170 s with the Python engine; `cargo test` passes its 111 and the app's Swift tests their 16. All 58 renders in the local projects, made by the Python engines, verify with the new perception code: mix and stem hashes, and the snapshot against the fingerprint in each report. Copies of three local songs were rendered with the final binary; `daw listen` verified and measured each, and `daw compare` set each beside the Python engine's render of the same song: identical for the song without effects, and within 0.02 dB in level for the two with reverb. The largest (179 s, 32 effects) rendered in 8.6 s with other programs busy, and the Python engine in 37 s in the same session. With the app open on a generated song, `uv run daw set`, a sample import, `effect add`, `check` and `undo` from a terminal landed in the app's host, and a picture of the window showed them in the activity panel. Nothing was played and nobody listened during these runs. The `daw parity` tool this plan listed was not built: local comparisons ran from scripts kept outside the repository, and with the reference gone it has no use. The choices made are recorded in D45.
- **M8:** Clips show what they play, patterns are edited in the window, and a browser adds samples from the library. A clip's waveform is its track's sampler output over the clip's span, before the track's effects and fader. After each compile a thread works out peaks for the tracks whose voices changed (`aaw-engine`'s `peaks`, `aaw-ffi`'s `waveform`), the least and greatest sample of every 64 frames and of stretches four times as long each, and the app draws from the resolution nearest its zoom. Until a track's new peaks arrive, a clip keeps the waveform it had, which moves with it. The detail panel has two editors: a row's header shows its devices, and a clip its pattern, with a row for each pad of the clip's track. A pad that plays its sample through has steps to click on and off, to drag along and to set from 1 to 9; a held pad has events as bars; a pad whose sample has a root note has them by note, as a piano roll does. Events are added, moved by steps of the pattern's grid and by notes, stretched and removed, and a pattern's length, step and swing are set beside it. Double-clicking an empty part of a track adds a clip with a new pattern, and Own Copy gives a clip a copy of a shared one. The browser lists what `daw samples search` finds in the library's index, plays a sample from its file on a click, and adds it as a pad of a track or as a new track with the mark beside it or a drag onto the arrangement, where an audio file from the Finder lands the same way. The file is copied into the project by Python (`daw samples import --copy-only`), and the sample, the pad and a new track are one undo step. Every edit is one of the host's existing commands with origin `user`. The host gained one event, a revision's compiled program, and a batch can now add to what it has just made. Measured on the largest local song (12 tracks, 179 s, 1,351 hits): peaks for all tracks take 277 ms one track at a time and 132 ms for the slowest, which `daw play --benchmark` now reports; in the app on a copy of the song every waveform is there 0.6 to 0.85 s after opening, of which the first compile is 0.4 s, and 20 to 153 ms after an agent's edit of a clip, over an edit on each of the 12 tracks (34 ms at the median). Two other local songs take 38 and 49 ms for all their tracks. Timeline: the app's `--measure` zooms in eight times, scrolls through the song and zooms out again, timing each draw. On a generated song of 16 tracks and 971 clips in a window 1,440 points wide, draws first took 13.9 ms on average and 30 ms at the 95th percentile, with almost a quarter of them longer than a sixtieth of a second and frames 21 ms apart on average. A profile put 44% of that in sorting the clips into drawing order every frame and 35% in laying out each clip's name every frame; waveforms were 6%. With the order kept between frames and lines of text laid out once, draws take 3.2 ms on average, 5.6 ms at the 95th percentile and 12.6 to 16.2 ms at most over two runs of 480 frames, none longer than a sixtieth of a second, and frames are 16.67 ms apart. The copy of the largest local song draws in 3.1 ms on average and 5.2 ms at the 95th percentile. Core Graphics is enough, and nothing moved to Metal. Exit test on a generated five-track song, with the app's input scripted as before: steps were clicked on, dragged along a row and set harder; `daw pattern steps`, `daw clip move` and `daw pattern event add` ran from a terminal; a note was added, moved in time and pitch, lengthened and raised, and two of those undone with ⌘Z; then "snare" was typed into the browser over a generated library of 11 samples, the first result added as a new track, a clip double-clicked onto it and four steps clicked on. The 18 revisions were in one change log with their origins, `song.yaml` differed from its start by exactly the surviving edits, a picture of the window showed them, the waveforms of the last change were there 6 ms after it, and draws took 2 ms on average. `cargo test` passes its 123 tests, `uv run pytest -q` its 171 in 81 s, and the app's Swift tests their 30. A drag from the browser or the Finder cannot be scripted and was not tried; the mark beside a sample, which makes the same edit, was. Nothing was played, and nobody watched the window or listened during these runs. The code the audio thread runs did not change. The choices made are recorded in D46.
- **M9:** The app is a bundle that runs from any folder, with `daw` inside it and a menu item that puts `daw` on the PATH. `apps/mac/build.sh` puts `daw`, built as the tests build it, at `Contents/Helpers/daw`, and copies libsndfile and the seven libraries it links from Homebrew into `Contents/Frameworks` with their licenses; the app and `daw` are pointed at the copies, and the copies at one another. The build stops if anything in the bundle still links outside it and the system or was built for a newer macOS than the app says it runs on, signs the libraries, `daw` and the bundle, verifies the signature and runs the bundle's `daw`. The bundle is 19 MB. **AAW › Install Command Line Tool…** makes `/usr/local/bin/daw` a link to the bundle's `daw`, asking for an administrator's password where the folder needs one; it says what is at the link first, points a link to another copy of the app at this one on asking, offers to remove its own, and leaves a file that is no link alone. `build.sh dist` zips the bundle; with `AAW_SIGN_IDENTITY` the parts are signed with that certificate, the hardened runtime and a timestamp, and with `AAW_NOTARY_PROFILE` the zip is notarized and the ticket stapled. The plan gave M9 no exit test, so this was run: the bundle was copied to another folder and its `daw` linked from a third, as the menu item links it. Through the link, every library `daw` loaded outside the system came from the copy; a generated song with effects, returns and automation rendered to the same six files, byte for byte, as with the checkout's `daw`; `daw check` and `daw listen` ran in Python; and with the copied app open on the song, `daw status` reported `"host": true` and `daw set tracks.t1.gain_db -7` showed in a picture of the window, on the fader and in the activity panel with origin `agent`. With `AAW_PYTHON` naming no file, `check` reported that Python could not be started and `inspect` worked. The zip `dist` made, 6.8 MB, unzipped elsewhere to a bundle whose signature verified and whose `daw` ran; Gatekeeper's assessment of it was `rejected`, as it is for anything not notarized. A copy signed ad hoc with the hardened runtime did not start: macOS refused the libraries for having no team to match, which is why an ad hoc build goes without it. With that one check waived by an entitlement, the same copy opened the song, hosted it, took an edit from `daw`, ran `check` in Python and ran `daw play --benchmark`, so nothing else in the app needs an entitlement. Not run: signing with a Developer ID and notarizing, there being no certificate on this Mac (a build asked to sign with one that is not there stops with codesign's error); the menu item's alerts and the request for a password, which wait for a person; and the app on another Mac. The link itself is covered by Swift tests in a scratch folder, which run the shell command an administrator would. `cargo test` passes its 123 tests, `uv run pytest -q` its 171 in 76 s, and the app's Swift tests their 40. Nothing was played, and nobody watched the window or listened during these runs. The code the audio thread runs did not change. The choices made are recorded in D47.

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
| Tracks and returns | `track add`, `track remove`, `track rename` (updates sends, sidechains and lanes that reference it), `track move`, `return add`, `return remove`, `return move` |
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
- **Automated filters:** the trapezoidal state-variable path with coefficient updates every 64 frames (decision in [decisions.md](../decisions.md)).
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
- **Build:** the Rust crates build as a static library and XCFramework, and UniFFI generates the Swift bindings, including callback interfaces for the change feed. `apps/mac/build.sh` runs both and builds the app as a Swift package, which Xcode also opens (D42).
- **Deployment target:** macOS 14.

### Structure

SwiftUI covers the window, toolbar, inspectors and panels. AppKit views, bridged into SwiftUI, cover the timeline, faders and knobs, where precise mouse handling and drawing performance matter. Drawing uses Core Graphics and Core Animation first; Metal only if profiling shows the timeline needs it.

View models subscribe to the host's change feed. Each change names the revision and the affected objects and carries the arrangement after it (D42). During a drag the control shows its local value immediately and reconciles with the host's when the change comes back.

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
- Update [decisions.md](../decisions.md) as listed below, close open question 8 in [plan.md](plan.md), and point spec section 16 at this document.
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

Existing deferrals in [mvp.md](../architecture.md) stand unless this plan names them.

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

Recorded in [decisions.md](../decisions.md) in M0:

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
