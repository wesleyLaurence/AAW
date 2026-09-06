# Build plan

Status: nothing built. Phases are ordered so that each ends with something a person can hear, and so that the sample-based workflow (the stated priority) arrives first. Each phase has an exit criterion phrased as a demo a fresh session can run.

The stable contract across all phases is the project document format and the CLI verbs. Engine internals may change freely behind them.

## Phase 0 — Skeleton and first sound

Goal: prove the document-and-render model end to end with the smallest possible surface.

- Project directory layout, `song.yaml` parser with real-unit values and bar.beat positions.
- `daw check` with the first lint rules: unknown ids, missing files, out-of-range values.
- Renderer that places audio clips on a timeline with gain, pan, fades, and sums tracks to a master. No devices yet.
- Render cache keyed by content hash. Draft and final modes.
- `daw render mix`, `daw render <track>`, `daw render stems`.
- `daw checkpoint`, `daw log`, and the `renders/` naming conventions.
- Deterministic-output tests.

Exit: hand-write a project with three audio clips from a folder of samples, render a mix and stems, change a gain, re-render, and confirm only the changed track re-rendered.

## Phase 1 — Sampler, drum rack, patterns

Goal: make a beat from the person's samples using MIDI.

- Sampler and drum rack devices with text patches and self-descriptions (`daw describe`).
- Step notation and note notation parsers, pattern placement with `repeat`, session swing with clip override.
- Groove and velocity handling, choke groups, per-pad output routing.
- Sections in the document and section-targeted rendering.
- Standard MIDI file import and export.

Exit: from a folder of one-shots, build a drum rack patch, write a two-bar step pattern and a bassline in note notation on a sampler, arrange intro, verse, and drop, and render the mix and stems.

## Phase 2 — Library, analysis, slicing, stretching

Goal: the sample workflow at full strength.

- Library directory and index with the per-sample analysis fields from the spec.
- `daw import` with the tempo policy, `daw analyze` for tempo, downbeat, pitch, and one-shot versus loop.
- Embedding-based `daw search` with filters.
- `daw slice` producing samples, a drum rack patch, and a replay pattern.
- `daw stretch` with preserve-pitch and repitch modes, and clip-level warp fields honored by the renderer.
- `daw separate` via Demucs.
- `daw promote` with provenance.

Exit: hand the agent a full song, have it detect tempo, separate stems, slice the drums into a rack, chop a vocal phrase, and build a beat around it at a tempo the person chose.

## Phase 3 — Effects, routing, automation

Goal: mixing.

- Effects: utility, eq, compressor with sidechain, limiter, reverb, delay, filter, saturation, chorus, phaser.
- Returns, sends pre and post fader, groups, sidechain inputs, DAG validation.
- Automation envelopes on any parameter with curve types.
- Master chain and `daw export` presets with a loudness target.

Exit: sidechain the bass to the kick, send drums and vocal to a shared reverb, automate a filter sweep into the drop, master to a streaming loudness target, and export.

## Phase 4 — Perception

Goal: the agent can evaluate its own work.

- Descriptors per render, section, and track.
- `daw listen` report format, `daw compare` diffs, reference comparison against a profile reference set.
- Masking analysis between track pairs, structure analysis across sections.
- Spectrogram and overview images drawn for reading.

Exit: render a mix with a deliberate low-end masking problem and have the agent find it, fix it with eq and sidechain, and show the diff in the compare output.

## Phase 5 — Synth and modulation

Goal: sound design from scratch and resampling.

- Subtractive synth with the modulation system.
- Resampling loop: render a synth sound, promote it, load it into a drum rack.
- Seeded randomness across humanize, random modulation, and round robin.

Exit: the agent designs a kick, a sub bass, and a pad from nothing, promotes the kick, and uses it in a drum rack alongside sliced samples.

## Phase 6 — Intent, memory, skills, autonomous mode

Goal: long-horizon work with a person in the loop.

- Profile directory: rules, preferences with evidence and decay, reference set, house kit, workflow preferences.
- Per-project brief and the interruption behaviors (promotions, dislikes recorded with section and reason).
- Harness instruction file for Claude Code and Codex pointing at the DAW's own files.
- Skills: slice a break, sidechain, gain stage, master to target, arrange a song, finish a song, definition of done.
- End-of-session summary of what was learned.

Exit: with a written brief and a folder of samples, "make a song and don't stop until it is done" produces an exported, mastered song with stems, a journal, and a chain of checkpoints, and the person can interrupt midway with "keep that sound" and see it promoted.

## Later

Tempo and time signature changes, warp markers for drifting material, FM and wavetable synthesis, multiband compression and transient shaping, realtime playback for the person, an optional audio-input model critic, Ableton import and export, a Rust port of the render hot path if the performance target is missed, third-party plugin hosting if a concrete need cannot be met in-house.

## Risks

- **Perception quality.** If the reports are misleading, the agent iterates confidently in the wrong direction. Mitigation: build phase 4 against known-bad test mixes with planted problems, and keep the human as the taste layer.
- **Render speed.** Python may miss the sub-second draft target once effects and modulation land. Mitigation: vectorize aggressively, cache per track, and plan the Rust port as a known escape hatch behind an unchanged contract.
- **Document size.** A long song with many clips and automation may exceed what fits comfortably in context. Mitigation: patterns and patches in separate files, sections as the unit of work, and tools that print only the relevant slice of the document.
- **Pattern notation ergonomics.** Untested with real models. Mitigation: prototype both notations early in phase 1 and measure how reliably the agent reads and edits them.
- **Scope creep toward a full DAW.** Every feature should be justified by an agent workflow, not by Ableton parity.

## Open questions

1. Concrete document syntax: YAML with unit-bearing strings, or a small purpose-built format that is stricter and terser?
2. Melodic pattern notation: the column form in the spec, a compact inline string form, or both?
3. Which local embedding model for sample search, and how large an index it should handle?
4. How much of the journal and brief the agent reads by default at session start, and how they are trimmed as they grow?
5. Whether draft renders should downsample or only skip expensive processing.
6. The CLI name.
