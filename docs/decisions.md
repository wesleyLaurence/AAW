# Decision log

Settled choices from the design conversation, with rationale, so a future session does not relitigate them. Revisit only with a new reason.

**D1. Offline, deterministic rendering. No realtime audio engine in version one.** Revised by D33.
Realtime is where nearly all DAW complexity lives, and an agent iterates by edit, render, analyze. Determinism lets the agent trust its own loop and lets git act as a version history of sound. The person hears the project through a player over rendered audio and, in the workspace, through fast region re-rendering against the cache, which is responsive without being a realtime engine. A callback-driven engine for playing instruments live is an end-state question, settled with the device language before phase 3 (D22, plan open question 7).

**D2. The session is a plain text document. The format is the API.**
Agents are best at reading and writing text. Ableton's own session file is gzipped XML, proving a DAW is a document. Text diffs cleanly, fits in context, and works with git. The agent edits state directly and uses the CLI only for verbs that compute.

**D3. Instruments and effects are built in-house, AI-native.**
Old VST plugins expose normalized floats, binary presets, and hidden state, all designed for a screen. In-house devices use real units, text patches, self-descriptions, and a modulation system declared in text. No third-party plugin hosting in version one.

**D4. Adopt existing open source only where clearly optimal and necessary.**
Time stretching (Rubber Band), stem separation (Demucs), FFT and resampling, loudness measurement, and audio embeddings are deep, solved problems that would take months to rebuild worse. Everything above that layer is built here. New formats such as the pattern notation are justified because nothing existing was designed for an agent reader.

**D5. Ableton vocabulary and the arrangement view mental model.**
Session, track, clip, device, rack, send, return, automation. The models already know these words. Arrangement timeline only; no session view, scenes, or clip launching.

**D6. No session-level key. Theory is a library.**
Chords sit outside keys, songs modulate, modes exist, and interesting music breaks rules. Scales, chords, modes, voicings, and key inference are tools the agent consults. Tonal intent is prose in the brief or session notes.

**D7. Tempo and time signature are session globals, constant in version one.**
They are set early and shape everything. Tempo automation and meter changes are deferred.

**D8. Time is bars and beats everywhere the agent reads or writes.**
Samples and seconds appear only inside the renderer and analysis tools.

**D9. Real units everywhere. Never normalized 0 to 1.**
Hz, dB, ms, semitones, cents, beats, percent. The clearest example of what old plugins got wrong for agents and the cheapest thing to get right in-house.

**D10. Perception is numbers and images, designed for the reader.**
The agent does not hear. Descriptors, relational analyses such as masking and diffs, reference deltas, and spectrograms drawn with bar gridlines are the perception layer. Taste and feel come from the person. An audio-input model critic is optional and later.

**D11. Samples are the priority workflow. No recording.**
Input is samples, songs, and files the person provides, plus what the agent synthesizes. A sample is a first-class analyzed object with search, slicing, and provenance. Sampler and drum rack are the first instruments; the synth comes after.

**D12. Tempo policy on import.**
No session tempo yet: adopt the detected tempo, confirming with the person when confidence is low or half or double time is ambiguous. Session tempo set: warp the material to it, preserving pitch by default, recording source tempo and downbeat on the clip. One-shots are never stretched.

**D13. Every step leaves a listenable artifact.**
Renders land in the project as they are made so the person can listen and interrupt. Stems are a byproduct of the track cache. The agent keeps a journal and commits checkpoints after meaningful steps.

**D14. Outputs are inputs.**
Any render can be reimported as a sample. Promotion copies artifacts into the library with the recipe that made them.

**D15. Three tiers of intent, distinguished by who may write them.**
Rules (person only, never edited by the agent), preferences (agent-written with evidence and dates, person-editable, decaying unless reconfirmed), and a per-project brief (both). Precedence is rules, brief, preferences, defaults. The agent states what it learned at the end of a session rather than adapting silently.

**D16. The DAW owns its own memory files.**
Profile and brief live in the DAW's format so they work under any harness. The harness instruction file points at them. Music belongs to the DAW; general working preferences belong to the harness.

**D17. Seeded randomness.**
Humanize, random modulation, and round robin take seeds from the document so renders are reproducible.

**D18. Python first, behind a stable contract.** Superseded by D34.
numpy and scipy for DSP, with the document format and CLI as the contract. A Rust port of the render hot path is the planned escape hatch if the performance target is missed. Devices are written in a block-processing shape from the start so that the device language can be revisited before phase 3 without changing their interface (D22).

**D19. Human in the loop by default, autonomous by capability.**
The person drives with ideas, material, references, and taste, and can interrupt at any point. The agent must also be able to finish a song alone against the brief, rules, and a definition of done.

**D20. The agentic loop is the product. The workspace UI is the end state, designed in from the start and built second.**
DAW UIs already exist. An agent that works for hours with a DAW toolkit does not, and that is what is unique here. But the end state is a shared workspace where a person adjusts tracks, devices, and knobs by hand and hears the result while the agent works in the same project. The numbered phases come first; the workspace runs as a parallel, lower-priority track after phase 0. The early build carries four small requirements on its behalf, each worth having anyway: canonical form (D23), single device implementation (D22), region rendering, and the library layering (D24).

**D21. The workspace is a second client of the document, never a second source of truth.** Revised by D36.
It reads and writes `song.yaml` through the same core library and serializer the agent's tools use, and holds no state the file does not hold. Watching the agent work is file watching; the agent's renders appear as files in `renders/`. Hand edits made in the workspace reach the agent as file changes and git diffs, and they are already a preference signal under D15. There is no integration layer between the agent and the workspace beyond the project directory. The agent stays in the terminal; the workspace is a window, not a chat client.

**D22. Every device is implemented exactly once. The person and the agent always hear the same audio.** Restated by D37.
If the workspace previewed a device with anything other than the real renderer, the person would tune by ear against one sound and the agent would analyze another, and the perception loop would stop being trustworthy. So workspace playback is rendered audio only, mute, solo, volume, and pan may be applied in the player because they are exact, and every other change goes through the renderer. Approximations such as browser audio nodes as a drag-time preview are forbidden. Consequence: if a true realtime engine is ever wanted, the devices must be in a language that runs both offline and in a callback, which is why the device language is decided before phase 3 rather than after.

**D23. The document has one canonical form and every writer produces it.**
Two writers rewriting one file need identical formatting or every diff fills with noise the agent has to read past. `daw fmt` produces canonical form, `daw check` rejects anything else, writes are atomic, and every writer re-reads before modifying. This constrains the syntax choice in plan open question 1.

**D24. One core library; the CLI and the workspace daemon are thin shells over it. The daemon is never required.** Revised by D35.
The `daw` package holds the document model, renderer, devices, perception, and library index. The CLI and `daw serve` contain no logic of their own. When a daemon is running the CLI hands renders to it so both clients share one cache; when none is running the CLI does the work itself and the agent notices no difference. This is what keeps the workspace from becoming a second implementation of anything.

**D25. Devices are written in Python (numpy/scipy), in the block-processing shape.** Superseded by D34.
Settles plan open question 7. Offline rendering is the product (D1) and a realtime engine is still hypothetical, so one Python implementation serves mixes, stems and previews. Devices carry explicit state, declare latency and must produce identical output for any block partition, which keeps a later port to a compiled kernel mechanical. Accepted risk: a realtime engine would require rewriting the devices.

**D26. Stems are post-insert and pre-master-effects.**
Each stem is a track after its inserts, gain, pan, mute and solo, master gain and end fade, and before master effects. Without master effects the stems sum to the mix; with a nonlinear master chain they cannot, and the render report says so in `stems_sum_to_mix`. Sidechain keys are taken after the source's inserts and before its fader, so fader and mute changes on the source never change ducking.

**D27. Shared space comes from return buses, and returns are stems.**
Tracks send to `returns`, post-fader by default (after gain and pan) or pre-fader (after inserts). One reverb shared by several tracks is how producers glue a mix, and it keeps the device count and render cost down. A muted or solo-muted track sends nothing, but returns are never solo-muted, so soloing a snare still plays its reverb. Track stems stay dry and each return renders its own stem, so D26's reconstruction still holds with track and return stems together. Returns share the ID namespace with tracks because both name stems. Sidechains key only from tracks, and returns cannot send, which keeps render order simple: tracks by sidechain dependency, then returns. Groups and return-to-return sends wait for a need.

**D28. Reverb is seeded convolution with a synthetic impulse response.** Partly revised by D40.
A generated noise tail with frequency-dependent exponential decay is reproducible from its parameters and seed (D17), convolves exactly regardless of block partition through fixed internal partitions with compensated latency (D25), and runs fast with numpy FFTs. A feedback-delay-network reverb would need per-sample recursion that is slow in numpy and hard to make partition-exact. Convolution also leaves room to load recorded impulse responses as samples later. Accepted limits: the input is summed to mono, and there is no modulation or early-reflection model.

**D29. Automation lanes override static values and cover an allowlist of continuous parameters.**
A lane is `{param, points}` on a track, return or the master and replaces the static value for the whole song, holding its first and last values outside its points, the way arrangement automation behaves in Ableton. Only continuous parameters whose change needs no device rebuild are automatable: levels, pans, send levels, filter cutoff, EQ band frequency, gain and q, compressor threshold and makeup, delay feedback and mix, reverb mix. Frequencies and q interpolate in log, everything else linearly in its own unit. Lanes are evaluated at absolute timeline frames, which the chain offsets for each device by the latency before it, so changes land on the beat and output stays independent of block partition (D25). Filter and EQ coefficients update every 64 frames from the start of the song, a fixed rate rather than one tied to the caller's blocks. Nothing is smoothed: a written jump is a jump. Unautomated devices keep their original code path, so existing projects render bit-identically. Modulation and automating switches wait for a need.

**D30. The project fingerprint hashes the saved form.**
`project_sha256` hashes the model without defaults, which is what `save` writes, so adding an optional field no longer changes every fingerprint and breaks older renders. Earlier engines hashed the full model; render verification also accepts those forms, one per schema change in `model.LEGACY_FIELDS`. Those forms are rebuilt from today's full model, so every later field must be listed there too; a test pins the fingerprints the earlier engines wrote and fails until it is.

**D31. Automated filters keep state-variable state across coefficient changes.**
A direct-form biquad's state carried into very different coefficients bursts: a 48 dB lowpass jumping from 20 kHz to 10 Hz reached 220 times the input peak, and even a 1/64 beat ramp reached 12. Each automated section is a trapezoidal state-variable filter whose integrator states stay near signal level, as an analog filter's capacitors do. Between updates it runs as the equivalent direct form II biquad through `lfilter`, which keeps it fast, and its history is converted when coefficients change. Static filters and EQs keep `butter()` and RBJ biquads.

**D32. A constant lane is its static value.**
A lane whose points all share one value renders exactly as a static parameter with that value: channel lanes return a scalar and effects are built from their spec with the value in place. Such a lane previously built per-frame arrays and sent filters and EQs through the state-variable path, which matched the static filter only to within 1e-9. Moving lanes are evaluated over consecutive frames by filling each segment in place, so a lane costs one array over the timeline rather than about ten. Reports still list constant lanes as automated.

**D33. Real-time playback is in scope. Offline rendering stays deterministic and uses the same engine.**
The end state is a person and an agent sharing one song: click a position, press space and hear each edit right away. Rendering the whole song offline took 4 to 34 seconds on local projects, far too slow for that. A callback-driven engine plays from any beat and applies edits during playback. Export runs the same program with fixed blocks as fast as the machine allows, so it stays deterministic, and the agent's render-and-analyze loop and git as a history of sound are unchanged. Settled with the plan in [Rust-Swift-Update.md](archive/Rust-Swift-Update.md).

**D34. The engine and devices move to Rust. Python keeps the sample library, analysis and perception.**
A real-time callback cannot tolerate the interpreter, the GIL or garbage collection, and D25 accepted that a real-time engine would mean rewriting the devices. Rust compiles to native code without a garbage collector, its compiler rejects most memory-safety errors and data races, and it has the FFT, lock-free queue, audio I/O and Swift and Python binding crates the port needs. C++ with JUCE was considered; its clearest advantage, hosting third-party plugins, is outside D3. The Python engine stays the reference: each subject is ported against it and deleted only after its parity tests pass. The library index, sample analysis and perception stay in Python because nothing there needs a real-time path.

**D35. The core library is Rust. The CLI, the Mac app and the Python bindings are shells over it.**
There is still exactly one implementation of the document (D23) and of each device (D22). The `daw` binary links the crates directly, the app through UniFFI and Python through PyO3, so Python reads and validates projects through the Rust model rather than a second schema. A running app is never required: with none open, the CLI runs the same core headless against the file, as D24 required of the daemon.

**D36. While the app has a project open, its session host is the authority for the song.**
The host holds the song in memory, applies edits from the app and the agent one at a time, and writes canonical `song.yaml` shortly after each change. The agent reaches it through the `daw` CLI over a local socket. With no app running, the CLI runs the same host headless against the file under the project lock. All musical state is in the file. Playhead, play state, loop region, selection, zoom and undo history are ephemeral host state, readable through `daw status` but never saved. A change to the file from outside, such as a text edit or `git checkout`, reloads as one undoable command. D21's file-only channel could not give the person and the agent one undo history, live playback of edits or a feed of what changed; an in-memory authority can, while the file stays the saved form and git stays the history.

**D37. One engine plays and exports. Live smoothing applies only to live parameter moves.**
The real-time and offline drivers run the same compiled program and the same devices, and processing stays independent of block partition (D25), so a performance played from the start equals its offline render. A knob or fader moved during playback is smoothed over a few milliseconds to avoid zipper noise. Automation written in the document keeps its exact, unsmoothed semantics (D29), and an offline render reflects only saved values. This replaces D22's rule that the workspace plays only rendered audio: the engine itself now plays, so the person and the agent still hear the same devices.

**D38. Edits are commands with origins, handles and undo.**
Every change, from the app or the agent, is a command validated against the whole resulting song before it applies, tagged `user`, `agent` or `external`, and given a new revision and `project_sha256`. Commands address fields with the automation parameter syntax, such as `tracks.drums.effects.sweep.cutoff_hz`. Objects that schema v1 leaves without IDs, such as clips, unnamed effects, pattern events and automation points, get handles that stay stable for the session, so an address keeps meaning the same object after other edits; the file format does not change. `daw apply --expect` remains for whole-document edits. A command log tells the agent what the person did as a list of operations rather than a diff to interpret, gives both one undo history, and lets edits to different objects commute.

**D39. The Mac app is native SwiftUI and AppKit, connected to Rust through UniFFI.**
Settles plan open question 8. The engine must run in the same process as the interface for the transport and immediate playback, and a native app gets CoreAudio, precise mouse handling and fast drawing without a web layer between them. UniFFI generates the Swift bindings, including callback interfaces for the change feed, so no C bridge is written by hand. Swift holds no model logic: it displays what the host reports and sends commands.

**D40. Reverb impulse responses come from a documented Rust generator.**
The Python reverb draws its noise from numpy's seeded Gaussian generator and shapes it with scipy's STFT. Reproducing numpy's generator bit for bit in Rust is possible but fragile, so the Rust reverb uses its own documented, seeded generator. Its impulse responses are statistically equivalent to Python's rather than sample-identical, and reverb parity is tested on decay time, spectrum and energy. Renders remain reproducible from the document (D17). The Rust reverb also replaces D28's fixed partitions with non-uniformly partitioned convolution, a short first partition followed by growing ones, which computes the same convolution with at most one small block of latency instead of up to 1.4 seconds.

**D41. The session host saves before it replies, crossfades into each edit, and keeps history only while it runs.**
Recorded with M3 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). A host writes canonical `song.yaml` after each command and before replying, so anything reading the file, including the Python tools, sees the change at once; a debounce arrives with drag gestures in M5, where it pays. Every edit recompiles the song, reusing prepared sample audio, and the audio thread crossfades into the new version over 5 ms at the same beat, which is the smoothing D37 asks for; sending parameter changes without recompiling is a later optimization. Stops, locates and loop jumps let the old position ring out for 5 ms without starting new hits, and voices picked up mid-sample fade in over 5 ms while hits at the new position play in full. Handles are written `@N` and last for one host session. Headless commands address objects by index or ID, and they have no undo history or change log, because both are ephemeral host state (D36). A file edited outside the host that does not validate pauses edits until it is fixed, rather than being overwritten. An edit that only reorders keys, such as pads, is a change although `project_sha256` ignores key order.

**D42. The app is a Swift package around an embedded host, which pushes each revision to it.**
Recorded with M4 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). The app runs the session host on a thread inside its process and reaches it through UniFFI; the same host answers the socket, so the app and the agent's `daw` commands share one song, one transport and one history. After every change the host hands the app the whole arrangement to draw, with the objects that differ from the revision before, both computed in Rust. The plan had the app re-read only the affected objects, but an arrangement is small, a pushed snapshot cannot be read half-updated, and Swift needs no knowledge of the song to stay current (D39); revisit if it shows in a profile. The app's host compiles each revision as it lands, so space plays at once, and the playhead is read from the audio thread rather than asked of the host, which may be busy compiling. Clicking the timeline sets the start position, and while playing it also jumps playback there. The app is a Swift package built by `apps/mac/build.sh`, not a hand-made Xcode project: the manifest is plain text an agent can edit and review, `swift build` and `swift test` run from a terminal, and Xcode opens the package as it is. The app delegate, windows and menus are AppKit, the bars and panels inside a window are SwiftUI, and the timeline is one AppKit view drawn with Core Graphics. The bundle is signed ad hoc and links Homebrew's libsndfile, which makes it a local build; packaging is M9. The app takes scripted clicks and key presses on its command line and can write a picture of its window, so an agent can check what the app does without anyone at the screen.

**D43. The app edits through the host's commands, and a drag is a gesture the host merges.**
Recorded with M5 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). An edit in the window is a command with origin `user`, the same commands an agent sends, so both land in one history and one change log (D38). `aaw-ffi` turns each edit into commands that address objects by handle, and works out positions from the host's exact beats, because the app holds only floats to draw with: a clip on a third of a beat stays on thirds. A drag of clips moves them by whole grid steps, so a clip off the grid keeps its offset; Option moves by thousandths of a beat. Copies go right after the span of what was copied, so a group repeats as a group. A click on a clip selects it and does not set the start position, which would make playback jump; clicks on empty lanes and the ruler still do (D42).

A level being dragged shows its value at once and sends the host the latest value at most 30 times a second, never more than one at a time, each with the drag's gesture ID. The host merges edits of one gesture and origin that land one after another into one undo step and one log entry, labeled for the whole move, and drops the step when the drag ends where it began. Each still makes a revision. Clip drags, resizes and reorders are shown as they move and sent once on release; an edit the host refuses is reported and eased back. A gesture's edits are saved 250 ms after the last one, with the next ordinary edit or on close, which is the debounce D41 left for drags. The host answers from memory, so `daw` commands see every edit at once; a reader of the file, such as git or the Python tools, can be a quarter of a second behind during a drag.

An edit of mixer values alone (a track's gain, pan, mute or solo, or the master gain) gives the compiled program new gains instead of compiling again, and the audio thread crossfades into it as for any edit (D41). On the largest local song a fader step then takes the host about 23 ms while playing, of which validating and writing the song's canonical form is most, where an edit took about 40 ms. Sending parameters to the audio thread with smoothing (D37) and compiling only the tracks an edit affects wait for M6, where devices have state to keep. The selection belongs to the app, which tells the host with `select` so that `daw status` reports it and an agent can be asked about "the selected clip"; an agent does not set it. A batch may carry a label naming what it does as a whole. Each track's sends are folded away until its header is unfolded. Volume drags cover −60 to +6 dB and sends −60 to 0, less than the model allows and an agent can set.

**D44. The engine plays one continuous stream: edits glide, tails ring on, and a change of structure fades through silence.**
Recorded with M6 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). With effects, a program has state that an edit must not throw away, so the audio thread no longer crossfades between two whole renders (D41). The transport moves only the voices. Effects run as one stream and keep their state through locates, loop jumps and stops, so a reverb rings on as it does in other DAWs; the plan's "effect state starts empty at the playhead" holds for a start from rest. The stream runs while stopped, with the timeline standing still, and rests once its output has stayed below −180 dBFS for a second.

Every edit still compiles a program and builds a renderer for it on the host thread, and the audio thread swaps to it between blocks. A compile keeps each track's voices, prepared sample audio and reverb kernels from the last one and redoes only what the edit changed, which replaces M5's mixer-only shortcut and is the plan's incremental compile: after a level or knob edit it takes under 1 ms on the largest local song. The plan's parameter messages are not a second path. A program with the same channels, devices and delays takes over in place: each device continues from the old one's state, levels, pans, sends, mutes, thresholds, makeup, feedback and mix glide to their new values over 5 ms, a filter or equalizer keeps what each section was holding and crosses from the old coefficients to the new over 5 ms, and a track whose voices changed lets the old ones ring out under the new. A track has a send for every return, so adding or removing a send is a level that fades. An offline render never takes over from anything, so it reads exact values and matches the Python engine. A program with another structure (an effect added, removed, bypassed or moved, a track or return added or removed, a delay time, a reverb's response, a limiter's look-ahead) is swapped in at the bottom of a 5 ms fade out and 5 ms fade in; devices that are still there keep their state by matching type and state-shaping settings. The Device View therefore sends such a field when its drag ends rather than as it moves. Accepted limits: automation written in the document is not smoothed (D29), a filter that a lane moves steps when an edit changes it, and a muted track given new clips in the same edit can leak for the 5 ms of the glide.

Latency is aligned by delay rather than by shifting output earlier. Only the limiter has latency. A track keyed by a source with latency renders its voices that much later, tracks are delayed to the slowest before their faders and sends, the dry sum waits for the slowest return, and the output trails the transport by the total, which the playhead subtracts along with the output device's own latency. An offline render runs that many frames longer and places each stem on the timeline. Chain outputs are cut at the session end, as the Python engine's arrays are, so a tail past the end cannot reach a look-ahead before it.

The reverb is D40's: its noise comes from SplitMix64 seeding xoshiro256++ with Box–Muller normals, and the rest of `reverb_ir` is ported as it is. Convolution runs the first 64 taps directly and the rest in overlap-save blocks of 64, 256, 1024, 4096 and 16384 frames, each covering the response from at least its own length in, so nothing waits for a block and the reverb reports no latency where Python's reports a partition. Blocks of 256 and more start two lengths in and spread their work over the following block in 64-frame slices of the reverb's own input, and the reverbs of a song do their transforms at different slices; output does not depend on the caller's blocks. Over sixteen seeds the two engines' decay times agree within 3 ms and their octave levels within about half a dB, while a single tail's low octaves vary by a couple of dB from seed to seed in either. The other five effects are ported operation for operation, with `butter` and its section pairing included. The compressor's and limiter's release hold is computed as a recurrence instead of Python's running maximum of logarithms, which loses precision as a stream grows; generated songs differ from Python's renders by at most 4e-9.

A device panel is drawn from `aaw_model::describe`, which gives each effect field a label, unit, range, default and whether a change reshapes the device's state; a test holds it to what validation accepts and to the model's defaults. The app's scripted mouse events are queued rather than sent, since a SwiftUI button follows a press by waiting for the release in the queue.

**D45. At the cutover `daw` is the Rust binary, Python reads songs through `aaw_py`, and the model is held to what the Python model did.**
Recorded with M7 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). The Python engine, effects, automation and model are deleted. `daw` is one command with two entry points, because nothing is installed on the PATH until M9: the Rust binary owns every command that reads, edits, plays or renders a song and passes `samples`, `listen`, `compare` and `check` to Python with its own arguments and output, and `uv run daw` passes everything else to the binary by replacing its process with it. The binary finds the Python of the checkout it was built from, or `AAW_PYTHON`; Python finds the release build of its checkout, or `AAW_DAW`. `init` and `describe` moved to Rust. `check` stays in Python because what it adds is measured pitch: it takes its summary from `daw inspect` and its automation warnings from the Rust model.

Python gets a song as plain data, the full dump of the validated document as dicts and lists, from `aaw_py`, a PyO3 module over `aaw-model` that the package builds for its own interpreter with setuptools-rust. Anything that takes a song validates it first, so there is no second schema and no Python object model to keep in step (D35). The module links no audio library, which is why the event schedule moved from `aaw-engine` into `aaw-model`; cargo leaves the crate out of plain workspace builds, since it only links inside a Python. Python no longer writes `song.yaml` or takes the project lock: a sample import copies the file, then adds it with `daw apply --expect`, so a running host takes it as an undoable edit and an edit made in between is not written over. `apply` gained a label, as a batch has, so the import says what it is in the change log and the Undo menu.

`daw describe` prints the JSON Schema pydantic generated from the Python model, kept as data in `aaw-model`, with the semantics text beside it. A test walks a song that has one of every model beside that schema and holds each field, default, required field, limit and choice to what validation does, so the published contract cannot drift from the model. A render report names the engine as `rust` with its version and libsndfile's, and `engine_sha256` hashes the running executable where it hashed the Python sources, so render IDs changed at the cutover. Reports from the Python engines still verify, because verification needs only the fingerprints and the audio hashes; the earlier forms D30 lists are now built in `aaw-model`.

With the reference gone, parity tests cannot run. What the Python model did is pinned instead: for 533 generated documents, their canonical YAML and fingerprints or their errors, and the outcomes of 120 generated edits, recorded from the Python model on its last day and checked against the Rust model in `tests/test_model.py`; a deliberate change is accepted by updating the pinned file. The engine is held to its own promises: generated songs are rendered whole, in other block sizes, as sections and as single channels and the results compared, and the Python suite's device, routing and automation tests now run through `daw render`, a chain alone being a song that plays the test signal once on a track with those effects. The suite drives the release build, which is the one `uv run daw` runs. The last comparison of the two engines is recorded in [../engine/README.md](../engine/README.md).

**D46. A clip's waveform is its track's sampler output, patterns are edited by the host's commands, and the browser asks Python.** Revised by D80.
Recorded with M8 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). Clips are patterns that trigger samples, so a clip has no audio file to show. Its waveform is what its track's voices sum to over its span, before the track's inserts, fader and sends. That is what the clip is, as an audio clip's file is in other DAWs; it does not change when a level or an effect does, so mixing redraws nothing; and it needs no effects to render, so it is cheap. A track's program carries an identity of everything its voices are made from (pads, clips, the patterns they play, samples and their files, tempo, rate and session length), which is the plan's "compiled identity". The host tells an embedding process each revision's program once it is compiled, and `aaw-ffi` works out peaks on a thread of its own for the tracks whose identity it has not seen: the least and greatest sample of every 64 frames as signed bytes, and coarser levels four times as long each, so the app reads a few buckets for a column at any zoom. A sum louder than full scale is drawn as full scale. The app is told each track's identity at the revision with the peaks it has not been sent, and draws a track's peaks only for the revision they are of. Until then a clip keeps the waveform it had, sourced from where the clip was, so a moved clip carries its picture along instead of blinking, which is the plan's placeholder made unobtrusive; a new clip shows a line. Only the latest revision is worked on, and peaks are kept for sixteen revisions, so undo finds them.

The pattern editor shows the pattern of the clip last selected with a row for each pad of that clip's track, because a pattern names pads and only a track gives them samples. A row's kind follows what the engine does with the pad: steps where a one-shot pad plays its sample through, events with a length where the pad is held, since a step has no length and the model refuses it there, and events by note where the sample has a root note. Steps and events of a pad both play, and both are shown. Edits are the host's commands as they were (`pattern.steps`, `event.add`, `event.set`, `event.remove`, `set`, and batches of them), composed in `aaw-ffi` from the song's exact beats: an event moves by whole steps of its pattern's grid, so one on a third stays on thirds, and a step row keeps its spaces and bar lines when a step in it changes. A row left without steps is removed. Changing a pattern's length or step rewrites its rows in the same batch and refuses a step that would not fall on the new grid. A click on a clip shows its pattern and a click on a header shows devices, as in Live; the two marks on the panel change between them. Double-clicking an empty part of a track makes a pattern one bar long and a clip of it, which with the browser lets a person build a beat without an agent; Own Copy duplicates a shared pattern for one clip. One event is selected at a time. Delete in the pattern editor removes the selected event and never the clip. `daw status` lists the selected event with the clip, so an agent can be asked about "the selected note".

The browser lists what Python's `daw samples search` finds and copies a chosen file with `daw samples import --copy-only`, so the index, its filename hints and the copy stay in one implementation (the plan's "sample import stays in Python"), at the cost of starting Python: about a third of a second a search, which runs off the main thread after a pause in typing. `aaw-ffi` then makes one batch as the person: the sample, the pad, and a track if it is new. A sample's measured pitch is its root note unless its name says it is a drum, an effect or a loop, since a root note makes a pad's row notes rather than steps. A click plays the sample's file through the system, not the engine: a preview of the file as it is, which the song's devices have no part in (D37 is about the song). The index is found from the song: `AAW_LIBRARY`, or `.daw/library.sqlite` in the song's folder or the nearest above it. Open question 5 of the plan, whether import should move into Rust, stays open.

The timeline stays Core Graphics. Measured, the cost was not filling pixels: it was sorting the clips into drawing order and laying out their names on every frame. The order is now kept between frames and each string is laid out once as a line that takes its color from the context, so a fade or a muted clip uses the same line; a clip's name is cut at the clip's edge rather than shortened with an ellipsis. Waveforms are drawn as one batch of rectangles a clip, a device pixel wide, from the columns in view. The app's `--measure` scripts the same run of zooming and scrolling for any song and reports draw times, frame spacing and how long waveforms trailed the last change, so the timeline can be measured again without anyone at the screen.

A batch could not add to what it had just added, because a new track, return or pattern had no lists until validation gave it its defaults; they now start with empty ones. An event's label names its pattern, pad and beat rather than its place in a list.

**D47. The app carries `daw` and its libraries, and the command line tool is a link into the bundle.**
Recorded with M9 of [Rust-Swift-Update.md](archive/Rust-Swift-Update.md). The bundle holds the binary the tests drive, at `Contents/Helpers/daw`, rather than a build of its own, so what is packaged is what was tested. libsndfile and the libraries it links are copied from Homebrew into `Contents/Frameworks`, not linked statically or built from source: the copies are the build the engine is tested with, whose decoding its renders depend on. The app and `daw` name them by `@executable_path/../Frameworks`, and the copies name one another by `@loader_path`. macOS resolves the first from where the binary really is, also when it is run through a link, which was checked before relying on it; so a link to `daw` needs no wrapper script. The bundle is therefore for one kind of processor and for a macOS no older than Homebrew built for, and the build refuses a bundle whose parts ask for a newer macOS than `Info.plist` states, since that would otherwise show only on someone else's Mac. The libraries' licenses go in the bundle with them, several being LGPL.

The command line tool is a link at `/usr/local/bin/daw`, not a copy: `daw` and the host in the app speak a protocol that is not versioned, so the two should be one build, and a link is replaced with the app. `/usr/local/bin` is on every shell's PATH without editing a profile, at the cost of an administrator's password, asked for through AppleScript's `do shell script ... with administrator privileges`, which needs no helper to install and no entitlement. The same command string runs without it where the folder is the person's, which is how the tests run it. The menu item says what is at the link before changing it and never replaces a file that is no link, which somebody else put there. An app that macOS runs from a temporary copy is told to move first, since a link to that copy would not last.

An ad hoc build is signed without the hardened runtime, because with it macOS refuses libraries that share no team with the app, and an ad hoc signature has no team. A certificate signs every part with the hardened runtime and a timestamp, as notarizing requires, and no entitlements: the app records nothing, loads no code but its own and runs Python as a separate process. Distribution is a zip made with `ditto`, notarized with `notarytool` from a keychain profile and stapled, outside the Mac App Store as the plan says. Python stays a `uv` environment, as the plan says: the bundle's `daw` and the browser look for it in the checkout the bundle was built from, or at `AAW_PYTHON`. A shared app therefore has no sample browser, library or perception commands until Python is bundled, and its binaries name the build Mac's checkout; both wait for a need to distribute.

**D48. A compressed file is decoded once at import, by a separate program, into float WAV.**
Recorded with feature 1 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md). The engine keeps reading PCM through libsndfile and nothing else: `daw samples import` decodes `.m4a` and `.mp3` into the project, so every later step reads the same frames and the audio is lossy-encoded no more often than it already was. MP3 goes the same way although libsndfile reads it, to keep one path. The decoder is a separate process, `afconvert` first and `ffmpeg` where it is missing, so nothing is linked and the bundle's libraries (D47) do not change. Both leave out the encoder's padding before the audio, which was measured on generated files: the decoded audio lines up with the original to the frame. `afconvert` also restores the length exactly; `ffmpeg` keeps about 15 ms after AAC, which no position depends on. The copy is 32-bit float WAV at the file's own rate, about 85 MB for a four-minute stereo song, because a decoded lossy file can peak above full scale and an integer format would clip it; FLAC cannot hold floats.

The copy is named by the original's hash, so a file imported twice is decoded once, and whichever decoder made it first is the one a project keeps. The song records the copy's hash as `sha256`, which the engine checks as before, and the original's as `source_sha256`, a field written only for decoded files. It is the first field added since the cutover (D45): the fingerprint forms gained an entry for dumps without it, the pinned corpus was updated for the one added form, and its canonical YAML, `project_sha256` and every earlier fingerprint were compared between the two builds and are the same. Import now opens what it copies and refuses a file the engine cannot play, and `daw check` warns of one in a song, so the error no longer waits for a render. The library index still lists only WAV, AIFF and FLAC.

**D49. A song's beat map is a steady grid fitted to its transients, measured with numpy and kept beside the audio.**
Recorded with feature 2 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); the method and what it was checked on are in [beat-map.md](features/beat-map.md). The tracker is written with numpy, as the sample analysis is, and no learned model was added: the songs this is for are made to a click, where one grid fitted to hundreds of transients is placed more finely than any tracker follows single beats, and a model would bring a large dependency and, for some published ones, a non-commercial license. A learned downbeat tracker stays an option if downbeats prove unreliable on real songs; none were available to measure, so the thresholds rest on generated songs and five local renders.

The steady grid is the first hypothesis and is kept when the onset envelope stays on it window by window. The first test of steadiness was the spread of the transients, and it failed on a local song whose samples have attacks several milliseconds apart: a change of instrument read as a change of tempo. A song that leaves the grid is followed through eight-beat windows and each beat takes its own transient, so the output is always a time for every beat and a steady song is the case where those times are the grid's.

Half or double time is not decidable from audio: among the local songs one at 144 BPM and one at 86 have the same pattern of strong and weak beats at the faster tempo. The rule taken is the faster tempo up to 180 BPM when its beats' transients are on the grid, because the faster grid contains the slower one's beats while a slower grid can sit on the backbeat, as it did on a 176 BPM song before the rule. The other tempo is reported, and `--bpm` settles it. The downbeat is reported with a confidence for each of the four places for the same reason, and `--downbeat` corrects it; both corrections are saved with the map.

The map is a file beside the audio, `NAME.beats.json`, not an entry in the song: it is a measurement of the file, valid while the file's hash and the measuring code are unchanged, and the checks and the app can find it from the sample's path. Which beat an edit cuts on is the song's business and belongs to the audio regions of feature 4. A file in the library index gets no file beside it, since the library is never written to.

**D50. An export is a render under a name, changed by at most one gain, with its record beside it.**
Recorded with feature 6 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); the command is described in [export.md](features/export.md). `daw export` runs in Python beside perception, which already reads renders and measures loudness, and encodes with a separate program as import decodes with one (D48), so nothing is linked. It exports the latest full render and renders first when the song has changed, so one command gives a current file and the record names the render it is.

The level is one gain or none. Limiting stays in the song, as a master `limiter` whose effect is in the render, the stems and the report; an export that limited would give a file no render matches. Of the three policies the features document lists for an edit of a mastered song, the trim is `--gain`, matching the source is `--match`, and the limiter is the song's. A gain is held under a ceiling and the export says how much it held back, rather than clipping or refusing. The default is the render as it is. Sixteen bits are dithered from a fixed seed, so the same audio gives the same file, as renders do.

The formats are the document's: 16- and 24-bit WAV, AAC and MP3. Its open question 6, which formats are asked for, is answered by offering those until a request needs another. An existing file is not written over without `--replace`, since an export is the person's and a render is a cache.

**D51. A join is checked twice, from the beat map and from the audio, and is found from the pads until there are audio clips.**
Recorded with feature 5 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); the command is described in [join-checks.md](features/join-checks.md). `daw joins` is a command of its own and not part of `daw listen`, whose report is long and about the mix: an agent checking an edit wants the joins and the length and nothing else.

Whether the beat carries across a join is answered from the song's beat map placed through each region, which is arithmetic on a grid fitted to hundreds of transients (D49) and is exact to the map. The audio is measured as well, as the distance of the transients either side from session beats, because the map says where the beats were meant to land and the stem says where they did. The two are reported apart and flagged at different sizes, 1 ms and 3 ms: attacks differ by instrument, so the audio's figure moves with the sound either side of a join while the map's does not.

The click measure is the step at the edges of the two parts against the 3 ms around each. The first version took the largest step inside the fade against the audio before it, as the features document describes, and flagged every clean join: a cut placed a few milliseconds before the beat has that beat's transient inside the fade out. A fade adds no step by construction, so what is looked for is an edge that jumps.

With the sampler a join is two hits in a row on a track that play one sample from different pads, each a beat or more, meeting within their fades. That is an inference from how an edit is built today, and it moves to the audio clips of feature 4 when they exist; the measurements do not depend on it.

**D52. A personal skill lives in the checkout, ignored, and holds only what is personal; the mechanics are a tracked skill and `daw describe`.**
Recorded with feature 7 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); see [skills.md](features/skills.md). The features document's open question 5 asked whether a personal skill lives in the checkout or in the person's own skills folder. Both work, and the checkout is the default, because the skill names paths under `projects/` and `content/` and runs `uv run daw`. Git ignores every folder under `.claude/skills/` except the generic ones `.gitignore` names, so `git add -A` cannot commit a personal skill and a generic one is a deliberate, reviewed addition. Nothing else under `.claude/` is ignored, since a project may want to track its settings.

What a skill needs is split three ways. What to do, in order, with what to tell the person, is the tracked `song-edit` skill. How each step works is in `daw describe`, as it already was for the sampler, effects and automation: `edit` for building an edit from parts of a song, `beats`, `joins` and `export` for those commands. Who asks, which sounds, the limit and the file names are the personal skill's, which refers to the generic one. The describe texts are held in the Rust contract beside the others although three of the commands run in Python, so there is one place an agent reads.

The generic skill was run once by hand from its own text on a generated song, encoded to AAC, through the command line only: import, beat map, cuts at downbeats, two parts joined, a limiter, the join check and an export. That run found three things the guidance lacked, which `daw describe edit` now says: a new song's master gain is -6 dB, its rate is 48 kHz, and an event's default velocity plays a part 2 dB down. With those set the edit measured within 0.2 LU of the song. The reusable sounds a skill names live under `content/`, ignored, and are imported by path into each project.

**D53. A pad stretches in time when its audio is prepared; Signalsmith Stretch is linked and the default, and Rubber Band is a program a song can name.**
Recorded with feature 3 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); see [time-stretch.md](features/time-stretch.md). The person chose both stretchers with the built-in one as the default (the document's open question 2). Signalsmith Stretch is MIT, so it is linked into `daw` and the app, through the `signalsmith-stretch` crate pinned to one version. Rubber Band's finer engine is the open-source reference for full mixes and is GPL, so it is never linked: the engine runs the installed `rubberband` program on a temporary file, as import runs a decoder (D48). Which sounds better at a few percent is to be judged by ear on real songs, and `session.stretcher` lets the same edit be rendered with each; nobody has listened yet. ffmpeg's `atempo` was measured and left out, because it moves beats by 10 ms and more.

The mode is on the pad, `stretch: repitch` or `preserve_pitch`, beside `source_bpm`, which already made a pad follow the tempo. A speed change is then a change of `session.tempo`: everything is placed in beats, so an edit built at the song's tempo keeps its joins at any tempo, and sounds without `source_bpm` keep their length. Stretching is done in `Job::make`, where repitching is, so it is cached by the same key, never runs on the audio thread, and playback equals the render (D37).

Each region is stretched with 0.2 s of the file either side and the margins cut off, rather than the whole file once. The measurements showed either would keep the beats, to under half a millisecond of each other, so the choice was for what fits the cache and costs only what is used, and for keeping a stretcher's own edges out of the audio at a join. At a ratio of one the stretcher is not run, so a song at its own tempo is bit for bit what it was. A render's report names the stretcher and its version only when the song stretches, so songs that do not keep their render identity. The two new fields are a new fingerprint form (D45, D48); the pinned corpus and the outcomes of the pinned edits were compared across the change and are the same.

Linking a C++ library added `libc++` to the app's link line. The licenses of Rust crates are still not gathered into the bundle, which now matters for one more of them.

**D54. Audio clips are a list of their own on a track, a clip starts a lead before its beat, and a join belongs to the clip that enters.**
Recorded with feature 4 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); see [audio-clips.md](features/audio-clips.md). The person chose audio clips in the schema with the engine and commands now and the Mac app's part later (the document's open question 3). The document sketched them inside `clips`. They are `tracks[].audio` instead: a pattern clip and an audio clip share only a beat, and every reader of `clips` (the schedule, the host's clip commands and handles, the app's arrangement and its edits) reads pattern clips, so one list of two kinds would have changed all of them and the app at once. With a list of their own nothing that existed changes, the app opens and plays such a song before it can draw one, and the generic `set` and `remove` already move and remove a clip.

A clip is written with the beat it belongs on and the song's own time for that beat, and `lead_ms` says how much earlier it starts. That is the document's "place for the cut a few milliseconds before the beat". With pads the same thing was an event at a fractional beat and a start a few milliseconds before the song's beat, arithmetic that `daw describe edit` had to teach; now `at` is the beat and nothing is offset by hand.

The first version gave a clip one lead for both its edges, and a join then depended on two clips agreeing: after a split and a later crossfade the neighbors had 5 and 6 ms and a millisecond of the song went missing. So where a clip leaves is not its own setting: a clip that ends where another of its track begins leaves from where that one starts. A join is at one place whatever the clips say, a crossfade sets the entering clip's lead and fade in and the leaving clip's fade out, and a split needs no fades at all.

A clip's place on the timeline is rounded to a frame once, from the exact place of its beat and the frame of its file it starts on. Rounding the beat and the lead apart put the right half of a split one frame late whenever its beat fell between frames; rounded once, a clip split at whole, fractional and decimal beats renders bit for bit as the whole one, which the tests hold.

Fades are equal power by default because an edit joins different parts of a song, and linear is there for audio that is the same. A clip plays its file at full level, without a pad's velocity and mono pan law, so a song on a clip is as loud as its file. `audio cut` is the request "take out the eight beats around 1:20": it splits, removes, closes the gap and crossfades as one undoable command. It is of one track; what should move with it on other tracks is moved by hand for now. `audio` on a track is a new fingerprint form, and the pinned corpus and the pinned edits' outcomes are the same across the change.

**D55. A point's shape bends the linear segment after it, by one number.**
Recorded with the last of the smaller gaps under feature 8 of [new-features-2026-10-01.md](archive/new-features-2026-10-01.md); see [automation.md](features/automation.md). A sweep that holds back and then opens took several points; it takes two and a `shape`. The shape is a number from -1 to 1 on the point, beside `curve`, and not more names in `curve`: one number covers a little bend and a strong one, and zero is the line as it was, bit for bit. The segment's progress is raised to the power 4 to the shape, and below zero the curve is mirrored, so a shape and its negative are the same bend from either end. The bend is in the parameter's domain, as a line is, so a frequency sweep bends in octaves. It is a new field of a point and so a new fingerprint form; the pinned corpus and the pinned edits' outcomes are the same across the change. The Mac app reads a point as held or not and draws a shaped segment straight.

**D56. Each kind of statement has one home in the docs, and there is no requirements file or forward spec.**
Decided October 2, 2026. Twenty-three files in one folder held four visions, three plans and a specification that still said nothing was built, and unbuilt work was spread over a dozen "Limits", "Later" and "Open questions" sections with no single list. Now [concept.md](concept.md) is the end state and never says what is built; [backlog.md](backlog.md) is everything not built, with an inbox for ideas; a file in [features/](features/) is one feature's design before it is built and its reference after; [architecture.md](architecture.md), the two READMEs beside the code and `daw describe` say what exists; this log says why; [completed.md](completed.md) says what was built and what a person tried; and superseded documents go to [archive/](archive/). A separate requirements file was considered and left out: it would restate the concept and be a second place to go stale, and the part the concept lacks, how to know a thing is done, belongs to each feature as "Done when". A hand-kept specification of the future was left out for the same reason; design for what is not built goes in the feature's file, and what is built is described beside the code. No new dated notes or plan files: the outcome of a conversation goes to the concept, this log or the backlog. [AGENTS.md](../AGENTS.md) carries the rules.

**D57. The app opens on an Untitled project, which is a folder from the start; a project can be kept anywhere; the agent works at two levels.**
Decided by the person on October 2, 2026, and described in [concept.md](concept.md); nothing of it is built. The app launches on a new Untitled project, as Ableton does, and closing one that was changed asks Save, Delete or Cancel. Untitled cannot mean "only in memory": the host saves after every edit, and its socket, its lock, imported samples and renders all go by the project's folder, so an Untitled project is a real folder in the app's own data folder, and naming it is a move the host makes. A project is a plain folder with a project file under the app's own extension, and it can be saved in any folder, so the app keeps an index of the projects it knows and the sample index moves out of the folders above a project into the app's data. The agent has one home for everything the person makes (instructions, taste, skills) and a level for each project (`SONG.md`, its skills, its conversations), and it is given a skill's name and description, reading the body only when the skill applies. The conversation in the window starts in the project's folder with the home attached; an agent in a terminal starts in the home and finds projects through the index. Signing in to an agent from the app waits on the agent companies' terms; until then the app uses the agent the person installed and signed in to. The designs are in [features/new-and-untitled-projects.md](features/new-and-untitled-projects.md) and [features/agent-panel.md](features/agent-panel.md).

**D58. Save As… copies a project that has a name and moves an Untitled one, commands for the old path follow the window, and a blank project is 32 bars.**
Decided with the person on October 2, 2026, before backlog item 1 was built; see [features/new-and-untitled-projects.md](features/new-and-untitled-projects.md). The design had Save As… as a move in every case, because one host command would then serve both. The person expected what every app does: Save As… on "My Beat" with the name "My Beat 2" leaves both. So a project that has a name is copied and the window carries on in the copy, and only an Untitled project, whose hidden folder nobody wants left behind, is moved. To the person the two are one thing: the project now exists under the name they typed.

A copy raised a question a move does not. An agent in a terminal names the project by its folder, and after a copy the old folder is still a project. Its next command could edit the original, which nothing shows and which alters the version the person had just set aside, or follow the window to the copy. It follows the window: the host keeps answering at the old path, applies the command to the copy and says where the project is now. The cost is that the original cannot be reached by its path while the copy is open, and it is released when the copy closes or the original is opened in the app. D57's "the host answers at the old path until it closes" is kept for both cases.

Save As… is a change in the log and not an undo step, so that Undo after it is still the last edit. Undo steps are whole revisions, each with a title, so every revision in the history is given the new title; otherwise undoing an earlier edit would bring "Untitled" back. A step that only changed the title is dropped.

A blank project is 32 bars, 64 seconds at 120 BPM: `session.length_beats` must cover every clip, and the schema's default of four bars is too short to begin in. A song that grows as clips are placed is the better end state and a backlog line, since today the arrangement holds a dragged clip inside the song. Untitled projects a crash left are listed in Open Recent and launch asks nothing, so that launch is the same every time. Whether an Untitled project can be deleted unasked is judged by what it holds, a blank song and no other file, and not by its revision, so a track added and undone leaves nothing to ask about. The design's `AAW_UNTITLED_DIR` became `AAW_DATA_DIR`, since the index needs the same separation from the person's own as the Untitled folder does.

**D59. In the app a file dropped on the timeline is an audio clip, the song grows to hold it, a clip is drawn to where its sound ends, and it draws its file.**
Decided with the person on October 2, 2026, before backlog item 1 was built; see [features/audio-clips-in-app.md](features/audio-clips-in-app.md). The person dropped a WAV on a track and looked for it on the timeline, where every other DAW puts it. So the timeline makes an audio clip and the headers make a pad, which is Ableton's split between a lane and a device: the place says which, nothing has to be held down, and the + by a sample in the browser keeps making a pad. Holding Option to choose was the alternative, and nothing on screen would have shown it.

A blank project is 32 bars (D58), and a song dropped on it would have been cut off at 64 seconds. The app's edits therefore lengthen the song, in the same undo step, when an audio clip's sound would end past the song's end: a drop, a move, a trim, a copy and a new tempo of its own. It is the app's edits that do this and not the host's commands, because an agent that places a clip sets the length it means, and the `song-edit` skill does. Pattern clips keep to the song's end until the backlog line for them is built. A sound that passes the end by less than the song's end fade does not lengthen it. Without that slack a clip an agent ended on the last beat, whose 8 ms fade out comes after it, grew the song by a bar whenever it was moved to another track.

The song writes a fade out after the place a clip leaves (D54), which is what a join needs. A person dragging a fade out expects it inside the clip, ending where the clip ends. The app draws a clip to where its sound ends, and its edits keep that end: an end is trimmed to the fade's length before the beat, and a longer fade out moves the place the clip leaves earlier. The document's meaning is unchanged and an agent's commands write what they wrote. The cost is that an end trimmed in the app to a beat leaves 8 ms before it, where `daw audio trim --end` leaves on it; a join by hand is then a short dip, as two abutting clips are in other DAWs, and a crossfaded join is still made with `daw audio crossfade` or `cut`.

An audio clip draws its file's peaks, not its track's (D46). A track's peaks are of what was compiled, so an edge dragged outward would have shown nothing until the edit landed and the track was worked out again, and every trim of a three-minute song would have summed the whole track for a picture. The file's peaks are read once, in blocks, and a track of audio clips alone has none of its own worked out. They are of the file as it is: the clip's gain scales the drawing, and its fades are drawn over it.

A clip moved to another track needed a command of the host's, `audio.move`, since removing it and adding it again would have given it a new handle, and the selection and the eased move in the window go by handles. The beat map is drawn when `daw samples beats` has left one beside the file and the app measures nothing, so that this adds no Python to what the standalone app has to carry. Split and the clip's panel were not in the item's wording; the person took both into it. The app's scripted input gained `--drop`, which hands a file to where a drag would have left it, because the feature began with a drop and a drag cannot be scripted.

**D60. MIDI note clips are editable independently of their instruments, and ordinary clip copies are independent.**
Required by the person on October 3, 2026, for the programmed-music workflow described in [concept.md](concept.md). A MIDI track must support creating and editing note clips before any instrument is attached, then switching between a sampler and future software instruments without changing the notes. Both the person and the agent must be able to create melodies, chords and rhythms, edit individual pitches, starts, durations and velocities, and place notes ahead of or behind a beat without forcing them onto a grid. Copying and pasting a clip must permit changing the copy without changing the original.

The reason is the person's established Ableton workflow: musical material remains editable and reusable independently of the sound playing it, and an agent must have the same editing freedom as the person. These are product requirements; the current pad-dependent events and shared-pattern clip duplication do not fully satisfy them. The exact schema, commands, migration and instrument interface remain to be designed. This decision does not select raw MIDI messages or separate `.mid` files as the project's internal storage format.

**D61. The first MIDI note model stores numeric pitches without enharmonic spelling.**
Chosen by the person on October 3, 2026. MIDI pitches 0–127 are sufficient for the required note editing, chords, transposition, timing, independent copies and instrument replacement. The first round should establish how well the agent works with this standard representation before adding a spelling field. A piano roll can derive note labels under a documented octave convention without storing a second pitch value. C-sharp and D-flat therefore share a pitch value in this version; preserving their intended spelling is deferred unless concrete agent tasks show a need. See [MIDI tracks and note clips](features/midi-clips.md) for the proposed build and its checks.

**D62. A note clip owns its notes, note clips are where patterns are headed, and the sampler maps notes to pads.**
Proposed in review of the MIDI plan and accepted by the person on October 3, 2026. D60 required independent copies and left the schema open; whether notes live inside the clip or in shared content the clip refers to decided the schema, copying, undo and the app's view, so it is settled before building rather than at its start. Notes live inside the clip: a copy is a deep copy with new IDs, which is how Ableton behaves and the simplest way to make a copy independent. Linked copies stay in Later.

The plan kept patterns working beside note clips without saying whether both would last. Two kinds of clip, two ways of writing pitch (note names in events, numbers in notes) and two editors would each have to be kept. Note clips are the end state; pattern clips are the format of songs made before them and keep working, unchanged, until a later item converts a track when asked. That conversion, and a drum `.mid` file, both need the sampler to give a note number a sound, so the sampler maps a note or a range of notes to a pad, pitched or not, rather than playing pads by name. The work is split into three backlog items — the model and commands, the app, Standard MIDI files — so the agent can program parts before the piano roll is built. See [MIDI tracks and note clips](features/midi-clips.md).

**D63. A note past its clip's end is kept and silent, note clips do not repeat, IDs are given when an object is made, and either schema version is read.** Revised by D64
Settled on October 3, 2026, in building backlog item 1 of [MIDI tracks and note clips](features/midi-clips.md), whose open questions these were. A note sounds until its end or its clip's, whichever is first, and a note that starts at or after its clip's end is kept and does not play, as Ableton and Logic keep what a shortened region hides: making a clip shorter and then longer loses nothing, and `daw check` and `daw note list` say which notes are outside. A note cannot start before its clip, since the model refuses negative beats, and a command that would move one there is refused rather than clamped. Note clips have no `repeats`: a copy is how a phrase plays again, and clips that loop wait for the Later item that loops audio clips too, so that one rule serves both.

The plan had the host assign IDs. The host does, when a command makes a clip or a note, so that a later command of the same batch can name it; and the model gives an ID to any clip or note that arrives without one, through `daw apply` or a file edited by hand, so that every path to the song ends with IDs. Both give the next number after the highest: `clipN` across the song, `nN` within a clip. A note's ID is its clip's, so a copy's notes keep their IDs inside a clip with a new one; the plan said copies receive new note IDs, and a note in the copy is a new object either way, named by its clip's path.

A song with a MIDI track is saved as `schema_version: 2` and one without as 1, byte for byte as before. Either version is read whatever the song holds, since the version follows from the content when the song is written. An engine from before MIDI tracks refuses version 2 with its own message, "Input should be 1". The one change to the pinned corpus is a document `schema_version: 2` with an empty session, which was refused and is now read.

**D64. A note may start before its clip, a sample dropped on a MIDI track becomes its instrument, and Copy takes what it copies.** Revised by D66 and D79.
Settled on October 3, 2026, in building backlog item 2 of [MIDI tracks and note clips](features/midi-clips.md). Revises the part of D63 that a note cannot start before its clip. Its open question was what a clip shortened from its left edge does with the notes it passes. The person chose what Ableton and Logic do: the notes keep their places in the song, the ones the edge passed are kept and silent, and moving the edge back brings them back. A note is placed from its clip's start, so such a note has a negative `at`. The two other answers were to stop the edge at the first note, which no other DAW does, or to delete the notes, which a shortening from the right does not. A note that starts before its clip does not play even where it would last into the clip, as a note-on before an Ableton clip's start is not heard. The song format stays at version 2: a negative `at` is new only to note clips, which were made the same day. `daw clip trim` moves either edge to a song beat, and `daw check` and `daw note list` name notes on either side of their clip.

The piano roll is a view of its own (`NoteEditor`, `PianoRollLayout`) rather than the pattern editor with one row: it shows all 128 notes and scrolls up and down, takes in notes outside the clip, and selects several notes at once, none of which the pattern editor's rows do. It reuses the pattern editor's drawing, its grid choices, its typed fields and its bars. The grid is the piano roll's own setting, since a note clip has none in the song; notes are added on it and moved by whole steps of it, so a note off the grid stays as far off it, and Option takes them off it, as on the timeline.

A sample dropped on a MIDI track's header, or added to one with + in the browser, becomes the track's instrument: a sampler that plays it on every note, from its root note when it has one, else as it is, in place of the instrument the track had. That is what dropping a sample on an Ableton MIDI track does, and it is the attach, swap and remove the item asked for, with the device panel's × to take it off. A kit of pads on notes is made with `daw instrument map` for now. A sample dropped on a MIDI track's lane goes to a new track as an audio clip, since a MIDI track holds note clips only.

Copy takes the clips or notes as they are when it is pressed, not the objects, so that Paste works after the originals changed or were removed and Cut is Copy and Delete, as in every app. Clips paste at the start position on the track whose lane was last clicked, and notes where the piano roll was last clicked in the clear; each paste moves that place to the end of what it pasted, so pastes follow one another. Add MIDI Track is ⇧⌘T, as in Ableton.

**D65. A MIDI file is one part: its notes and velocities become one note clip, and what the song cannot hold is reported, not asked about.**
Chosen by the person on October 3, 2026, in starting backlog item 3 of [MIDI tracks and note clips](features/midi-clips.md), whose open question was how a file's channels and tracks become MIDI tracks and clips and what happens to data the song cannot hold. The person's use is a single part: a chord progression played on a synth, saved and brought into a new song. What matters is the notes and their velocities, not the file's tempo. So an import reads a type 0 or type 1 file whose notes are all in one file track and on one channel, and makes one note clip of them; a file with notes in more than one is refused with a message that names its parts, rather than guessing how to combine them. The file's tempo is not taken: positions in a MIDI file are in beats, so the part lands on the same bars at the song's tempo, and the reply gives the file's tempo for information.

The feature file asked for performance data to be reported or refused before an import was committed. For one part played on a synth that would ask about nearly every file, since program changes and controllers are in most of them, so the notes are imported at once and the reply counts what was left out, by kind: the sustain pedal, pitch bend, controllers, program changes and the rest. Export writes one note clip, as Ableton's Export MIDI Clip does, so that a part written here can be kept and used in another song, and so that an export and an import can be checked against each other. A file of several parts, a tempo taken from a file, and the pedal, pitch bend and controllers kept are one line under Later.

**D66. A sample dropped on a MIDI track plays at every note's pitch, as it is at middle C.**
Chosen by the person on October 3, 2026, after dropping a metal percussion hit on a MIDI track and hearing it the same on every note. D64 made the sampler pitched only when the sample had a root note, and the browser leaves out the measured pitch of a drum, an effect or a loop, while a file from the Finder is never measured, so most hits played as they are. The person's expectation is Ableton's Simpler: whatever the sample is, a single note or not, it plays as it is at middle C and higher or lower on the keys around it. So a sample dropped on a MIDI track's header, or added to one with +, becomes a sampler with one pitched entry for every note, and a sample new to the song comes in with no root note, its measured pitch not taken. A pitched map entry whose sample has no root note repitches from middle C, C4 or MIDI 60, in place of being refused, so nothing has to write a root note the audio disagrees with and `daw check` stays quiet; a sample the song already gives a root note keeps it. The pad plays to the end of its sample, as the hit the person approved did, rather than stopping at the note-off. A pattern's pitched event still needs a root note. Setting the root note and the mode in a sampler's panel belongs to the Sampler device on the backlog.

**D67. The metronome is a playback monitor, outside the song and its effects.**
Implemented October 3, 2026, for the requested transport-bar toggle. The host
owns the enabled state for each open project, initially off, and the audio
player adds the click at the audible beat after the song's processing. This
keeps it in time without a UI timer, lets an empty project click, and prevents
master effects, faders, renders or stems from changing or capturing the timing
reference. The setting survives live song edits and transport commands but
is not saved or added to undo. See [metronome](features/metronome.md).


**D68. The browser's Folders are shared sample sources across projects.**

Chosen by the person on October 3, 2026. Add Folder registers local sample
directories, including several locations, in the app's shared library. The
browser's search bar and the agent search the same index, over all registered
sources or a selected subset. This extends D57's app-owned sample index with
explicit source registration; it is workspace configuration, outside a song and
its undo history. Removing a source never deletes its files or project copies.
The browser item includes this shared index and folder management; porting search
and import from Python to Rust remains the standalone-app item.

**D69. The Synth is a Serum-class instrument with text patches in the workspace library, built after the Sampler device.**

Chosen by the person on October 3, 2026, when the synth was planned
([features/synth.md](features/synth.md)). The concept's "subtractive synth"
widens to a polyphonic synthesizer with oscillators in unison, wavetables, a
filter, envelopes, LFOs, a modulation matrix, macros and an effect chain of its
own, so a patch sounds finished on any track; the patch is a plain mapping in
the song under `instrument: {synth: ...}`, self-describing as effects are
(D44), with modulation amounts in each target's own unit. Oscillators are a
mapping by ID, as pads are, not fixed slots: a smaller schema and stable
command paths. Saved patches are YAML files in
`~/Music/AAW/library/patches/`, the first folder of the workspace the concept
describes, made when the first is saved; the hidden app data folder and a
registered folder were the alternatives. Factory patches ship in `daw`.
Wavetables start as built-in tables and a single cycle from a WAV; multi-frame
tables with a position stay in Later. The detail panel's height becomes
draggable rather than the synth opening a window of its own. A MIDI keyboard
is not part of it; the host's `note.preview` is its hook. It is backlog item 2,
after the Sampler device, whose wide instrument panel it shares, and is built
as four pull requests: engine and commands, patches, panel, then unison,
wavetables and the patch's effects.

**D70. The Sampler's root note is the sample's, and its pad is named after the sample it plays.**

Chosen on October 3, 2026, in building the [Sampler device](features/sampler-device.md), whose open question was whether the root note belongs to the sample, as the song keeps it, or to the pad, as Ableton keeps it per instrument. It stays on the sample: the song keeps one truth about a file, `daw check` compares that note with the pitch it measures from the file, and the browser's measured pitch and `import --root-note` write the same field, so Measure in the panel, the agent's `daw set samples.S.root_note` and `daw check` agree on one value. The cost is that two Samplers of one file cannot have different root notes; Transpose on the pad is per instrument and covers that. A `root_note` on the pad that overrides the sample's is a model change to make if a song shows the need. A sample loaded over the Sampler's renames the pad after the new sample, so that `daw pad set` names the pad for what it plays, and carries the panel's settings over, as Simpler keeps its settings when a sample is swapped; the start and the end go back to the whole file, since they were of the old file. Every field of the pad is sent when a drag ends, as a delay's time is, because any pad change gives the sampler new voices and a playing song fades through it.

**D71. One filter a voice with a per-oscillator bypass, control values every 16 frames, and an envelope's modulation taken at the note's start.**

Decided on October 3, 2026, in building the Synth's engine
([features/synth.md](features/synth.md)), whose open question was a filter
per oscillator route, two filters, or one. One filter a voice, with each
oscillator either through it or not, is what the sounds in the recipes need (a
sub under the filter, a saw through it) and keeps the panel to one filter
column; a second filter with routing is a Later line. Oscillators and the
filter run every frame, and every control value is worked out every 16 frames
of the voice's own time, so a render is a function of the patch, the note and
the frames since the note started, the same bytes twice and in any block size,
at a cost a few voices leave unnoticed. An entry on an envelope's field is
taken when the note starts and holds for the note, since an attack cannot
change under way; an LFO's fields are not matrix targets, since a free LFO is
shared by every voice and a per-voice amount would contradict that. The amp
release reaches exactly nothing at `release_ms`, 60 dB down and then zero, so
a voice ends without a step. Every voice's state is allocated when a renderer
is built, 16 voices and 16 ringing out whatever the patch asks for, so a
change of `voices` glides like a knob; a change of wave, filter mode, slope or
routing, or of the parts a patch has, swaps through the dip, since it would
jump the waveform. Patch names with spaces, the other open question, wait for
patches as files.

**D72. A patch's file is the slug of its name, saving names the song's patch, and a patch file is validated as a song's synth is.**

Decided on October 4, 2026, in building the Synth's patches
([features/synth.md](features/synth.md)). D71 left open how a patch with
spaces in its name is named. A patch's name is free text, Soft Pad, written
inside the file as `name`; its file is the slug of that name, `soft-pad.yaml`,
lower-case letters, digits and hyphens, so the terminal, the browser and the
Finder agree on one file for one name and `daw patch load` takes either.
`patch save` writes the file and also sets the song's `patch` field to the
name, as one undo step, because the song should say where its sound came from
and the panel's header shows that name; `patch load` and `synth add --patch`
set it too, and nothing reads it. A saved name is written over only with
`--replace`, which keeps the file's description and tags unless new ones are
given; a saved patch of a factory name shadows the factory one without
asking, since the factory patch is still in the binary. The mapping in a file
is validated by the same code that validates a synth in a song, wrapped as an
instrument, so a wrong wave or a dangling matrix entry is refused with the
field named either way, and the file is written as the song saves the
mapping, defaults left out. `saved_by` is the command's origin, user or
agent, and `saved_at` a UTC timestamp; a person's name is not kept. The
factory patches are written for the Synth as item 1 built it: four detuned
saws stand in for unison, and they carry no effects, until item 4 adds both.

**D73. A previewed note plays a Synth only, a standing stream is cut by nothing, a key's note is a beat long, and the header steps through the browser's list.**

Decided on October 4, 2026, in building the Synth's panel
([features/synth.md](features/synth.md)). `note.preview` plays a note through
a MIDI track's Synth and its chain and refuses a Sampler: a Sampler's note
needs audio prepared for its pitch, which the compiled song holds only for
the notes it plays, and preparing it on the way is work of its own, a Later
line. Previewed notes are kept apart from the song's in the Synth, so a seek
or a stop leaves them and a program swap carries them, and the one thing the
engine changed for them is that the end fade and the gate at the song's end
apply only while the transport rolls: a standing stream used to be forced to
silence past the end, which would have cut a note previewed there, and
nothing but silence was lost by letting tails ring as they do after any stop.
A key plays a note of one beat, its velocity how far down the key it is
pressed, since the host's command has a length and no note-off; holding and
letting go is a Later line with the MIDI keyboard. ◂ ▸ in the header step
through the patches as the browser lists them, Factory then Mine, and Save…
writes over a patch only when the sound came from one of the person's own,
asking for a name otherwise, so a factory patch is never overwritten by
accident and nothing is written without a name the person gave. The detail
panel's height is kept in the app's defaults, not in the song, as the window's
size is.

**D74. Unison is on the oscillator at the level of one, a wavetable is one cycle from a built-in table or a sample, and the patch's effects are the song's effect kinds on the synth's path.**

Decided on October 4, 2026, in building the Synth's last item
([features/synth.md](features/synth.md)). Unison is three fields of the
oscillator, `unison`, `unison_detune_cents` and `unison_width_percent`,
rather than a layer of its own: the copies are spread evenly in detune and
across the field either side of the oscillator's pan, each at its own random
phase from the seed unless `phase` is given, and summed at 1/√n each, so
turning unison up thickens a sound without making it louder and the Supersaw
patch is one saw in seven copies with a macro on its detune. The detune is a
matrix target in cents and a lane target; the count is structural. A
wavetable is one cycle, read bandlimited from a stack of levels with fewer
harmonics each, so it never aliases, and `table` names a built-in table or a
sample of the project: the project already knows its samples by ID, a cycle
drawn or cut from any sound is then a wave, and a patch that names a sample
loads only into a song that has it, which validation says with the
oscillator named; a table of several frames with a sweepable position stays
a Later line. The patch's effects are the song's effect kinds, not a
second kind of device: they are edited with the effect commands on
`tracks.T.instrument.synth`, automated as `instrument.effects.REF.FIELD`
from the track's lanes, which follow a move and go with a removal as an
owner's do, and run on the sum of the voices before the inserts, with a
limiter's look-ahead counted into the track's latency so the track lands
where it does either way; a compressor there has no sidechain, since the
chain hears only the synth. Chorus and saturation were built as effect kinds
anywhere, since a chorus on a return or a saturation on a drum track are as
wanted as either in a patch, and the schema and `daw describe effects` gained
them; a clipper and a phaser stay Later lines. The panel draws the patch's
chain as columns after the matrix and adds to it from the + menu; an effect
dragged from the browser still lands on the track's inserts.

**D75. The backlog is ordered by how much an item helps the person and the agent make good music; packaging waits.**

Chosen by the person on October 4, 2026. Next had been the road to a
distributable app: an app that stands alone, the `.aaw` file type, the
workspace, the agent panel, other Macs and sign-in. The person builds the app
from this checkout and opens it without trouble, and what limits the work is
making music with it, by hand in the app and by the agent through `daw`. So Next
is ordered by that, and the four packaging items wait under Later › Shipping.
The workspace and the agent panel stay in Next, lower, since `SONG.md`, taste
and a conversation in the window help the two work together.

The order, from an agent's trial song made the same day and the person's
editing in the app: first what the agent needs on every song and costs little
(a short `describe` and errors that name the fix, the arrangement map, musical
checks); then what the person does most by hand (the piano roll, selecting
several things); then working at the scale of a song (ranges of bars, clips
that loop); then playing and working together (a MIDI keyboard, listening
markers, a `daw init` project in the app, turns with A/B); then the mix
(a clipper and a utility, groups, a reference); then choosing and shaping
material (sound descriptors, note transformations and chords, slicing, key
detection, presets); then the workspace, the agent panel, stem separation and
generated audio. Verify is ordered too, with the Synth, the piano roll and the
Sampler first, since everything after them is built on how they sound.

**D76. `daw describe` is short by default, its field lines are text inside the JSON, and the hint for an unknown field comes before the model's message.**

Made while building [Learning the CLI quickly](features/cli-discovery.md) on
October 4, 2026, settling the design's open questions without the person.
A topic prints what its fields mean and a line for each field, its path, what
it takes and its default, generated from the schema; `--schema` prints the JSON
Schema as before, and `daw describe` without a topic lists the topics rather
than printing the whole song's schema. The lines are strings in the JSON result
rather than JSON objects of their own: an agent reads them as easily, and they
are a third of the size. The Synth's fields are listed by their paths in the
patch, which every Synth command takes, rather than from the top of the song.
A key the song has no field for is answered with the nearest field, made from
the schema at the host, before pydantic's message, which stays as it is so the
model's errors remain those of the Python model they were held to. The Python
commands keep argparse's help until they move to Rust; only their closed pipe
is handled now.

**D77. The arrangement map prints its text as a list of lines beside each letter's clips, and a pattern clip's repeats share its letter.**

Made while building [the arrangement map](features/arrangement-map.md) on
October 4, 2026, settling the design's open questions without the person. The
design had the map's text as one string beside structured rows of cells. A
string printed as JSON is one line of `\n`s whose columns do not line up, and
`jq` is not on every Mac, so `map` is a list of lines, which printed JSON keeps
aligned; the rows' cells would repeat the grid, so they were left out. What the
agent needs from the JSON to act is a clip's reference, so `clips` names each
letter's clips, space separated in a line a letter rather than as objects, which
on a 64-bar song would have been several times the map. A pattern clip's
repeats are the same music again and share its letter, as copies of a phrase
do. `#` marks clips that sound at once, not clips that only meet in a cell. The
legend counts places past six, and the same music on two tracks has one letter
so that a doubled part shows. Whether `inspect` lists clips in the order they
play was settled by D76.

**D78. `daw check`'s warnings are objects with a code and a list of paths; overlaps are information rather than ignorable, and duplicates chain.**

Made while building [musical checks](features/musical-checks.md) on October 4,
2026, settling the design's open questions without the person. A warning is
`{code, level, message, paths, at}`: the design gave it one `path`, but a
stacked pair and two crowded tracks are about two things, so `paths` is always
a list. `level` is `warning`, or `info` for `clips-overlap`, which is
sometimes deliberate; rather than a `check_ignore` field in the song, the agent
reads information as information and moves on, and the song holds nothing for
the checks. `clip-after-end` was dropped: validation already refuses a clip
that starts at or after the song's end, or a pattern or note clip that runs
past it, so `song-ends-inside` is about audio clips, whose length Python passes
in from each file. `register-crowded` counts notes at their written pitch, and
leaves out a drum map's notes and pattern hits without a note, so a kick on C2
is not a bass part; an instrument's own range stays unknown. Warnings that
could run to hundreds (`note-retriggered`, `pad-far-from-root`,
`note-below-hearing`) are one a clip, pad or track, with the count and the
first beat. `daw render` does not run the checks. `clip duplicate` without
`--at` places a copy past the run of clips that follow the original, and
`--times N` makes a row as one step; `--at` with `--times` lays them from that
beat whatever is there.

**D79. The piano roll previews notes through either instrument, Option-drag copies and ⌘ takes things off the grid, and grids are named as note values.**

Made while building [the piano roll for writing by hand](features/piano-roll.md)
on October 5, 2026, without the person, as the backlog item asked: "as in
Ableton". It revises the part of D64 that Option takes notes off the grid. In
Ableton, Option-drag copies and ⌘ held during a drag bypasses the grid; the
item asked for the first, and keeping Option for off the grid as well would
have made one key mean two things. So ⌘ is off the grid everywhere the app
had Option for it, the timeline, the pattern editor, automation and dropped
files included, so that one key means one thing across the window. Option
still shows every row's lanes and changes whether a point holds, where
nothing is dragged. Option is read while the drag goes on and at its end,
as the Finder does, rather than only at the press.

A preview is heard on a Sampler as on a Synth, since most MIDI tracks made by
dropping a sample have one. The host prepares the Sampler's voice, as an edit
prepares a clip's notes, and the player hands each replaced voice back to be
freed, keeping the audio thread free of allocation. Sixteen slots for those
notes across the tracks: a drag across pitches with a long one-shot sample
overlaps, as Ableton's does, and past sixteen the oldest stops. A preview
lasts the note's length within an eighth of a beat and a beat, rather than
while the mouse is held, which waits for the MIDI keyboard; while a chord is
moved, only the grabbed note sounds, so a drag is not a smear of chords.

The velocity lane changes the selected notes by the same amount, as Ableton's
does, each held within 1 to 127. Zoom up and down is Option-scroll, Ableton's
key for it in the MIDI editor; ⌘-scroll and pinch stay across. A note's start
is its left five points when the note is at least fifteen wide, so a short note
can still be moved. The grid and the pattern's step are named as note values
(1/16, 1/8T, 1 Bar) and still written in beats in the song, so no song changes;
the piano roll's grid gained 1 Bar and 1/2.

**D80. A rectangle in the clear selects, Option-drag copies everywhere a thing is dragged, and several events and points are selected as notes are.**

Made while building [selecting several things in the app](features/selection.md)
on October 6, 2026, without the person, settling what the backlog item left
open. It revises the part of D46 that one event is selected at a time.

A drag from the clear of the timeline selects the clips a rectangle touches,
as Logic's does, rather than making a time selection across the tracks, as
Ableton's does. Every other editor of the app already selects by a
rectangle (the piano roll since D64), and a time range is what [editing a
range of bars](features/bar-ranges.md) is about, which will take a gesture
of its own, such as a drag in the ruler under the loop strip. The press
still sets the start position, so a click in the clear means what it did.

Option-drag copies clips and pattern events as D79 made it copy notes, so
that one key means one thing wherever something is dragged; a press with
Option on an edge copies instead of trimming, as on a note's end. A copy
of a pattern clip plays the same pattern, as ⌘D's does, since a copy that
owns its pattern is one Own Copy away and a shared pattern is how the song
repeats a part.

Events and automation points are selected as notes are: Shift-click, a
rectangle from the clear, and a drag that moves every selected one by the
grabbed one. In a row of steps a press still paints, so a rectangle starts
only in a row of events. Points of several lanes move together in time and
by the same share of each lane's height, which on a frequency's lane is a
ratio, and they stop together at a lane's edge rather than each at its own,
so that the shape of what was drawn is kept; the time each can move is
bounded by the points of its lane that stay. Pasted events go where the
pattern was last clicked in the clear, as notes do in the piano roll. Events
in pitch move only where they have one, so a selection across a row of notes
and a row of hits can be moved up an octave. The edits that took one event
or point now take a list, so the app has one edit for one or several, and
each is one undo step named for how many it changed.


**D81. The app and `daw` are held to the same reach, and what the app lacks goes into the backlog by how often the person meets it.**

Chosen by the person on October 6, 2026, from a review of what each side can
do. Because every edit is a command to one host, the two are peers for the
song itself. What the app alone can do is hearing, the metronome, setting the
selection, and previewing a Sampler's note or a library sample through the
speakers; the CLI reads the selection and the metronome and lacks a command
for the other three, which the host already has. What `daw` alone can do is of
two kinds: the agent's tooling (`inspect`, `map`, `check`, `listen`,
`compare`, `joins`, `timeline`, measured search), which is by design; and
editing the app never got a control for: rendering and exporting the mix,
sections, the session's length, master gain and end fade, a pad's settings on
a pattern track, `audio crossfade` and `audio cut`, a send's pre-fader switch,
a clip's velocity scale, an event's transpose, an effect's ID, and a lane
segment's shape.

The second kind is placed in the backlog by how often the person meets it,
under D75's order. Exporting goes third in Next, after the two song-scale
items already designed, because the person meets it on every song and the app
cannot finish one: the only export is a MIDI clip. Sections and the length go
after the MIDI keyboard and listening markers, since those are how the person
and the agent make music together and a section is a label. A pad's settings
go after presets, since the Sampler's panel has most of them and `daw pad
set` the rest. The remaining fields and `daw check`'s warnings in the window
are Later lines, as are the CLI's metronome and Sampler preview, which the
agent cannot hear the result of. Crossfades and the segment's shape in the
app were Later lines already, and a range cut is part of editing a range of
bars.

The export is built over `daw export` rather than beside it, so there is one
export and the app shows what the command measured; whether a render should
run inside the host, so that a bundle without Python can export, is its open
question.
