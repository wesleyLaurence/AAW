# Decision log

Settled choices from the design conversation, with rationale, so a future session does not relitigate them. Revisit only with a new reason.

**D1. Offline, deterministic rendering. No realtime engine in version one.**
Realtime is where nearly all DAW complexity lives, and an agent iterates by edit, render, analyze. Determinism lets the agent trust its own loop and lets git act as a version history of sound. Realtime playback for the person comes later as a player over rendered audio.

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
numpy and scipy for DSP, with the document format and CLI as the contract. A Rust port of the render hot path is the planned escape hatch if the performance target is missed.

**D19. Human in the loop by default, autonomous by capability.**
The person drives with ideas, material, references, and taste, and can interrupt at any point. The agent must also be able to finish a song alone against the brief, rules, and a definition of done.
