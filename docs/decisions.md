# Decision log

Settled choices from the design conversation, with rationale, so a future session does not relitigate them. Revisit only with a new reason.

**D1. Offline, deterministic rendering. No realtime audio engine in version one.**
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

**D18. Python first, behind a stable contract.**
numpy and scipy for DSP, with the document format and CLI as the contract. A Rust port of the render hot path is the planned escape hatch if the performance target is missed. Devices are written in a block-processing shape from the start so that the device language can be revisited before phase 3 without changing their interface (D22).

**D19. Human in the loop by default, autonomous by capability.**
The person drives with ideas, material, references, and taste, and can interrupt at any point. The agent must also be able to finish a song alone against the brief, rules, and a definition of done.

**D20. The agentic loop is the product. The workspace UI is the end state, designed in from the start and built second.**
DAW UIs already exist. An agent that works for hours with a DAW toolkit does not, and that is what is unique here. But the end state is a shared workspace where a person adjusts tracks, devices, and knobs by hand and hears the result while the agent works in the same project. The numbered phases come first; the workspace runs as a parallel, lower-priority track after phase 0. The early build carries four small requirements on its behalf, each worth having anyway: canonical form (D23), single device implementation (D22), region rendering, and the library layering (D24).

**D21. The workspace is a second client of the document, never a second source of truth.**
It reads and writes `song.yaml` through the same core library and serializer the agent's tools use, and holds no state the file does not hold. Watching the agent work is file watching; the agent's renders appear as files in `renders/`. Hand edits made in the workspace reach the agent as file changes and git diffs, and they are already a preference signal under D15. There is no integration layer between the agent and the workspace beyond the project directory. The agent stays in the terminal; the workspace is a window, not a chat client.

**D22. Every device is implemented exactly once. The person and the agent always hear the same audio.**
If the workspace previewed a device with anything other than the real renderer, the person would tune by ear against one sound and the agent would analyze another, and the perception loop would stop being trustworthy. So workspace playback is rendered audio only, mute, solo, volume, and pan may be applied in the player because they are exact, and every other change goes through the renderer. Approximations such as browser audio nodes as a drag-time preview are forbidden. Consequence: if a true realtime engine is ever wanted, the devices must be in a language that runs both offline and in a callback, which is why the device language is decided before phase 3 rather than after.

**D23. The document has one canonical form and every writer produces it.**
Two writers rewriting one file need identical formatting or every diff fills with noise the agent has to read past. `daw fmt` produces canonical form, `daw check` rejects anything else, writes are atomic, and every writer re-reads before modifying. This constrains the syntax choice in plan open question 1.

**D24. One core library; the CLI and the workspace daemon are thin shells over it. The daemon is never required.**
The `daw` package holds the document model, renderer, devices, perception, and library index. The CLI and `daw serve` contain no logic of their own. When a daemon is running the CLI hands renders to it so both clients share one cache; when none is running the CLI does the work itself and the agent notices no difference. This is what keeps the workspace from becoming a second implementation of anything.

**D25. Devices are written in Python (numpy/scipy), in the block-processing shape.**
Settles plan open question 7. Offline rendering is the product (D1) and a realtime engine is still hypothetical, so one Python implementation serves mixes, stems and previews. Devices carry explicit state, declare latency and must produce identical output for any block partition, which keeps a later port to a compiled kernel mechanical. Accepted risk: a realtime engine would require rewriting the devices.

**D26. Stems are post-insert and pre-master-effects.**
Each stem is a track after its inserts, gain, pan, mute and solo, master gain and end fade, and before master effects. Without master effects the stems sum to the mix; with a nonlinear master chain they cannot, and the render report says so in `stems_sum_to_mix`. Sidechain keys are taken after the source's inserts and before its fader, so fader and mute changes on the source never change ducking.

**D27. Shared space comes from return buses, and returns are stems.**
Tracks send to `returns`, post-fader by default (after gain and pan) or pre-fader (after inserts). One reverb shared by several tracks is how producers glue a mix, and it keeps the device count and render cost down. A muted or solo-muted track sends nothing, but returns are never solo-muted, so soloing a snare still plays its reverb. Track stems stay dry and each return renders its own stem, so D26's reconstruction still holds with track and return stems together. Returns share the ID namespace with tracks because both name stems. Sidechains key only from tracks, and returns cannot send, which keeps render order simple: tracks by sidechain dependency, then returns. Groups and return-to-return sends wait for a need.

**D28. Reverb is seeded convolution with a synthetic impulse response.**
A generated noise tail with frequency-dependent exponential decay is reproducible from its parameters and seed (D17), convolves exactly regardless of block partition through fixed internal partitions with compensated latency (D25), and runs fast with numpy FFTs. A feedback-delay-network reverb would need per-sample recursion that is slow in numpy and hard to make partition-exact. Convolution also leaves room to load recorded impulse responses as samples later. Accepted limits: the input is summed to mono, and there is no modulation or early-reflection model.

**D29. Automation lanes override static values and cover an allowlist of continuous parameters.**
A lane is `{param, points}` on a track, return or the master and replaces the static value for the whole song, holding its first and last values outside its points, the way arrangement automation behaves in Ableton. Only continuous parameters whose change needs no device rebuild are automatable: levels, pans, send levels, filter cutoff, EQ band frequency, gain and q, compressor threshold and makeup, delay feedback and mix, reverb mix. Frequencies and q interpolate in log, everything else linearly in its own unit. Lanes are evaluated at absolute timeline frames and devices offset them by upstream latency, so changes land on the beat and output stays independent of block partition (D25). Filter and EQ coefficients update every 64 frames from the start of the song, a fixed rate rather than one tied to the caller's blocks. Nothing is smoothed: a written jump is a jump. Unautomated devices keep their original code path, so existing projects render bit-identically. Modulation and automating switches wait for a need.
