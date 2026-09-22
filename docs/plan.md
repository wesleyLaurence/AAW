# Build plan

Status: nothing built. Phases are ordered so that each ends with something a person can hear, and so that the sample-based workflow (the stated priority) arrives first. Each phase has an exit criterion phrased as a demo a fresh session can run.

The stable contract across all phases is the project document format, the CLI verbs, and the core library API that the CLI and the workspace daemon share. Engine internals may change freely behind them.

The agentic loop is the product and the numbered phases are the priority. The workspace UI is the end state and runs as a separate, lower-priority track (below) that starts after phase 0. Four small requirements in the numbered phases exist for its sake and are worth having regardless: canonical form, the block-processing device shape, region rendering, and the library layering.

## Phase 0 — Skeleton and first sound

Goal: prove the document-and-render model end to end with the smallest possible surface.

- Package layout: the `daw` core library separate from the CLI entry point, so the daemon can share it later.
- Project directory layout, `song.yaml` parser with real-unit values and bar.beat positions.
- Canonical serializer, `daw fmt`, atomic writes with re-read before modify.
- `daw check` with the first lint rules: unknown ids, missing files, out-of-range values, non-canonical formatting.
- Renderer that places audio clips on a timeline with gain, pan, fades, and sums tracks to a master. No devices yet. Block-processing from the start, so devices slot into the same shape.
- Render cache keyed by content hash. Draft and final modes.
- `daw render mix`, `daw render <track>`, `daw render stems`, and `--region` on each.
- `daw checkpoint`, `daw log`, and the `renders/` naming conventions.
- Deterministic-output tests.

Exit: hand-write a project with three audio clips from a folder of samples, render a mix and stems, change a gain, re-render, and confirm only the changed track re-rendered. `daw fmt` is a no-op on a file it has already written.

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

Before this phase begins, settle open question 7, the device language. Effects are the bulk of device code and the thing that would otherwise be written twice if a realtime engine is ever wanted.

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

## Workspace track — U0 to U4

Goal: the end state, a visual DAW where a person adjusts tracks, devices, and knobs and hears the result while the agent works in the same project. See spec section 16. This track starts after phase 0, runs beside the numbered phases, and yields to them. Each stage ends with something usable and is small enough to stop after.

- **U0 — Watch.** `daw serve` watching the project directory. A browser-tab frontend showing the arrangement, waveforms from the render cache, sections, transport and playhead, the journal and brief. Redraws as the agent edits and renders. Exit: start the agent on a task in one terminal, open the workspace, and watch tracks and waveforms appear as it works, playing back the latest mix at any point.
- **U1 — Reach in.** Mute, solo, volume, pan, and sends applied instantly in the player over stems; move, trim, and delete clips written to the document through the canonical serializer, with the daemon re-rendering. Exit: nudge a clip and lower a track by hand while the agent is between tasks, then have the agent notice the edit in the diff and carry on.
- **U2 — Panels and editors.** Device panels generated from `daw describe` output, a step grid and piano roll that read and write the pattern notation, automation lanes, the perception report and images for the current render, git history as a panel.
- **U3 — Responsive.** Region rendering around the playhead on every change, audio swapped in the player. Exit: drag a filter cutoff and hear it follow within a few hundred milliseconds, then confirm the agent's `daw listen` on the same document matches what was heard.
- **U4 — Realtime engine.** Only if live instrument playing becomes a goal, and only if the device language decision in phase 3 made it possible. Wrap the frontend in Tauri so the engine can live in-process.

## Later

Tempo and time signature changes, warp markers for drifting material, FM and wavetable synthesis, multiband compression and transient shaping, an optional audio-input model critic, Ableton import and export, a Rust port of the render hot path if the performance target is missed, third-party plugin hosting if a concrete need cannot be met in-house, embedding the agent in the workspace through the agent SDK.

## Risks

- **Perception quality.** If the reports are misleading, the agent iterates confidently in the wrong direction. Mitigation: build phase 4 against known-bad test mixes with planted problems, and keep the human as the taste layer.
- **Render speed.** Python may miss the sub-second draft target once effects and modulation land. Mitigation: vectorize aggressively, cache per track, and plan the Rust port as a known escape hatch behind an unchanged contract.
- **Document size.** A long song with many clips and automation may exceed what fits comfortably in context. Mitigation: patterns and patches in separate files, sections as the unit of work, and tools that print only the relevant slice of the document.
- **Pattern notation ergonomics.** Untested with real models. Mitigation: prototype both notations early in phase 1 and measure how reliably the agent reads and edits them.
- **Scope creep toward a full DAW.** Every feature should be justified by an agent workflow, not by Ableton parity. The workspace track is the most likely source of this: if it starts growing features that only make sense for composing by hand, it has drifted from the product.
- **Two sources of truth.** If the workspace ever caches project state that is not in the file, the agent and the person will disagree about what the song is. Mitigation: the workspace reads and writes the document through the same core library and canonical serializer as the CLI, and holds nothing else.
- **Two implementations of a device.** If the workspace ever previews a device with anything other than the real renderer, the person and the agent hear different things and the perception loop stops being trustworthy. Mitigation: playback is rendered audio only, and the device language is decided before phase 3 so no device is written twice.

## Open questions

1. Concrete document syntax: YAML with unit-bearing strings, or a small purpose-built format that is stricter and terser? Either must have one canonical form, which a purpose-built format gets by construction and YAML gets only with a fixed ruamel style.
2. Melodic pattern notation: the column form in the spec, a compact inline string form, or both?
3. Which local embedding model for sample search, and how large an index it should handle?
4. How much of the journal and brief the agent reads by default at session start, and how they are trimmed as they grow?
5. Whether draft renders should downsample or only skip expensive processing.
6. The CLI name.
7. ~~Device language~~ Settled as D25: Python throughout in the block-processing shape, accepting that a realtime engine would require a rewrite.
8. Workspace shell: a browser tab served by the daemon, Tauri, or native macOS? Browser tab first is the working assumption.
