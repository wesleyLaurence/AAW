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
The end state is a person and an agent sharing one song: click a position, press space and hear each edit right away. Rendering the whole song offline took 4 to 34 seconds on local projects, far too slow for that. A callback-driven engine plays from any beat and applies edits during playback. Export runs the same program with fixed blocks as fast as the machine allows, so it stays deterministic, and the agent's render-and-analyze loop and git as a history of sound are unchanged. Settled with the plan in [Rust-Swift-Update.md](Rust-Swift-Update.md).

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
Recorded with M3 of [Rust-Swift-Update.md](Rust-Swift-Update.md). A host writes canonical `song.yaml` after each command and before replying, so anything reading the file, including the Python tools, sees the change at once; a debounce arrives with drag gestures in M5, where it pays. Every edit recompiles the song, reusing prepared sample audio, and the audio thread crossfades into the new version over 5 ms at the same beat, which is the smoothing D37 asks for; sending parameter changes without recompiling is a later optimization. Stops, locates and loop jumps let the old position ring out for 5 ms without starting new hits, and voices picked up mid-sample fade in over 5 ms while hits at the new position play in full. Handles are written `@N` and last for one host session. Headless commands address objects by index or ID, and they have no undo history or change log, because both are ephemeral host state (D36). A file edited outside the host that does not validate pauses edits until it is fixed, rather than being overwritten. An edit that only reorders keys, such as pads, is a change although `project_sha256` ignores key order.

**D42. The app is a Swift package around an embedded host, which pushes each revision to it.**
Recorded with M4 of [Rust-Swift-Update.md](Rust-Swift-Update.md). The app runs the session host on a thread inside its process and reaches it through UniFFI; the same host answers the socket, so the app and the agent's `daw` commands share one song, one transport and one history. After every change the host hands the app the whole arrangement to draw, with the objects that differ from the revision before, both computed in Rust. The plan had the app re-read only the affected objects, but an arrangement is small, a pushed snapshot cannot be read half-updated, and Swift needs no knowledge of the song to stay current (D39); revisit if it shows in a profile. The app's host compiles each revision as it lands, so space plays at once, and the playhead is read from the audio thread rather than asked of the host, which may be busy compiling. Clicking the timeline sets the start position, and while playing it also jumps playback there. The app is a Swift package built by `apps/mac/build.sh`, not a hand-made Xcode project: the manifest is plain text an agent can edit and review, `swift build` and `swift test` run from a terminal, and Xcode opens the package as it is. The app delegate, windows and menus are AppKit, the bars and panels inside a window are SwiftUI, and the timeline is one AppKit view drawn with Core Graphics. The bundle is signed ad hoc and links Homebrew's libsndfile, which makes it a local build; packaging is M9. The app takes scripted clicks and key presses on its command line and can write a picture of its window, so an agent can check what the app does without anyone at the screen.
